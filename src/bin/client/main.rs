use OrderBookRust::Order;
use serde::Deserialize;
use std::env;
use std::fs;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::{Duration, sleep};

#[derive(Debug, Deserialize)]
struct OrdersFile {
    orders: Vec<Order>,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: client <orders.json>");
        return;
    }

    let file_path = &args[1];
    let content = fs::read_to_string(file_path).expect("Failed to read file");
    let data: OrdersFile = serde_json::from_str(&content).expect("Invalid JSON");

    let mut stream = TcpStream::connect("127.0.0.1:8080").await.unwrap();

    for order in data.orders {
        let json = serde_json::to_string(&order).unwrap();

        stream.write_all(json.as_bytes()).await.unwrap();
        stream.write_all(b"\n").await.unwrap();
        sleep(Duration::from_millis(500)).await;
    }
}
