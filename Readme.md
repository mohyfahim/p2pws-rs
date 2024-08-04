# P2P WebSocket Network Example

This project demonstrates how to create a peer-to-peer (P2P) network over a WebSocket infrastructure using Rust and the Tokio async runtime. The example code allows nodes to connect to each other, forming a distributed network where messages can be exchanged between peers.

## Features

- Establish a P2P network using WebSockets.
- Support for both client and server WebSocket connections.
- Broadcast messages to all connected peers.
- Graceful shutdown on receiving Ctrl+C.

## Dependencies

This project uses the following Rust crates:

- `tokio`: For asynchronous programming and handling I/O operations.
- `tokio-tungstenite`: For WebSocket client and server implementation.
- `futures`: For handling asynchronous streams and sinks.
- `clap`: For parsing command-line arguments.
- `log`: For logging information and errors.
- `env_logger`: For configuring the log output.

## Running

Ensure you have Rust and Cargo installed on your system. Clone the repository and navigate to the project directory:

```bash
$ git clone https://github.com/mohyfahim/p2pws-rs.git
$ cd p2pws-rs
$ cargo build
```
Then in separate sessions, run the program in orders with the following arguments:
```bash
$ RUST_LOG=debug cargo run -- --bind localhost:8080
```
```bash
$ RUST_LOG=debug cargo run -- --peers ws://localhost:8080 --bind localhost:8085
```
```bash
$ RUST_LOG=debug cargo run -- --peers ws://localhost:8080,ws://localhost:8085 --bind localhost:8086
```
Then you can see the message, broadcasting from each peer to all others. all-to-all broadcast!