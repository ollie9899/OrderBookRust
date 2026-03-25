use OrderBookRust::{Order, OrderBook, Side};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn make_order(id: u64, side: Side, price: u64, qty: u64) -> Order {
    Order::new(id, side, price, qty, 0)
}

fn bench_single_match(c: &mut Criterion) {
    c.bench_function("single match", |b| {
        b.iter(|| {
            let mut order_book = OrderBook::new();
            order_book.incoming_ask(make_order(1, Side::Ask, 10000, 10));
            order_book.incoming_bid(black_box(make_order(2, Side::Bid, 10100, 10)));
        });
    });
}
fn bench_insert_resting(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert resting");
    for size in [100u64, 1000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let book = OrderBook::new();  // ← moved to setup
                    let orders: Vec<Order> = (0..size)
                        .map(|i| make_order(i, Side::Ask, 10000 + i, 10))
                        .collect();
                    (book, orders)               // ← return both
                },
                |(mut book, orders)| {           // ← destructure both
                    for order in orders {
                        book.incoming_ask(black_box(order));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_sweep(c: &mut Criterion) {
    let mut group = c.benchmark_group("sweep levels");
    for levels in [10, 100, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(levels),
            &levels,
            |b, &levels| {
                b.iter_batched(
                    || {
                        // setup — build a order_book with N ask levels
                        let mut order_book = OrderBook::new();
                        for i in 0..levels {
                            order_book.incoming_ask(make_order(i, Side::Ask, 10000 + i, 1));
                        }
                        order_book
                    },
                    |mut order_book| {
                        // measure — one bid that sweeps all levels
                        order_book.incoming_bid(black_box(make_order(
                            levels + 1,
                            Side::Bid,
                            10000 + levels + 100,
                            levels,
                        )));
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_purge(c: &mut Criterion) {
    let mut group = c.benchmark_group("purge empty levels");
    for size in [100, 1000, 10_000] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter_batched(
                || {
                    let mut order_book = OrderBook::new();
                    for i in 0..size {
                        order_book.incoming_ask(make_order(i * 2, Side::Ask, 10000 + i, 1));
                        order_book.incoming_bid(make_order(
                            i * 2 + 1,
                            Side::Bid,
                            10000 + i + 100,
                            1,
                        ));
                    }
                    order_book
                },
                |mut order_book| {
                    order_book.purge_empty_levels();
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_single_match,
    bench_insert_resting,
    bench_sweep,
    bench_purge,
);
criterion_main!(benches);
