use rand::Rng;
use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::Error as SerdeError;
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use std::{thread, time};

#[derive(Debug)]
pub enum HeartbeatError {
    Io(io::Error),
    Serde(SerdeError),
    WouldBlock,
    Poison(String),
    Other(String),
}

impl fmt::Display for HeartbeatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeartbeatError::Io(err) => write!(f, "IO error: {}", err),
            HeartbeatError::Serde(err) => write!(f, "Serialization error: {}", err),
            HeartbeatError::WouldBlock => write!(f, "Operation would block"),
            HeartbeatError::Poison(err) => write!(f, "Poison error: {}", err),
            HeartbeatError::Other(err) => write!(f, "Other error: {}", err),
        }
    }
}

impl std::error::Error for HeartbeatError {}

impl From<io::Error> for HeartbeatError {
    fn from(err: io::Error) -> HeartbeatError {
        if err.kind() == io::ErrorKind::WouldBlock {
            HeartbeatError::WouldBlock
        } else {
            HeartbeatError::Io(err)
        }
    }
}

impl From<SerdeError> for HeartbeatError {
    fn from(err: SerdeError) -> HeartbeatError {
        HeartbeatError::Serde(err)
    }
}

impl From<String> for HeartbeatError {
    fn from(err: String) -> HeartbeatError {
        HeartbeatError::Other(err)
    }
}

