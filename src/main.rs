use std::{
    collections::HashMap,
    net::{SocketAddr, ToSocketAddrs},
    sync::{Arc, Mutex},
};

use clap::{command, Parser};
use futures::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc::{unbounded_channel, UnboundedSender},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, protocol::Message},
    MaybeTlsStream, WebSocketStream,
};

/// Command line arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List of client addresses to connect to
    #[arg(short, long, value_delimiter = ',', value_parser = parse_client)]
    clients: Vec<String>,

    /// Address to bind the server
    #[arg(short, long, value_parser = parse_bind)]
    bind: String,
}

/// Parse and validate client URLs
fn parse_client(s: &str) -> Result<String, String> {
    // Validate the URL starts with ws:// or wss://
    if s.starts_with("ws://") {
        let ip_port = &s[5..];
        if let Ok(_socket_addr) = ip_port.to_socket_addrs() {
            return Ok(s.to_string());
        }
    }
    Err(format!("Invalid client URL: {}", s))
}

/// Parse and validate the bind address
fn parse_bind(s: &str) -> Result<String, String> {
    if let Ok(_socket_addr) = s.to_socket_addrs() {
        return Ok(s.to_string());
    }
    Err(format!("Invalid bind address: {}", s))
}

#[derive(Debug)]
struct P2PInnerMessage {
    message: Message,
    tx_handler: UnboundedSender<P2PInnerMessage>,
}
struct P2PWebsocketNetwork {
    addresses: Arc<Mutex<HashMap<SocketAddr, UnboundedSender<P2PInnerMessage>>>>,
    master: Arc<Mutex<UnboundedSender<P2PInnerMessage>>>,
}

// Define the WebSocket actor
struct WebSocketActor {
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}
impl WebSocketActor {
    async fn connect(url: &str) -> Option<Self> {
        match connect_async(url).await {
            Ok((conn, _)) => {
                println!("Connected successfully to {}", url);
                Some(WebSocketActor { ws_stream: conn })
            }
            Err(e) => {
                println!("Connection to {} failed: {:?}", url, e);
                None
            }
        }
    }
}

async fn handle_connection(state: Arc<P2PWebsocketNetwork>, conn: WebSocketActor) {
    // extract socket address as the key for clients list
    let addr = match conn.ws_stream.get_ref() {
        MaybeTlsStream::Plain(f) => f.peer_addr().unwrap(),
        _ => {
            panic!("tls is not supported yet");
        }
    };

    // this tx should be shared in network state
    let (tx, mut rx) = unbounded_channel::<P2PInnerMessage>();
    {
        let mut list = state.addresses.lock().unwrap();
        list.insert(addr, tx.clone());
    }

    let (mut ws_tx, mut ws_rx) = conn.ws_stream.split();
    loop {
        tokio::select! {
            Some(msg) = ws_rx.next() => {
                println!("1 Received: {:?}", msg);
                match msg {
                    Ok(msg) => {
                        if let Err(e) = state.master.lock().unwrap().send(P2PInnerMessage {
                            message: msg,
                            tx_handler: tx.clone(),
                        }) {
                            println!("Failed to send message to master: {:?}", e);
                        }
                    },
                    Err(e) => {
                        println!("Error receiving message or connection closed: {:?}", e);
                        break
                    }
                }
            }
            Some(msg) = rx.recv() => {
                println!("2 Sending: {:?}", msg);
                if let Err(e) = ws_tx.send(msg.message).await {
                    println!("Failed to send message on socket: {:?}", e);
                }
            }
        }
    }
    {
        // remove the client from the list
        let mut list = state.addresses.lock().unwrap();
        list.remove(&addr);
    }
}

async fn handle_server_connection(
    state: Arc<P2PWebsocketNetwork>,
    raw_stream: TcpStream,
    addr: SocketAddr,
) {
    let (tx, mut rx) = unbounded_channel::<P2PInnerMessage>();
    {
        let mut list = state.addresses.lock().unwrap();
        list.insert(addr, tx.clone());
    }

    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = match tokio_tungstenite::accept_async(raw_stream).await {
        Ok(ws) => ws,
        Err(e) => {
            println!("WebSocket handshake error: {:?}", e);
            return;
        }
    };

    println!("WebSocket connection established: {}", addr);

    let (mut ws_tx, mut ws_rx) = ws_stream.split();
    loop {
        tokio::select! {
            Some(msg) = ws_rx.next() => {
                println!("4 Received: {:?}", msg);
                match msg {
                    Ok(msg) => {
                        if let Err(e) = state.master.lock().unwrap().send(P2PInnerMessage {
                            message: msg,
                            tx_handler: tx.clone(),
                        }) {
                            println!("Failed to send message to master: {:?}", e);
                        }
                    },
                    Err(e) => {
                        println!("Error receiving message or connection closed: {:?}", e);
                        break
                    }
                }
            }
            Some(msg) = rx.recv() => {
                println!("5: {msg:?}");
                ws_tx.send(msg.message).await.unwrap_or_else(|e| println!("error to send on socket {e}"));
            }
        }
    }
    {
        // remove the client from the list
        let mut list = state.addresses.lock().unwrap();
        list.remove(&addr);
    }
}

async fn broadcast(
    state: Arc<P2PWebsocketNetwork>,
    tx: UnboundedSender<P2PInnerMessage>,
    bind: String,
) {
    // broadcast to connected clients
    let list = state.addresses.lock().unwrap();

    for (i, cl) in list.iter().enumerate() {
        println!("Broadcasting to {} ", cl.0);
        if let Err(e) = cl.1.send(P2PInnerMessage {
            message: tungstenite::protocol::Message::text(format!(
                "Message to client {} from {}",
                i, bind
            )),
            tx_handler: tx.clone(),
        }) {
            println!("Failed to send broadcast message: {:?}", e);
        }
    }
    println!("Broadcast end");
}
#[tokio::main]
async fn main() {
    let args = Args::parse();
    let (tx, mut rx) = unbounded_channel::<P2PInnerMessage>();
    let network_state: Arc<P2PWebsocketNetwork> = Arc::new(P2PWebsocketNetwork {
        addresses: Arc::new(Mutex::new(HashMap::new())),
        master: Arc::new(Mutex::new(tx.clone())),
    });

    for url in &args.clients {
        println!("connecting to {} ...", url);
        if let Some(conn) = WebSocketActor::connect(url).await {
            tokio::spawn(handle_connection(network_state.clone(), conn));
        } else {
            println!("could not connect to server: {url}");
        }
    }
    let listener = TcpListener::bind(&args.bind).await.expect("Failed to bind");

    loop {
        tokio::select! {
            Ok((stream, addr)) = listener.accept() => {
                tokio::spawn(handle_server_connection(network_state.clone(), stream, addr));
            }
            Some(msg) = rx.recv() => {
                println!("3: {msg:?}");
                // echo back that message to the client for the example:
                // msg.tx_handler.send(P2PInnerMessage{
                //     message: msg.message,
                //     tx_handler: tx.clone()
                // }).unwrap();
            }
            _ = tokio::signal::ctrl_c() => {
                println!("Received Ctrl+C, shutting down...");
                break
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(10)) => {
                tokio::spawn(broadcast(network_state.clone(), tx.clone(), args.bind.clone()));
            }
        }
    }
}
