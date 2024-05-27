mod gossip;

use core::time;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, sleep};
use textplots::{ColorPlot, Plot, Shape};
use tracing::error;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::new("error")) // Set the log level to ERROR
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let number_nodes = 50;
    let heartbeat_interval_secs = 10;
    let poll_interval_milisecs = 10;
    let heartbeat_spread = 5;

    let seed_nodes = vec![
        ("1".to_string(), "0.0.0.0:8001".to_string()),
        ("2".to_string(), "0.0.0.0:8002".to_string()),
    ];
    let port_start = 8000;

    let mut all_shared_storages: HashMap<String, Arc<Mutex<gossip::Storage>>> = HashMap::new();
    for i in 0..number_nodes {
        let id = i.to_string();
        let port = port_start + i;
        let address = "0.0.0.0:".to_string() + &port.to_string();

        let storage = gossip::setup_storage(id.clone(), address.clone(), seed_nodes.clone());
        let shared_storage = Arc::new(Mutex::new(storage));

        all_shared_storages.insert(id.clone(), shared_storage.clone());

        let _ = thread::spawn(move || {
            let node = gossip::Node::new(
                id,
                address,
                shared_storage.clone(),
                heartbeat_interval_secs,
                heartbeat_spread,
                poll_interval_milisecs,
            );

            node.run()
        });
    }

    let PLOT = true;
    if PLOT {
        plot(&all_shared_storages, number_nodes);
    }

    loop {
        sleep(time::Duration::from_secs(5));
    }
}

fn plot(all_shared_storages: &HashMap<String, Arc<Mutex<gossip::Storage>>>, number_nodes: u64) {
    const PURPLE: rgb::RGB8 = rgb::RGB8::new(0xE0, 0x80, 0xFF);
    // const RED: rgb::RGB8 = rgb::RGB8::new(0xFF, 0x00, 0x00);
    const GREEN: rgb::RGB8 = rgb::RGB8::new(0x00, 0xFF, 0x00);

    let term = console::Term::stdout();
    term.hide_cursor().unwrap();
    term.clear_screen().unwrap();

    let all_shared_storages_cloned = all_shared_storages.clone();

    // On ctrl+C, reset terminal settings and let the thread know to stop
    // ctrlc::set_handler(move || {
    //     should_run_ctrlc_ref
    //         .as_ref()
    //         .store(false, Ordering::Relaxed);
    // })
    // .unwrap();

    let _ = thread::spawn(move || {
        let mut fully_informed: Vec<(f32, f32)> = vec![];
        let mut know_all: Vec<(f32, f32)> = vec![];
        let mut messages_sent: Vec<(f32, f32)> = vec![];
        let mut max_n_messages_sent = 0.0;
        let mut i = 0;
        loop {
            let (n_fully_informed, n_know_all, n_messages_sent) =
                calculate_metrics(&all_shared_storages_cloned, number_nodes);

            fully_informed.push((i as f32, n_fully_informed));
            know_all.push((i as f32, n_know_all));
            messages_sent.push((i as f32, n_messages_sent));

            if n_messages_sent > max_n_messages_sent {
                max_n_messages_sent = n_messages_sent
            }

            term.move_cursor_to(0, 0).unwrap();
            textplots::Chart::new_with_y_range(100, 50, 0.0, i as f32, 0.0, number_nodes as f32)
                .linecolorplot(&Shape::Lines(&fully_informed), PURPLE)
                .linecolorplot(&Shape::Lines(&know_all), GREEN)
                .display();

            textplots::Chart::new_with_y_range(100, 50, 0.0, i as f32, 0.0, max_n_messages_sent)
                .lineplot(&Shape::Lines(&messages_sent))
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
    // first we collect the last hearbeat sent from each node
    // let mut latest_timestamps = HashMap::new();
    // for (node_id, shared_storage) in all_shared_storages {
    //     let storage = match shared_storage.lock() {
    //         Ok(guard) => guard,
    //         Err(PoisonError { .. }) => {
    //             error!("failed to lock shared storage");
    //             continue;
    //         }
    //     };
    //     let heartbeat_data = storage.data.get(node_id).unwrap();
    //     latest_timestamps.insert(node_id.clone(), heartbeat_data.heartbeat.timestamp);
    // }

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

        // dbg!(
        //     storage.data.len() == number_nodes as usize,
        //     storage.data.len(),
        //     number_nodes
        // );
        // dbg!(storage.data.keys());
        if storage.data.len() >= number_nodes as usize {
            n_know_all += 1;
        }

        let mut nr_with_latest = 0;
        for (node_id, data) in &storage.data {
            // let last_heartbeat_timestamp = match latest_timestamps.get(node_id) {
            //     Some(ts) => ts,
            //     None => continue,
            // };

            n_messages_sent += data.received_count;

            // dbg!(
            //     gossip::now_unix() - data.heartbeat.timestamp.clone() < 60,
            //     gossip::now_unix() - data.heartbeat.timestamp.clone()
            // );
            if (gossip::now_unix() - data.heartbeat.timestamp.clone() < healthy_threshold_secs) {
                nr_with_latest += 1
            }

            // dbg!(
            //     last_heartbeat_timestamp == &data.heartbeat.timestamp,
            //     last_heartbeat_timestamp,
            //     &data.heartbeat.timestamp
            // );
            // if last_heartbeat_timestamp == &data.heartbeat.timestamp {
            //     nr_with_latest += 1
            // }
        }

        // dbg!(nr_with_latest != number_nodes, nr_with_latest, number_nodes);
        if nr_with_latest != number_nodes {
            continue;
        }

        n_fully_informed += 1
    }

    dbg!(
        n_fully_informed as f32,
        n_know_all as f32,
        n_messages_sent as f32,
    );

    return (
        n_fully_informed as f32,
        n_know_all as f32,
        n_messages_sent as f32,
    );
}
