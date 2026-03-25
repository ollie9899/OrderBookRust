use crate::{Order, OrderId, Price, PriceLevel, Side, Trade};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};
use std::{time::{SystemTime, UNIX_EPOCH}};

/// Order book supporting price-time priority matching.
///
/// Bids are stored highest-first using `Reverse<Price>` as the BTreeMap key.
/// Asks are stored lowest-first using `Price` directly.
/// This ensures the best opposing price is always at the front of iteration,
/// allowing the matching loop to break early as soon as the spread is not crossed.
pub struct OrderBook {
    /// Bids sorted highest price first (best bid at the front of iteration).
    bids: BTreeMap<Reverse<Price>, PriceLevel>,
    /// Asks sorted lowest price first (best ask at the front of iteration).
    asks: BTreeMap<Price, PriceLevel>,
    /// Flat lookup table mapping OrderId to the full Order.
    /// Used to retrieve and mutate resting orders during matching.
    /// Cancelled orders are removed here but their IDs may remain in price
    /// level queues until the next match cycle cleans them up.
    orders_map: HashMap<OrderId, Order>,
    /// All trades produced by this book, in the order they were matched.
    pub trades: Vec<Trade>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders_map: HashMap::new(),
            trades: Vec::new(),
        }
    }

    /// Process an incoming bid (buy) order.
    ///
    /// Iterates ask levels from lowest price upward. Matches as long as the
    /// incoming bid price is greater than or equal to the resting ask price.
    /// Breaks immediately when the spread is not crossed — safe because asks
    /// are sorted ascending, so no further level can match.
    ///
    /// The timestamp is read once on the first match and reused for all trades
    /// produced by this order, ensuring consistent timestamps across a sweep.
    /// No syscall is made if the order does not match.
    pub fn incoming_bid(&mut self, mut order: Order) {
        let mut order_filled = false;
        let mut timestamp = None;
        for (price, level) in &mut self.asks {
            if (&order.price >= price) && !order_filled {
                // Read the clock when there is a guarenteed match
                let ts = timestamp.get_or_insert_with(|| {
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64
                });
                while !order_filled && !level.is_empty() {
                    // Copy the ID out of the queue so the borrow on `level`
                    // is released before we mutate `orders_map` below.
                    let ask_order_id = match level.entries.front() {
                        Some(id) => *id,
                        None => break,
                    };

                    // If the ID is not in the map the order was cancelled.
                    // Remove the dangling queue entry and continue to the next.
                    let ask_order = match self.orders_map.get_mut(&ask_order_id) {
                        Some(order) => order,
                        None => {
                                level.entries.pop_front();
                                continue;
                        }
                    };
                    let (trade, bid_filled, ask_filled) = order.match_against(ask_order, *ts);
                    #[cfg(feature = "verbose")]
                    println!("{}", trade);
                    self.trades.push(trade);
                    if ask_filled {
                        // Resting ask fully consumed — remove from both structures.
                        self.orders_map.remove(&ask_order_id);
                        level.entries.pop_front();
                    }
                    if bid_filled {
                        order_filled = true;
                    }
                }
            } else {
                // Ask price exceeds bid price — no further levels can match.
                break;
            }
        }
        // Incoming order was not fully filled — rest the remainder in the book.
        if !order_filled {
            self.insert_waiting(order);
        }
    }

    /// Process an incoming ask (sell) order.
    ///
    /// Iterates bid levels from highest price downward. Matches as long as the
    /// incoming ask price is less than or equal to the resting bid price.
    /// Breaks immediately when the spread is not crossed — safe because bids
    /// are sorted descending via `Reverse<Price>`, so no further level can match.
    ///
    /// The timestamp is read once on the first match and reused for all trades
    /// produced by this order, ensuring consistent timestamps across a sweep.
    /// No syscall is made if the order does not match.
    pub fn incoming_ask(&mut self, mut order: Order) {
        let mut order_filled = false;
        let mut timestamp = None;
        for (price, level) in &mut self.bids {
            if (&order.price <= &price.0) && !order_filled {
                // Read the clock when there is a guarenteed match
                let ts = timestamp.get_or_insert_with(|| {
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64
                });
                while !order_filled && !level.is_empty() {
                    // Copy the ID out of the queue so the borrow on level
                    // is released before we mutate `orders_map` below.
                    let bid_order_id = match level.entries.front() {
                        Some(id) => *id,
                        None => break,
                    };

                    // If the ID is not in the map the order was cancelled.
                    // Remove the dangling queue entry and continue to the next.
                    let bid_order = match self.orders_map.get_mut(&bid_order_id) {
                        Some(order) => order,
                        None => {
                                level.entries.pop_front();
                                continue;
                            }
                    };
                    let (trade, ask_filled, bid_filled) = order.match_against(bid_order, *ts);
                    #[cfg(feature = "verbose")]
                    println!("{}", trade);
                    self.trades.push(trade);
                    if bid_filled {
                        // Resting bid fully consumed — remove from both structures.
                        self.orders_map.remove(&bid_order_id);
                        level.entries.pop_front();
                    }
                    if ask_filled {
                        order_filled = true;
                    }
                }
            } else {
                // Bid price is below ask price — no further levels can match.
                break;
            }
        }
        // Incoming order was not fully filled — rest the remainder in the book.
        if !order_filled {
            self.insert_waiting(order);
        }
    }

    /// Insert a resting order into the appropriate price level.
    ///
    /// Creates a new price level if one does not already exist at this price.
    /// The order is also inserted into `orders_map` for O(1) lookup during matching.
    pub fn insert_waiting(&mut self, order: Order) {
        match order.side {
            Side::Bid => {
                if let Some(level) = self.bids.get_mut(&Reverse(order.price)) {
                    level.add_order(order.id, order.quantity);
                } else {
                    let mut level = PriceLevel::new();
                    level.add_order(order.id, order.quantity);
                    self.bids.insert(Reverse(order.price), level);
                }
            }

            Side::Ask => {
                if let Some(level) = self.asks.get_mut(&order.price) {
                    level.add_order(order.id, order.quantity);
                } else {
                    let mut level = PriceLevel::new();
                    level.add_order(order.id, order.quantity);
                    self.asks.insert(order.price, level);
                }
            }
        }
        self.orders_map.insert(order.id, order);
    }

    /// Cancel a resting order by ID.
    ///
    /// Removes the order from `orders_map` immediately. The order's ID may
    /// still be present in its price level's queue — it will be skipped and
    /// removed lazily the next time the matching loop encounters it.
    pub fn cancel_order(&mut self, order_id: OrderId) {
        self.orders_map.remove(&order_id);
    }

    /// Print all trades recorded by this book to stdout.
    pub fn print_trades(&self) {
        for trade in &self.trades {
            println!("{}", trade);
        }
    }

    /// Remove price levels whose order queues are empty.
    ///
    /// Called periodically (every 200 orders) to prevent ghost levels from
    /// accumulating after matches exhaust all orders at a price. Also called
    /// before `print_book` to ensure accurate display.
    pub fn purge_empty_levels(&mut self) {
        self.asks.retain(|_, level| !level.entries.is_empty());
        self.bids.retain(|_, level| !level.entries.is_empty());
    }

    /// print the current state of the order book to stdout.
    ///
    /// Purges empty levels before display. Asks are shown highest-first,
    /// bids are shown highest-first, with a separator between the two sides.
    /// Quantities are summed across all resting orders at each price level.
    pub fn print_book(&mut self) {
        self.purge_empty_levels();

        println!("┌─────────────────────────────────────────┐");
        println!("│           ORDER BOOK                    │");
        println!("├──────────────┬──────────────┬───────────┤");
        println!("│     SIDE     │    PRICE     │    QTY    │");
        println!("├──────────────┼──────────────┼───────────┤");

        // Collect asks and reverse so the highest ask is displayed at the top.
        let asks: Vec<_> = self.asks.iter().collect();
        for (price, level) in asks.iter().rev() {
            let total_qty: u64 = level.entries
                .iter()
                .filter_map(|id| self.orders_map.get(id))
                .map(|o| o.quantity)
                .sum();
            println!("│     ASK      │  {:>10.2}  │  {:>7}  │",
                **price as f64 / 100.0,
                total_qty
            );
        }

        println!("├──────────────┼──────────────┼───────────┤");

        for (price, level) in &self.bids {
            let total_qty: u64 = level.entries
                .iter()
                .filter_map(|id| self.orders_map.get(id))
                .map(|o| o.quantity)
                .sum();
            println!("│     BID      │  {:>10.2}  │  {:>7}  │",
                price.0 as f64 / 100.0,
                total_qty
            );
        }

        println!("└──────────────┴──────────────┴───────────┘");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Order, Side};

    fn make_order(id: u64, side: Side, price: u64, qty: u64) -> Order {
        Order::new(id, side, price, qty, 0)
    }

    #[test]
    fn test_resting_bid_inserted() {
        let mut book = OrderBook::new();
        book.incoming_bid(make_order(1, Side::Bid, 10000, 5));
        assert!(book.bids.contains_key(&std::cmp::Reverse(10000)));
        assert_eq!(book.trades.len(), 0);
    }

    #[test]
    fn test_resting_ask_inserted() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 5));
        assert!(book.asks.contains_key(&10000));
        assert_eq!(book.trades.len(), 0);
    }

    #[test]
    fn test_no_match_bid_below_ask() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10100, 5));
        book.incoming_bid(make_order(2, Side::Bid, 9900, 5));
        assert_eq!(book.trades.len(), 0);
        assert!(book.asks.contains_key(&10100));
        assert!(book.bids.contains_key(&std::cmp::Reverse(9900)));
    }

    #[test]
    fn test_exact_match() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 5));
        book.incoming_bid(make_order(2, Side::Bid, 10000, 5));
        assert_eq!(book.trades.len(), 1);
        assert_eq!(book.trades[0].quantity, 5);
        assert_eq!(book.trades[0].price, 10000);
        assert!(book.asks.is_empty() || book.asks.get(&10000).map(|l| l.entries.is_empty()).unwrap_or(true));
        assert!(book.bids.is_empty() || book.bids.get(&std::cmp::Reverse(10000)).map(|l| l.entries.is_empty()).unwrap_or(true));
    }

    #[test]
    fn test_bid_above_ask_matches_at_ask_price() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 9900, 5));
        book.incoming_bid(make_order(2, Side::Bid, 10100, 5));
        assert_eq!(book.trades.len(), 1);
        assert_eq!(book.trades[0].price, 9900);
        assert_eq!(book.trades[0].quantity, 5);
    }

    #[test]
    fn test_ask_below_bid_matches_at_bid_price() {
        let mut book = OrderBook::new();
        book.incoming_bid(make_order(1, Side::Bid, 10100, 5));
        book.incoming_ask(make_order(2, Side::Ask, 9900, 5));
        assert_eq!(book.trades.len(), 1);
        assert_eq!(book.trades[0].price, 10100);
    }

    #[test]
    fn test_partial_fill_bid_larger_than_ask() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 3));
        book.incoming_bid(make_order(2, Side::Bid, 10000, 10));
        assert_eq!(book.trades.len(), 1);
        assert_eq!(book.trades[0].quantity, 3);
        let level = book.bids.get(&std::cmp::Reverse(10000)).unwrap();
        let resting = book.orders_map.get(level.entries.front().unwrap()).unwrap();
        assert_eq!(resting.quantity, 7);
    }

    #[test]
    fn test_partial_fill_ask_larger_than_bid() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 10));
        book.incoming_bid(make_order(2, Side::Bid, 10000, 3));
        assert_eq!(book.trades.len(), 1);
        assert_eq!(book.trades[0].quantity, 3);
        let level = book.asks.get(&10000).unwrap();
        let resting = book.orders_map.get(level.entries.front().unwrap()).unwrap();
        assert_eq!(resting.quantity, 7);
    }

    #[test]
    fn test_bid_sweeps_multiple_ask_levels() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 9800, 5));
        book.incoming_ask(make_order(2, Side::Ask, 9850, 5));
        book.incoming_ask(make_order(3, Side::Ask, 9900, 5));
        book.incoming_bid(make_order(4, Side::Bid, 10000, 15));
        assert_eq!(book.trades.len(), 3);
        assert_eq!(book.trades.iter().map(|t| t.quantity).sum::<u64>(), 15);
    }

    #[test]
    fn test_ask_sweeps_multiple_bid_levels() {
        let mut book = OrderBook::new();
        book.incoming_bid(make_order(1, Side::Bid, 10200, 5));
        book.incoming_bid(make_order(2, Side::Bid, 10150, 5));
        book.incoming_bid(make_order(3, Side::Bid, 10100, 5));
        book.incoming_ask(make_order(4, Side::Ask, 10000, 15));
        assert_eq!(book.trades.len(), 3);
        assert_eq!(book.trades.iter().map(|t| t.quantity).sum::<u64>(), 15);
    }

    #[test]
    fn test_bid_partially_sweeps_levels() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 9800, 5));
        book.incoming_ask(make_order(2, Side::Ask, 9850, 5));
        book.incoming_ask(make_order(3, Side::Ask, 9900, 5));
        book.incoming_bid(make_order(4, Side::Bid, 9860, 10));
        assert_eq!(book.trades.len(), 2);
        assert_eq!(book.trades.iter().map(|t| t.quantity).sum::<u64>(), 10);
        assert!(book.asks.contains_key(&9900));
    }

    #[test]
    fn test_fifo_within_level() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 3));
        book.incoming_ask(make_order(2, Side::Ask, 10000, 3));
        book.incoming_bid(make_order(3, Side::Bid, 10000, 3));
        assert_eq!(book.trades.len(), 1);
        assert_eq!(book.trades[0].sell_order_id, 1);
        let level = book.asks.get(&10000).unwrap();
        assert_eq!(*level.entries.front().unwrap(), 2);
    }

    #[test]
    fn test_purge_removes_empty_levels() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 5));
        book.incoming_bid(make_order(2, Side::Bid, 10000, 5));
        book.purge_empty_levels();
        assert!(!book.asks.contains_key(&10000));
    }

    #[test]
    fn test_print_book_purges_before_display() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 5));
        book.incoming_bid(make_order(2, Side::Bid, 10000, 5));
        book.print_book();
        assert!(book.asks.is_empty());
    }

    #[test]
    fn test_trade_ids_correct() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 10000, 5));
        book.incoming_bid(make_order(2, Side::Bid, 10000, 5));
        assert_eq!(book.trades[0].buy_order_id, 2);
        assert_eq!(book.trades[0].sell_order_id, 1);
    }

    #[test]
    fn test_multiple_trades_same_incoming_order() {
        let mut book = OrderBook::new();
        book.incoming_ask(make_order(1, Side::Ask, 9900, 3));
        book.incoming_ask(make_order(2, Side::Ask, 9950, 3));
        book.incoming_bid(make_order(3, Side::Bid, 10000, 6));
        assert_eq!(book.trades.len(), 2);
        assert_eq!(book.trades[0].buy_order_id, 3);
        assert_eq!(book.trades[1].buy_order_id, 3);
    }

    #[test]
    fn test_trade_count_accumulates() {
        let mut book = OrderBook::new();
        for i in 0..10 {
            book.incoming_ask(make_order(i * 2, Side::Ask, 10000, 1));
            book.incoming_bid(make_order(i * 2 + 1, Side::Bid, 10000, 1));
        }
        assert_eq!(book.trades.len(), 10);
    }
}
