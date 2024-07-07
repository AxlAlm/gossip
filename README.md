# Gossip Protocol Simulation

This project simulates a gossip protocol in a distributed system. It includes functionality to monitor the health and information dissemination among nodes in the network.

### Features

- **Node Initialization**: Start a specified number of nodes, each with its own storage and communication channels.
- **Gossip Protocol**: Nodes periodically exchange information about each other's states.
- **Node Failure Simulation**: Randomly simulate node failures and recoveries.
- **Metrics Calculation**: Track and display various metrics to evaluate the protocol's performance.
- **Real-time Plotting**: Visualize the number of fully informed nodes, nodes that know all others, messages sent, and alive nodes over time.

## Problem

We have a distributed system of nodes, and we need to track the health and status of all nodes in the network.

## Solution

We use a epidemic gossip protocol to disseminate heartbeats from nodes to all nodes, making each node aware of the health of each other node.

1. **Heartbeat Transmission**: Every `N` seconds, each node sends a heartbeat to `N` random nodes.
2. **Probabilistic Forwarding**: Upon receiving a heartbeat, a node forwards it to `N` random nodes with a probability that decreases based on the number of times the heartbeat has been received previously. This means that we pass on "seen" information with some degree of redundancy which ensures all information reaches all nodes.

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
- suspecting that the implementation is way to chatty, around 30k message are sent to inform 100 nodes which seems alot. Either more tuning of configuration needs to be done, or the implementation is too naive to be able to reduce "chattyness".

## General Insights

- Configuration is important to get a stable network, i.e. with a certain number of nodes you would need a certain decay factor, certain spread and so on.