impl<T> From<PoisonError<MutexGuard<'_, T>>> for HeartbeatError {
    fn from(err: PoisonError<MutexGuard<'_, T>>) -> HeartbeatError {
        HeartbeatError::Poison(err.to_string())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Heartbeat {
    id: String,
    address: String,
    generation: u64,
    version: u64,
    timestamp: u64,
}

impl Default for Heartbeat {
    fn default() -> Self {
        Heartbeat {
            id: "".to_string(),
            address: "".to_string(),
            generation: 0,
            version: 0,
            timestamp: time::SystemTime::now()
                .duration_since(time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
}

struct NodeHeartbeatData {
    heartbeat: Heartbeat,
    received_count: u64,
    // node_addresses_sent_to: Vec<String>,
}

struct Storage {
    data: Arc<Mutex<HashMap<String, NodeHeartbeatData>>>,
}

impl Storage {
    fn get_node_heartbeat(&self, id: &str) -> Result<Heartbeat, HeartbeatError> {
        let locked_data = self.data.lock()?;
        match locked_data.get(id) {
            Some(d) => return Ok(d.heartbeat.clone()),
            None => {
                return Err(HeartbeatError::Other(
                    "Unable to get heartbeat data".to_string(),
                ))
            }
        };
    }

    fn select_n_random_addresses(&self, n: usize) -> Result<Vec<String>, HeartbeatError> {
        let locked_data = self.data.lock()?;
        let addresses: Vec<String> = locked_data
            .iter()
            .map(|(_, v)| v.heartbeat.address.clone())
            .collect();
        let selected_addresses = select_random_n_strings(addresses, n);
        return Ok(selected_addresses);
    }

    fn insert(&self, heartbeat: Heartbeat) -> Result<u64, HeartbeatError> {
        let mut locked_data = self.data.lock()?;

        if locked_data.get(&heartbeat.id).is_none() {
            locked_data.insert(k, v)
            return Ok(0)
        }


        if heartbeat.generation > current.heartbeat.generation
            || heartbeat.version > current.heartbeat.version
        {}

        // locked_data
        //     .insert(
        //         heartbeat.id.to_string(),
        //         NodeHeartbeatData {
        //             heartbeat,
        //             received_count: 0,
        //         },
        //     )
        //     .ok_or(|e| HeartbeatError::Other("Failed to insert new data".to_string()));
        //

        // let mut heartbeat_data = match locked_data.get_mut(&heartbeat.id) {
        //     Some(current) => {
        //         if heartbeat.generation > current.heartbeat.generation
        //             || heartbeat.version > current.heartbeat.version
        //         {
        //             current.received_count = 0;
        //         }
        //         current.received_count = 0;
        //         current
        //     }
        //     None => locked_data
        //         .insert(
        //             heartbeat.id.to_string(),
        //             NodeHeartbeatData {
        //                 heartbeat,
        //                 received_count: 0,
        //             },
        //         )
        //         .ok_or(|e| HeartbeatError::Other("Failed to insert new data".to_string())),
        // };

        return Ok(());
    }
}

struct UdapChannel {
    socket: Arc<Mutex<UdpSocket>>,
}

impl UdapChannel {
    fn receive(&self) -> Result<Heartbeat, HeartbeatError> {
        let mut buf = [0; 256];
        let locked_socket = self.socket.lock()?;
        let (size, _src) = locked_socket.recv_from(&mut buf)?;
        let heartbeat = serde_json::from_slice::<Heartbeat>(&buf[..size])?;
        Ok(heartbeat)
    }

    fn send(&self, heartbeat: Heartbeat, target_addresses: Vec<String>) -> Result<(), String> {
        let msg = serde_json::to_string(&heartbeat).map_err(|e| e.to_string())?;
        let locked_socket = self.socket.lock().map_err(|e| e.to_string())?;
        for address in target_addresses {
            locked_socket
                .send_to(msg.as_bytes(), address)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub struct Node {
    id: String,
    address: String,
    generation: u64,
    storage: Storage,
    channel: UdapChannel,
    heartbeat_interval: u64,
    heartbeat_spread: usize,
}

impl Node {
    pub fn new(
        id: String,
        address: String,
        generation: u64,
        seed_node_addresses: Vec<String>,
        heartbeat_interval: u64,
        heartbeat_spread: usize,
    ) -> Self {
        let mut table = HashMap::new();

        // add seed nodes
        for address in &seed_node_addresses {
            table.insert(
                address.to_string(),
                NodeHeartbeatData {
                    received_count: 0,
                    heartbeat: Heartbeat {
                        address: address.to_string(),
                        ..Default::default()
                    },
                },
            );
        }

        // add node itself
        table.insert(
            id.to_string(),
            NodeHeartbeatData {
                heartbeat: Heartbeat {
                    id: id.to_string(),
                    address: address.to_string(),
                    ..Default::default()
                },
                received_count: 0,
            },
        );

        let storage = Storage {
            data: Arc::new(Mutex::new(table)),
        };

        let socket = UdpSocket::bind(&address).expect("Could not bind socket");
        socket
            .set_nonblocking(true)
            .expect("Could not set socket to nonblocking");
        let channel = UdapChannel {
            socket: Arc::new(Mutex::new(socket)),
        };

        Node {
            id,
            address,
            generation,
            storage,
            channel,
            heartbeat_interval,
            heartbeat_spread,
        }
    }

    pub fn run(&self) -> Result<(), String> {
        let poll_interval_milisecs = 10;
        let heartbeat_interval_secs = self.heartbeat_interval;
        let heartbeat_spread = self.heartbeat_spread;
        let id = self.id;
        let address = self.address;
        let generation = self.generation;

        let _ = thread::spawn(move || {
            periodic_heartbeat(
                id,
                address,
                generation,
                heartbeat_interval_secs,
                heartbeat_spread,
                self.storage,
                self.channel,
            )
        });

        let _ = thread::spawn(move || {
            gossip_process(poll_interval_milisecs, heartbeat_spread, s_table, s_socket)
        });

        let _ = thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(1000));
        });

        Ok(())
    }

    pub fn update_status() -> Result<(), String> {
        Ok(())
    }
}

fn periodic_heartbeat(
    node_id: String,
    address: String,
    generation: u64,
    heartbeat_interval_secs: u64,
    heartbeat_spread: usize,
    storage: Storage,
    channel: UdapChannel,
) {
    let mut version = 0;
    loop {
        version += 1;
        let heartbeat = Heartbeat {
            id: node_id.clone(),
            address: address.clone(),
            generation,
            version,
            timestamp: now_unix(),
        };

        match storage.insert(heartbeat.clone()) {
            Ok(_) => (),
            Err(_) => {
                println!("Failed to get addresses");
                continue;
            }
        }

        let addresses = match storage.select_n_random_addresses(heartbeat_spread) {
            Ok(addresses) => addresses,
            Err(_) => {
                println!("Failed to get addresses");
                continue;
            }
        };

        match channel.send(heartbeat, addresses) {
            Ok(_) => thread::sleep(Duration::from_secs(heartbeat_interval_secs)),
            Err(_) => println!("failed to send shit"),
        };
    }
}

fn gossip(
    poll_interval_milisecs: u64,
    heartbeat_spread: usize,
    storage: Storage,
    channel: UdapChannel,
) {
    loop {
        thread::sleep(Duration::from_millis(poll_interval_milisecs));
        let heartbeat = match channel.receive() {
            Ok(heartbeat) => heartbeat,
            Err(HeartbeatError::WouldBlock) => continue,
            Err(_e) => {
                eprint!("Oh no could not receive heartbeat");
                continue;
            }
        };

        let n_times_received = storage.upsert(heartbeat);
        if !should_forward(n_times_received) {
            continue;
        }

        let addresses = match storage.select_n_random_addresses(heartbeat_spread) {
            Ok(addresses) => addresses,
            Err(_) => {
                println!("Failed to get addresses");
                continue;
            }
        };

        match channel.send(heartbeat.clone(), addresses) {
            Ok(_) => (),
            Err(_) => println!("failed to send shit"),
        };
    }
}

// fn gossip_process(
//     poll_interval_milisecs: u64,
//     gossip_to_n_nodes: usize,
//     storage: SharedStorage,
//     chanel: UdapChannel,
// ) {
//     loop {
//         thread::sleep(Duration::from_millis(poll_interval_milisecs));

//         // let l_socket = socket.lock().unwrap();
//         // let mut buf = [0; 256];
//         // let (size, _src) = match l_socket.recv_from(&mut buf) {
//         //     Ok(data) => data,
//         //     Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
//         //         continue;
//         //     }
//         //     Err(_) => {
//         //         println!("Could not read the UDP packet");
//         //         continue;
//         //     }
//         // };

//         let received_node_info = match serde_json::from_slice::<Heartbeat>(&buf[..size]) {
//             Ok(node) => node,
//             Err(_) => {
//                 println!("Failed to parse JSON");
//                 continue;
//             }
//         };

//         let table = &mut node_info_table.lock().unwrap();
//         let n_times_received = match table.get(&received_node_info.id) {
//             Some(current) => {
//                 let is_new = received_node_info.generation > current.heartbeat.generation
//                     || received_node_info.version > current.heartbeat.version;

//                 if is_new {
//                     0
//                 } else {
//                     current.received_count + 1
//                 }
//             }
//             None => 0,
//         };

//         if !should_forward(n_times_received) {
//             continue;
//         }

//         // if n_times_received > 0 {
//         //     continue;
//         // }

//         table.insert(
//             received_node_info.id.to_string(),
//             NodeHeartbeatData {
//                 received_count: n_times_received,
//                 heartbeat: received_node_info.clone(),
//             },
//         );

//         let addresses: Vec<String> = table
//             .iter()
//             .map(|(_, v)| v.heartbeat.address.clone())
//             .collect();
//         let selected_addresses = select_random_n_strings(addresses, gossip_to_n_nodes);
//         let msg = match serde_json::to_string(&received_node_info) {
//             Ok(s) => s,
//             Err(_) => {
//                 println!("Failed to serialize node");
//                 continue;
//             }
//         };

//         match send_msg(msg, selected_addresses, &l_socket) {
//             Ok(_) => {
//                 println!("sending update");
//             }
//             Err(e) => {
//                 println!("{}", e.to_string());
//             }
//         }
//     }
// }

// fn send_msg(msg: String, target_addresses: Vec<String>, socket: &UdpSocket) -> Result<(), String> {
//     for address in target_addresses {
//         socket
//             .send_to(msg.as_bytes(), address)
//             .map_err(|e| e.to_string())?;
//     }
//     Ok(())
// }

fn select_random_n_strings(a: Vec<String>, n: usize) -> Vec<String> {
    let mut a = a;
    let mut rng = thread_rng();
    a.shuffle(&mut rng);

    if a.len() < n {
        return a;
    }
    a[..n].to_vec()
}

fn should_forward(n_times_receieved: u64) -> bool {
    let base_probability = 1.0; // Starting probability of forwarding
    let decay_factor = 0.3; // Adjust this factor based on your needs
    let probability = base_probability * f64::exp(-decay_factor * n_times_receieved as f64);
    let mut rng = rand::thread_rng();
    rng.gen::<f64>() < probability
}

fn now_unix() -> u64 {
    return time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
}
