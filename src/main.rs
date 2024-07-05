mod gossip;

use core::time;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, sleep, JoinHandle};
use textplots::{ColorPlot, Shape};
use tracing::error;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

const HEALTHY_THRESHOLD_SECS: u64 = 60;
const NUMBER_NODES: u64 = 100;
const HEARTBEAT_INTERVAL_SECS: u64 = 10;
const POLL_INTERVAL_MILISECS: u64 = 15;
const HEARTBEAT_SPREAD: usize = 5;

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::new("error")) // Set the log level to ERROR
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let seed_nodes = vec![
        ("1".to_string(), "0.0.0.0:8001".to_string()),
        ("2".to_string(), "0.0.0.0:8002".to_string()),
    ];
    let port_start = 8000;

    // start inital nodes
    let mut all_shared_storages: HashMap<String, Arc<Mutex<gossip::Storage>>> = HashMap::new();
    // let active_nodes = Arc::new(Mutex::new(vec![true; NUMBER_NODES as usize]));
    let mut handles = vec![];
    for i in 0..NUMBER_NODES {
        let handle = start_node(
            i,
            port_start,
            seed_nodes.clone(),
            HEARTBEAT_INTERVAL_SECS,
            HEARTBEAT_SPREAD,
            POLL_INTERVAL_MILISECS,
            &mut all_shared_storages,
        );
        handles.push(handle);
    }

    plot(&all_shared_storages, NUMBER_NODES, HEALTHY_THRESHOLD_SECS);

    // let shared_handles = Arc::new(Mutex::new(handles));
    // let shared_handles_clones = shared_handles.clone();
    // let _ = thread::spawn(move || {

    //     loop {
    //
    //         let x = shared_handles_clones.lock().unwrap();
    //         x[0].join()
    //     }
    // })

    loop {
        sleep(time::Duration::from_secs(5));
    }
}

fn start_node(
    i: u64,
    port_start: u64,
    seed_nodes: Vec<(String, String)>,
    heartbeat_interval_secs: u64,
    heartbeat_spread: usize,
    poll_interval_milisecs: u64,
    all_shared_storages: &mut HashMap<String, Arc<Mutex<gossip::Storage>>>,
) -> JoinHandle<Result<(), String>> {
    let port = port_start + i;
    let address = "0.0.0.0:".to_string() + &port.to_string();

    let storage = gossip::setup_storage(i.to_string(), address.clone(), seed_nodes.clone());
    let shared_storage = Arc::new(Mutex::new(storage));
    all_shared_storages.insert(i.to_string(), shared_storage.clone());

    let handle = thread::spawn(move || {
        let node = gossip::Node::new(
            i.to_string(),
            address,
            shared_storage.clone(),
            heartbeat_interval_secs,
            heartbeat_spread,
            poll_interval_milisecs,
        );

        node.run()
    });

    handle
}

fn plot(
    all_shared_storages: &HashMap<String, Arc<Mutex<gossip::Storage>>>,
    number_nodes: u64,
    healthy_threshold_secs: u64,
) {
    const PURPLE: rgb::RGB8 = rgb::RGB8::new(0xE0, 0x80, 0xFF);
    const GREEN: rgb::RGB8 = rgb::RGB8::new(0x00, 0xFF, 0x00);

    let term = console::Term::stdout();
    term.hide_cursor().unwrap();
    term.clear_screen().unwrap();

    let all_shared_storages_cloned = all_shared_storages.clone();

    let _ = thread::spawn(move || {
        let mut fully_informed: Vec<(f32, f32)> = vec![];
        let mut know_all: Vec<(f32, f32)> = vec![];
        let mut messages_sent: Vec<(f32, f32)> = vec![];
        let mut max_n_messages_sent = 0.0;
        let mut i = 0;
        loop {
            let (n_fully_informed, n_know_all, n_messages_sent) = calculate_metrics(
                &all_shared_storages_cloned,
                number_nodes,
                healthy_threshold_secs,
            );

            fully_informed.push((i as f32, n_fully_informed));
            know_all.push((i as f32, n_know_all));
            messages_sent.push((i as f32, n_messages_sent));

            if n_messages_sent > max_n_messages_sent {
                max_n_messages_sent = n_messages_sent
            }

            term.move_cursor_to(0, 0).unwrap();
            println!("Number Fully Informed Nodes");
            textplots::Chart::new_with_y_range(200, 50, 0.0, i as f32, 0.0, number_nodes as f32)
                .linecolorplot(&Shape::Lines(&fully_informed), GREEN)
                // .linecolorplot(&Shape::Lines(&know_all), GREEN)
                .display();

            println!("Number Message");
            textplots::Chart::new_with_y_range(200, 50, 0.0, i as f32, 0.0, max_n_messages_sent)
                .linecolorplot(&Shape::Lines(&messages_sent), PURPLE)
                .display();

            sleep(time::Duration::from_millis(1000));
            i += 1;
        }
    });
}

fn calculate_metrics(
    all_shared_storages: &HashMap<String, Arc<Mutex<gossip::Storage>>>,
    number_nodes: u64,
    healthy_threshold_secs: u64,
) -> (f32, f32, f32) {
    // then check to see if each node has the latest info about each other node
    let mut n_messages_sent = 0;
    let mut n_fully_informed = 0;
    let mut n_know_all = 0;
    for (_, shared_storage) in all_shared_storages {
        let storage = match shared_storage.lock() {
            Ok(guard) => guard,
            Err(PoisonError { .. }) => {
                error!("failed to lock shared storage");
                continue;
            }
        };
        let storage_snapshot = storage.clone();
        drop(storage);

        if storage_snapshot.data.len() >= number_nodes as usize {
            n_know_all += 1;
        }

        let mut nr_with_latest = 0;
        for (_, data) in &storage_snapshot.data {
            let seconds_since = gossip::now_unix() - data.heartbeat.timestamp.clone();

            if seconds_since < HEARTBEAT_INTERVAL_SECS {
                n_messages_sent += data.received_count;
            }

            if seconds_since < healthy_threshold_secs {
                nr_with_latest += 1
            }
        }

        if nr_with_latest != number_nodes {
            continue;
        }

        n_fully_informed += 1
    }

    return (
        n_fully_informed as f32,
        n_know_all as f32,
        n_messages_sent as f32,
    );
}
