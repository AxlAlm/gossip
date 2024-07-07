mod gossip;

use core::time;
use rand::{seq::SliceRandom, thread_rng};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, sleep};
use std::time::Duration;
use textplots::{ColorPlot, Shape};
use tracing::error;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

const PORT_BASE: u64 = 8000;
const NUMBER_SEED_NODES: u64 = 2;
const NUMBER_NODES: u64 = 100;
const HEALTHY_THRESHOLD_SECS: u64 = 30;
const HEARTBEAT_INTERVAL_SECS: u64 = 5;
const POLL_INTERVAL_MILISECS: u64 = 10;
const HEARTBEAT_SPREAD: usize = 5;
const NUMBER_NODES_TO_KILL: usize = 20;
const KILL_NODES_AFTER_N_SECS: u64 = 60;
const START_ALL_NODES_AFTER_N_SECS: u64 = 40;
// factor of which the probability of forwarding information should decrease given times the
// information already been sent
const DECAY_FACTOR: f64 = 0.8;

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::new("error")) // Set the log level to ERROR
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let mut seed_nodes = vec![];
    for i in 0..NUMBER_SEED_NODES {
        let port = PORT_BASE + i;
        let address = "0.0.0.0:".to_string() + &port.to_string();
        seed_nodes.push((i.to_string(), address));
    }

    // start inital nodes
    let mut all_shared_storages: HashMap<String, Arc<Mutex<gossip::Storage>>> = HashMap::new();
    let is_alive_flags: Arc<Mutex<Vec<Arc<AtomicBool>>>> = Arc::new(Mutex::new(Vec::new()));

    for i in 0..NUMBER_NODES {
        let port = PORT_BASE + i;
        let address = "0.0.0.0:".to_string() + &port.to_string();
        let storage = gossip::setup_storage(i.to_string(), address.clone(), seed_nodes.clone());
        let shared_storage = Arc::new(Mutex::new(storage));
        all_shared_storages.insert(i.to_string(), shared_storage.clone());

        let is_alive = Arc::new(AtomicBool::new(true));
        let is_alive_clone = is_alive.clone();

        is_alive_flags
            .lock()
            .expect("Failed to get is_alive_flags")
            .push(is_alive);

        let node = gossip::Node::new(
            i.to_string(),
            address,
            shared_storage.clone(),
            HEARTBEAT_INTERVAL_SECS,
            HEARTBEAT_SPREAD,
            POLL_INTERVAL_MILISECS,
            DECAY_FACTOR,
            is_alive_clone,
        );
        let _ = node.run();
    }

    let is_alive_flags_shared = is_alive_flags.clone();
    let _cancellation_thread = thread::spawn(move || {
        thread::sleep(Duration::from_secs(KILL_NODES_AFTER_N_SECS));

        {
            let flags = is_alive_flags_shared.lock().unwrap();
            let num_flags = flags.len();

            let mut indices = (0..num_flags).collect::<Vec<_>>();
            let mut rng = thread_rng();
            indices.shuffle(&mut rng);

            for &index in &indices[0..NUMBER_NODES_TO_KILL] {
                flags[index].store(false, Ordering::SeqCst);
            }
        }

        thread::sleep(Duration::from_secs(START_ALL_NODES_AFTER_N_SECS));

        {
            let flags = is_alive_flags_shared
                .lock()
                .expect("failed to get is_alive_flags");
            for flag in flags.iter() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    });

    let is_alive_flags_shared = is_alive_flags.clone();
    plot(
        &all_shared_storages,
        is_alive_flags_shared,
        NUMBER_NODES,
        HEALTHY_THRESHOLD_SECS,
        HEARTBEAT_INTERVAL_SECS,
    );

    loop {
        sleep(time::Duration::from_secs(5));
    }
}

fn plot(
    all_shared_storages: &HashMap<String, Arc<Mutex<gossip::Storage>>>,
    is_alive_flags: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
    number_nodes: u64,
    healthy_threshold_secs: u64,
    hearthbeat_interval_secs: u64,
) {
    const PURPLE: rgb::RGB8 = rgb::RGB8::new(0xE0, 0x80, 0xFF);
    const GREEN: rgb::RGB8 = rgb::RGB8::new(0x00, 0xFF, 0x00);
    const YELLOW: rgb::RGB8 = rgb::RGB8::new(0xFF, 0xFF, 0x00);
    // const BLUE: rgb::RGB8 = rgb::RGB8::new(0x00, 0x00, 0xFF);

    let term = console::Term::stdout();
    term.hide_cursor().unwrap();
    term.clear_screen().unwrap();

    let all_shared_storages_cloned = all_shared_storages.clone();

    let _ = thread::spawn(move || {
        let mut fully_informed: Vec<(f32, f32)> = vec![];
        let mut know_all: Vec<(f32, f32)> = vec![];
        let mut messages_sent: Vec<(f32, f32)> = vec![];
        let mut number_nodes_alive: Vec<(f32, f32)> = vec![];
        let mut max_n_messages_sent = 0.0;
        let mut i = 0;
        loop {
            let (n_fully_informed, n_know_all, n_messages_sent) = calculate_metrics(
                &all_shared_storages_cloned,
                number_nodes,
                healthy_threshold_secs,
                hearthbeat_interval_secs,
            );

            let flags = is_alive_flags.lock().expect("failed to get is_alive_flags");
            let number_alive = flags
                .iter()
                .filter(|flag| flag.load(Ordering::SeqCst))
                .count();

            fully_informed.push((i as f32, n_fully_informed));
            know_all.push((i as f32, n_know_all as f32));
            number_nodes_alive.push((i as f32, number_alive as f32));
            messages_sent.push((i as f32, n_messages_sent));

            if n_messages_sent > max_n_messages_sent {
                max_n_messages_sent = n_messages_sent
            }

            term.move_cursor_to(0, 0).unwrap();
            println!("Yellow = N nodes that has the latest heartbeat for each node.");
            // println!("Blue = N nodes that know about all other nodes");
            textplots::Chart::new_with_y_range(200, 50, 0.0, i as f32, 0.0, number_nodes as f32)
                .linecolorplot(&Shape::Lines(&fully_informed), YELLOW)
                // .linecolorplot(&Shape::Lines(&know_all), BLUE) // NOT SURE IF USEFUL
                .display();

            println!("Number Message");
            textplots::Chart::new_with_y_range(200, 50, 0.0, i as f32, 0.0, max_n_messages_sent)
                .linecolorplot(&Shape::Lines(&messages_sent), PURPLE)
                .display();

            println!("Number Alive Nodes");
            textplots::Chart::new_with_y_range(200, 50, 0.0, i as f32, 0.0, number_nodes as f32)
                .linecolorplot(&Shape::Lines(&number_nodes_alive), GREEN)
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
    hearthbeat_interval_secs: u64,
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

            if seconds_since < hearthbeat_interval_secs {
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
