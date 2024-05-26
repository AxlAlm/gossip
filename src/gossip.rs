use rand::Rng;
use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::Error as SerdeError;
use std::collections::HashMap;
use std::fmt;
use std::io::{self, empty};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use std::{thread, time};
use tracing::{error, info, span, Level};

pub struct Node {
    id: String,
    address: String,
    shared_storage: Arc<Mutex<Storage>>,
    shared_channel: Arc<Mutex<UdapChannel>>,
    heartbeat_interval_secs: u64,
    heartbeat_spread: usize,
    poll_interval_milisecs: u64,
}

impl Node {
    pub fn new(
        id: String,
        address: String,
        shared_storage: Arc<Mutex<Storage>>,
        heartbeat_interval_secs: u64,
        heartbeat_spread: usize,
        poll_interval_milisecs: u64,
    ) -> Self {
        let socket = UdpSocket::bind(&address).expect("Could not bind socket");
        socket
            .set_nonblocking(true)
            .expect("Could not set socket to nonblocking");
        let channel = UdapChannel { socket };

        Node {
            id,
            address,
            shared_storage,
            shared_channel: Arc::new(Mutex::new(channel)),
            heartbeat_interval_secs,
            heartbeat_spread,
            poll_interval_milisecs,
        }
    }

    pub fn run(&self) -> Result<(), String> {
        let node_span = span!(
            Level::INFO,
            "node",
            node_id = &self.id,
            address = &self.address,
            thread = "main",
        );
        let _enter = node_span.enter();

        info!("Running Node");
        let poll_interval_milisecs = self.poll_interval_milisecs;
        let heartbeat_interval_secs = self.heartbeat_interval_secs;
        let heartbeat_spread = self.heartbeat_spread;
        let id = self.id.clone();
        let address = self.address.clone();
        let shared_storage = self.shared_storage.clone();
        let shared_channel = self.shared_channel.clone();
        let shared_storage_clone = shared_storage.clone();
        let shared_channel_clone = shared_channel.clone();

        let span_clone = node_span.clone();
        let _ = thread::spawn(move || {
            let _enter = span_clone.enter();
            periodic_heartbeat(
                id,
                address,
                heartbeat_interval_secs,
                heartbeat_spread,
                shared_storage_clone,
                shared_channel_clone,
            )
        });

        let shared_storage_clone = shared_storage.clone();
        let shared_channel_clone = shared_channel.clone();
        let address = self.address.clone();
        let span_clone = node_span.clone();
        let _ = thread::spawn(move || {
            let _enter = span_clone.enter();
            gossip(
                address,
                poll_interval_milisecs,
                heartbeat_spread,
                shared_storage_clone,
                shared_channel_clone,
            )
        });

        Ok(())
    }
}

fn periodic_heartbeat(
    node_id: String,
    address: String,
    heartbeat_interval_secs: u64,
    heartbeat_spread: usize,
    shared_storage: Arc<Mutex<Storage>>,
    shared_channel: Arc<Mutex<UdapChannel>>,
) {
    loop {
        let heartbeat = Heartbeat {
            id: node_id.clone(),
            address: address.clone(),
            timestamp: now_unix(),
        };

        let mut storage = match shared_storage.lock() {
            Ok(guard) => guard,
            Err(PoisonError { .. }) => {
                error!("failed to lock shared storage");
                continue;
            }
        };

        match storage.insert(heartbeat.clone()) {
            Ok(_) => (),
            Err(e) => {
                error!(error = e.to_string(), "failed insert heartbeat");
                continue;
            }
        }

        let addresses = match storage.select_n_random_addresses(
            heartbeat_spread,
            // we filter out address to node itself and node we got heartbeat from
            vec![address.clone(), heartbeat.address.clone()],
        ) {
            Ok(addresses) => addresses,
            Err(e) => {
                error!(error = e.to_string(), "failed to select n random addresses");
                continue;
            }
        };

        let channel = match shared_channel.lock() {
            Ok(guard) => guard,
            Err(PoisonError { .. }) => {
                error!("failed to lock shared channel");
                continue;
            }
        };

        match channel.send(heartbeat.clone(), addresses.clone()) {
            Ok(_) => {
                info!("Heartbeat sent successfully");
                // storage.sent_to(heartbeat.id.clone(), addresses.clone());
                drop(storage);
                drop(channel);
                thread::sleep(Duration::from_secs(heartbeat_interval_secs))
            }
            Err(e) => error!(error = e.to_string(), "failed to send heartbeat"),
        };
    }
}

fn gossip(
    address: String,
    poll_interval_milisecs: u64,
    heartbeat_spread: usize,
    shared_storage: Arc<Mutex<Storage>>,
    shared_channel: Arc<Mutex<UdapChannel>>,
) {
    loop {
        thread::sleep(Duration::from_millis(poll_interval_milisecs));

        let channel = match shared_channel.lock() {
            Ok(guard) => guard,
            Err(PoisonError { .. }) => {
                error!("failed to lock shared channel");
                continue;
            }
        };

        let heartbeat = match channel.receive() {
            Ok(heartbeat) => heartbeat,
            Err(HeartbeatError::WouldBlock) => continue,
            Err(e) => {
                error!(error = e.to_string(), "failed to receive heartbeat");
                continue;
            }
        };

        let mut storage = match shared_storage.lock() {
            Ok(guard) => guard,
            Err(PoisonError { .. }) => {
                error!("failed to lock shared storage");
                continue;
            }
        };

        let n_times_received = match storage.insert(heartbeat.clone()) {
            Ok(count) => count,
            Err(e) => {
                error!(error = e.to_string(), "failed to insert heartbeat");
                continue;
            }
        };

        if !should_forward(n_times_received) {
            continue;
        }

        let mut b = vec![address.clone(), heartbeat.address.clone()];
        // if storage.sent_to_data.get(&heartbeat.id).is_some() {
        //     b.extend(storage.sent_to_data.get(&heartbeat.id).unwrap().clone())
        // }

        let addresses = match storage.select_n_random_addresses(
            heartbeat_spread,
            // we filter out address to node itself and node we got heartbeat from
            b.clone(),
        ) {
            Ok(addresses) => addresses,
            Err(e) => {
                error!(error = e.to_string(), "failed to select n random addresses");
                continue;
            }
        };

        dbg!(heartbeat.address.clone(), addresses.clone());

        match channel.send(heartbeat.clone(), addresses.clone()) {
            Ok(_) => (),
            Err(e) => error!(error = e.to_string(), "failed to send heartbeat"),
        };

        // storage.sent_to(heartbeat.id, addresses);
    }
}

