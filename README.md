# Gossip Protocol Simulation

This project simulates a gossip protocol in a distributed system. It includes functionality to monitor the health and information dissemination among nodes in the network.

## Features

- **Node Initialization**: Start a specified number of nodes, each with its own storage and communication channels.
- **Gossip Protocol**: Nodes periodically exchange information about each other's states.
- **Node Failure Simulation**: Randomly simulate node failures and recoveries.
- **Metrics Calculation**: Track and display various metrics to evaluate the protocol's performance.
- **Real-time Plotting**: Visualize the number of fully informed nodes, nodes that know all others, messages sent, and alive nodes over time.

## Problem

We have a distributed system of nodes, and we need to efficiently track the health and status of all nodes in the network.

## Solution:

We use a gossip protocol to disseminate heartbeat messages in an epidemic fashion, ensuring tracking of the health and status of all nodes in the distributed system.

The implemented gossip protocol is an epidemic variant that relies on broadcasting multiple messages to ensure redundancy. Nodes periodically send and receive heartbeat messages to track each other's status. To enhance reliability, nodes also forward previously seen messages, ensuring that information propagates throughout the network and reaches all nodes.

1. **Heartbeat Transmission**: Every `N` seconds, each node sends a heartbeat to `N` random nodes.
2. **Probabilistic Forwarding**: Upon receiving a heartbeat, a node forwards it to `N` random nodes with a probability that decreases based on the number of times the heartbeat has been received previously.

## Simulation (with base settings)

##### Base Assumptions

- A node is considered healthy if a heartbeat was received within 30 seconds.

##### Simulation Steps

1. **Start Nodes**: Initialize 100 nodes, each node only knows about 2 seed nodes at start.
2. **Ramp-Up Period**: Allow nodes to gossip for 60 seconds to reach a stable state where all nodes have exchanged heartbeats.
3. **Stop Nodes**: Randomly stop 20 nodes.
4. **Health Threshold**: Wait for 40 seconds to ensure the health threshold is surpassed, marking nodes as unhealthy.
5. **Recovery**: restart the 20 nodes and allow the network to recover.

### How to run:

```sh
cargo run
```

## Possible improvements

- when selecting which nodes to forward information to the selection of nodes could be weighted based on how many times they have been send the information before

## General Insights

- Configuration is important to get a stable network, i.e. with a certain number of nodes you would need a certain decay factor, certain spread and so on.
