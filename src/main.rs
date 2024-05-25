mod gossip;

use core::time;
use std::collections::HashMap;
use std::thread;

use std::sync::{Arc, Mutex};

fn main() {
    tracing_subscriber::fmt::init();

    let number_nodes = 10;
    let heartbeat_interval_secs = 30;
    let poll_interval_milisecs = 500;
    let heartbeat_spread = 2;
    let mut all_shared_storages: HashMap<String, Arc<Mutex<gossip::Storage>>> = HashMap::new();
    for i in 0..number_nodes {
        let seed_node_addresses = vec!["0.0.0.0:8001".to_string()];
        let id = i.to_string();
        let port = 8000 + i;
        let address = "0.0.0.0:".to_string() + &port.to_string();

        let storage =
            gossip::setup_storage(id.clone(), address.clone(), seed_node_addresses.clone());
        let shared_storage = Arc::new(Mutex::new(storage));

        all_shared_storages.insert(id.clone(), shared_storage.clone());

        let _ = thread::spawn(move || {
            let node = gossip::Node::new(
                id,
                address,
                1,
                shared_storage.clone(),
                heartbeat_interval_secs,
                heartbeat_spread,
                poll_interval_milisecs,
            );

            node.run()
        });
    }

    loop {
        for (_node_id, shared_storage) in all_shared_storages.iter() {
            let storage = shared_storage.lock().unwrap();
            println!("number node heartbeats: {}", storage.data.len());
        }
        thread::sleep(time::Duration::from_secs(1));
    }
}
