# OrderBookRust

A high-performance order book implemented in Rust, supporting price-time priority matching for both bid and ask sides.

## Design

All orders are stored in a hashmap for quick access using the order ID. The available order IDs corresponding to orders in the hashmap are stored in BtreeMap data structures (sorted key-value map), one for bids and one for asks. These are structured in levels which have a queue (FIFO) so that the oldest orders of that price have proirty over newer ones that price. When an order comes in the order book checks the BTreeMap of opposing resting orders to see if a match is available. Bids are in reverse (high to low) so that the best (highest) buy price is checked first. If this is not a valid trade then its known that no others will be valid so no more iterations are needed and the incoming order can be stored as resting. The reverse apllies for ask orders (stored low to high). This idea is what makes this implmentation fast (see benchmarking results below). The cost to insert a price level to the BTreeMap is O(log P) where P = number of price levels. To make this faster a preallocated array could be used assuming the vast majority of orders are in a certain range (BtreeMap used for outliers) and the index is calculated from price. This would have a greater memory overhead but would be faster due seqential data being in cache. Even though iterating through the array could be O(n) its still faster than finding data scattered on the heap due to the speed of cache.

Networking

```
TCP connection A ──┐
TCP connection B ──┼──► mpsc channel (cap 2000) ──► order book task
TCP connection C ──┘                                (single-threaded)
```

Each incoming connection is handled by its own Tokio task, so multiple clients can connect and send orders simultaneously without blocking each other. All orders funnel through a bounded channel into a single order book task, which processes them sequentially. This design keeps the matching engine free of locks while allowing concurrent network IO.

Press `ctrl-c` to print the final order book state and full trade log before exiting.

Currently the system only accepts orders in JSON and there is a simple test client that sends orders in JSON.

## Features

- Price-time priority (FIFO) matching engine
- Separate bid and ask sides using sorted `BTreeMap` price levels
- `VecDeque`-backed FIFO queues per price level
- Full trade recording with timestamps
- Order cancellation
- Optional verbose trade logging via feature flag
- Pretty-printed order book and trade display

## Project Structure

```
src/
├── lib.rs          — public API and module re-exports
├── order.rs        — Order and Side types, match_against logic
├── order_book.rs   — OrderBook matching engine
├── price_level.rs  — PriceLevel FIFO queue per price
├── trade.rs        — Trade type and display formatting
└── types.rs        — Price and OrderId type aliases (u64)

benches/
└── order_book.rs   — Criterion benchmark suite
```

## Usage

### Build 

```cargo build```

### Run 

Server

```bash
cargo run --bin server
```

With printing of information to terminal (performance cost)

```bash
cargo run --bin client --features verbose
```

Test client

```bash
cargo run --bin client any_test_file.json
```

```rust
use OrderBookRust::{Order, OrderBook, Side};

let mut book = OrderBook::new();

// Prices are integer pence (e.g. 10000 = £100.00)
book.incoming_ask(Order::new(1, Side::Ask, 9900, 10, 0));
book.incoming_bid(Order::new(2, Side::Bid, 10000, 10, 0));

// Matched at the resting ask price (9900)
assert_eq!(book.trades[0].price, 9900);
assert_eq!(book.trades[0].quantity, 10);
```

### Order fields

```rust
Order::new(id, side, price, quantity, timestamp)
//         u64  Side   u64    u64      u64
```

Prices are fixed-point integers representing pence. A price of `10050` represents `£100.50`.

### Cancellation

```rust
book.cancel_order(order_id);
```

Removes the order from the map. The order ID will be skipped if it appears in a price level queue during a future match cycle.

### Display

```rust
book.print_book();
book.print_trades();
```

## Matching Rules

- Incoming bids match against the lowest available ask prices first
- Incoming asks match against the highest available bid prices first
- Trades execute at the **resting order's price**
- Orders that do not cross the spread are inserted as resting orders
- Partial fills leave the remainder resting in the book
- Multiple price levels are swept in a single call if the incoming order has sufficient quantity

## Benchmarks

Run with:

```bash
cargo bench
```

### Benchmark suite

| Benchmark | Description | Result(mean)
|---|---|---|
| `single match` | One ask resting, one bid that crosses — measures end-to-end match latency | 166.78 ns
| `insert resting` (100 / 1,000 / 10,000) | Inserts N ask orders at distinct price levels with no matches | 7.89 µs / 132.41 µs / 331.85 µs	
| `sweep levels` (10 / 100 / 1,000) | Builds N resting ask levels then sweeps all of them with a single large bid | 0.56 µs / 5.27 µs / 47.30 µs
| `purge empty levels` (100 / 1,000 / 10,000) | Fully matches N orders then measures `purge_empty_levels()` cost | 3.06 µs / 31.32 µs / 331.85 µs

Results are reported by Criterion in `target/criterion/`. View the HTML report at:

```
target/criterion/report/index.html
```

## Running Tests

```bash
cargo test
```


