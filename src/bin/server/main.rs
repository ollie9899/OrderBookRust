use OrderBookRust::{Order, Side, order_book::OrderBook};
use tokio::{io::{AsyncBufReadExt, BufReader}, net::TcpListener, sync::mpsc};

#[tokio::main]
async fn main() {
    // channel between the TCP listener tasks and the order book task.
    // capacity of 2000 means up to 2000 orders can be queued before
    // senders start blocking.
    let (tx, mut rx) = mpsc::channel::<Order>(2000);

    // spawn a dedicated task that owns the order book.
    // all matching runs here sequentially — no locking needed since
    // only this task ever uses the orderbook
    tokio::spawn(async move {
        let mut order_book = OrderBook::new();
        let mut order_count = 0;

        loop {
            tokio::select! {
                // process the next incoming order from the channel
                Some(order) = rx.recv() => {
                    #[cfg(feature = "verbose")]
                    println!("Order received: id={} side={:?} price={:.2} qty={}",
                        order.id,
                        order.side,
                        order.price as f64 / 100.0,
                        order.quantity,
                    );

                    order_count += 1;

                    // purge empty price levels for every 200 orders received to keep the book clean
                    if order_count % 200 == 0 {
                        order_book.purge_empty_levels();
                    }

                    // route to the correct matching function based on side
                    match order.side {
                        Side::Bid => order_book.incoming_bid(order),
                        Side::Ask => order_book.incoming_ask(order),
                    }
                }

                // on Ctrl+C print the final book state and trade log then exit
                _ = tokio::signal::ctrl_c() => {
                    println!("\n=== FINAL ORDER BOOK ===");
                    order_book.print_book();

                    println!("\n=== TRADE LOG ({} trades) ===", order_book.trades.len());
                    order_book.print_trades();

                    std::process::exit(0);
                }
            }
        }
    });

    // bind the TCP listener — all clients connect to this port
    let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

    loop {
        println!("Order book is waiting for orders on 127.0.0.1:8080");

        // block until a new client connects
        let (socket, _) = listener.accept().await.unwrap();

        // clone the sender so this connection's task can forward orders
        // to the order book task — each connection gets its own clone
        let tx = tx.clone();

        // spawn a task per connection so multiple clients can send
        // orders concurrently without blocking each other
        tokio::spawn(async move {
            // wrap socket in a buffered line reader — each line is one JSON order
            let reader = BufReader::new(socket);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<Order>(&line) {
                    Ok(order) => {
                        // forward the parsed order to the order book task
                        if let Err(e) = tx.send(order).await {
                            eprintln!("Failed to send order: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        // handle malformed order 
                        eprintln!("Invalid JSON: {}", e);
                    }
                }
            }
        });
    }
}