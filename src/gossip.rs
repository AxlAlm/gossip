use rand::Rng;
use rand::{seq::SliceRandom, thread_rng};
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::Error as SerdeError;
use std::collections::HashMap;
use std::fmt;
use std::io;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use std::{thread, time};
use tracing::{error, info, span, Level};

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

pub struct NodeHeartbeatData {
    heartbeat: Heartbeat,
    received_count: u64,
}

pub struct Storage {
    node_address: String,
    pub data: HashMap<String, NodeHeartbeatData>,
}

impl Storage {
    fn select_n_random_addresses(&self, n: usize) -> Result<Vec<String>, HeartbeatError> {
        let addresses: Vec<String> = self
            .data
            .iter()
            .map(|(_, v)| v.heartbeat.address.clone())
            .filter(|a| a != &self.node_address)
            .collect();
        let selected_addresses = select_random_n_strings(addresses, n);
        return Ok(selected_addresses);
    }

    fn insert(&mut self, heartbeat: Heartbeat) -> Result<u64, HeartbeatError> {
        if self.data.get(&heartbeat.id).is_none() {
            self.data.insert(
                heartbeat.id.clone(),
                NodeHeartbeatData {
                    heartbeat: heartbeat.clone(),
                    received_count: 1,
                },
            );
            return Ok(1);
        };

        let heartbeat_data = self.data.get(&heartbeat.id).unwrap();
        if heartbeat.generation > heartbeat_data.heartbeat.generation
            || heartbeat.version > heartbeat_data.heartbeat.version
        {
            self.data.insert(
                heartbeat.id.clone(),
                NodeHeartbeatData {
                    heartbeat: heartbeat.clone(),
                    received_count: 1,
                },
            );
            return Ok(1);
        } else {
            let x = self
                .data
                .insert(
                    heartbeat.id.clone(),
                    NodeHeartbeatData {
                        heartbeat: heartbeat.clone(),
                        received_count: heartbeat_data.received_count + 1,
                    },
                )
                .unwrap();
            return Ok(x.received_count);
        }
    }
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

pub struct Node {
    id: String,
    address: String,
    generation: u64,
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
        generation: u64,
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
            generation,
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
        let generation = self.generation;
        let shared_storage = self.shared_storage.clone();
        let shared_channel = self.shared_channel.clone();
        let shared_storage_clone = shared_storage.clone();
        let shared_channel_clone = shared_channel.clone();

        let span_clone = node_span.clone();
        let _ = thread::spawn(move || {
            let _enter = span_clone.enter();
            span_clone.record("thread", "periodic gossip");
            periodic_heartbeat(
                id,
                address,
                generation,
                heartbeat_interval_secs,
                heartbeat_spread,
                shared_storage_clone,
                shared_channel_clone,
            )
        });

        let shared_storage_clone = shared_storage.clone();
        let shared_channel_clone = shared_channel.clone();
        let span_clone = node_span.clone();
        let _ = thread::spawn(move || {
            let _enter = span_clone.enter();
            span_clone.record("thread", "gossip");
            gossip(
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
    generation: u64,
    heartbeat_interval_secs: u64,
    heartbeat_spread: usize,
    shared_storage: Arc<Mutex<Storage>>,
    shared_channel: Arc<Mutex<UdapChannel>>,
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

        let addresses = match storage.select_n_random_addresses(heartbeat_spread) {
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

        match channel.send(heartbeat, addresses) {
            Ok(_) => {
                info!("Heartbeat sent successfully");
                drop(storage);
                drop(channel);
                thread::sleep(Duration::from_secs(heartbeat_interval_secs))
            }
            Err(e) => error!(error = e.to_string(), "failed to send heartbeat"),
        };
    }
}

fn gossip(
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

        let n_times_received = storage.insert(heartbeat.clone());
        if !should_forward(n_times_received.unwrap()) {
            continue;
        }

        let addresses = match storage.select_n_random_addresses(heartbeat_spread) {
            Ok(addresses) => addresses,
            Err(e) => {
                error!(error = e.to_string(), "failed to select n random addresses");
                continue;
            }
        };

        match channel.send(heartbeat, addresses) {
            Ok(_) => (),
            Err(e) => error!(error = e.to_string(), "failed to send heartbeat"),
        };
    }
}

pub fn setup_storage(id: String, address: String, seed_node_addresses: Vec<String>) -> Storage {
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
        node_address: address.clone(),
        data: table,
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

fn should_forward(n_times_receieved: u64) -> bool {
    let base_probability = 1.0;
    let decay_factor = 0.3;
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