#[derive(Debug)]
pub enum HeartbeatError {
    Io(io::Error),
    Serde(SerdeError),
    WouldBlock,
    Other(String),
}

impl fmt::Display for HeartbeatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeartbeatError::Io(err) => write!(f, "IO error: {}", err),
            HeartbeatError::Serde(err) => write!(f, "Serialization error: {}", err),
            HeartbeatError::WouldBlock => write!(f, "Operation would block"),
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Heartbeat {
    id: String,
    address: String,
    pub timestamp: u64,
}

#[derive(Debug)]
pub struct NodeHeartbeatData {
    pub heartbeat: Heartbeat,
    pub received_count: u64,
}

#[derive(Debug)]
pub struct Storage {
    pub data: HashMap<String, NodeHeartbeatData>,
    pub sent_to_data: HashMap<String, Vec<String>>,
}

impl Storage {
    fn select_n_random_addresses(
        &self,
        n: usize,
        filter_out: Vec<String>,
    ) -> Result<Vec<String>, HeartbeatError> {
        let addresses: Vec<String> = self
            .data
            .iter()
            .map(|(_, v)| v.heartbeat.address.clone())
            .filter(|a| !filter_out.contains(a))
            .collect();
        let selected_addresses = select_random_n_strings(addresses, n);
        return Ok(selected_addresses);
    }

    fn insert(&mut self, heartbeat: Heartbeat) -> Result<u64, HeartbeatError> {
        let received_count = match self.data.get(&heartbeat.id) {
            Some(d) => {
                if heartbeat.timestamp > d.heartbeat.timestamp {
                    // self.sent_to_data.insert(heartbeat.id.clone(), vec![]);
                    1
                } else {
                    d.received_count + 1
                }
            }
            None => 1,
        };

        let x = self.data.insert(
            heartbeat.id.clone(),
            NodeHeartbeatData {
                heartbeat,
                received_count,
            },
        );

        dbg!(x);

        Ok(received_count)
    }

    // fn sent_to(&mut self, id: String, addresses: Vec<String>) {
    //     // let mut x = addresses;
    //     // if self.sent_to_data.get(&id).is_some() {
    //     //     x.extend(self.sent_to_data.get(&id).unwrap().clone());
    //     // }
    //     self.sent_to_data.insert(id, addresses);
    // }

    // fn get_sent_to(&mut self, id: String, ) {
    //     let z = self.sent_to_data.get(&id).unwrap();
    // }
}

struct UdapChannel {
    socket: UdpSocket,
}

impl UdapChannel {
    fn receive(&self) -> Result<Heartbeat, HeartbeatError> {
        let mut buf = [0; 256];
        let (size, _src) = self.socket.recv_from(&mut buf)?;
        let heartbeat = serde_json::from_slice::<Heartbeat>(&buf[..size])?;

        Ok(heartbeat)
    }

    fn send(
        &self,
        heartbeat: Heartbeat,
        target_addresses: Vec<String>,
    ) -> Result<(), HeartbeatError> {
        let msg = serde_json::to_string(&heartbeat).map_err(|e| e.to_string())?;
        for address in target_addresses {
            self.socket
                .send_to(msg.as_bytes(), address)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub fn setup_storage(id: String, address: String, seed_nodes: Vec<(String, String)>) -> Storage {
    let mut data = HashMap::new();

    // add seed nodes
    for (id, address) in &seed_nodes {
        data.insert(
            address.to_string(),
            NodeHeartbeatData {
                received_count: 0,
                heartbeat: Heartbeat {
                    id: id.to_string(),
                    address: address.to_string(),
                    timestamp: now_unix(),
                },
            },
        );
    }

    // add node itself
    data.insert(
        id.to_string(),
        NodeHeartbeatData {
            heartbeat: Heartbeat {
                id: id.to_string(),
                address: address.to_string(),
                timestamp: now_unix(),
            },
            received_count: 0,
        },
    );

    let storage = Storage {
        data,
        sent_to_data: HashMap::new(),
    };
    return storage;
}

fn select_random_n_strings(a: Vec<String>, n: usize) -> Vec<String> {
    let mut a = a;
    let mut rng = thread_rng();
    a.shuffle(&mut rng);

    if a.len() < n {
        return a;
    }
    a[..n].to_vec()
}

// fn should_forward(n_times_receieved: u64) -> bool {
//     let base_probability = 1.0;
//     let decay_factor = 0.3;
//     let probability = base_probability * f64::exp(-decay_factor * n_times_receieved as f64);
//     let mut rng = rand::thread_rng();
//     rng.gen::<f64>() < probability
// }

fn should_forward(n_times_received: u64) -> bool {
    let probability = 1.0 / n_times_received as f64;
    let mut rng = rand::thread_rng();
    rng.gen::<f64>() < probability
}

fn now_unix() -> u64 {
    return time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
}
