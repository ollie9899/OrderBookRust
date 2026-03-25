use std::collections::VecDeque;
use crate::OrderId;

/// A queue of orders all resting at the same price point.
///
/// Orders are maintained in FIFO order — the first order to arrive
/// at a price level is the first to be filled (price-time priority).
pub struct PriceLevel {
    /// Order IDs queued at this price, front = oldest (next to fill)
    pub entries: VecDeque<OrderId>,
    /// Running total of all resting quantity at this level.
    pub count: u64,
}

impl PriceLevel {
    /// Create a new empty price level.
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            count: 0,
        }
    }

    /// Add a resting order to the back of the queue.
    /// Quantity is added to the running total.
    pub fn add_order(&mut self, order_id: OrderId, quantity: u64) {
        self.entries.push_back(order_id);
        self.count += quantity;
    }

    /// Returns true if there are no resting orders at this level.
    /// Used by the matching loop to know when a level is exhausted
    /// and by purge to know when the level can be removed from the book.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_is_empty() {
        let level = PriceLevel::new();
        assert!(level.is_empty());
        assert_eq!(level.count, 0);
    }

    #[test]
    fn test_add_multiple_orders() {
        let mut level = PriceLevel::new();
        level.add_order(1, 10);
        level.add_order(2, 5);
        level.add_order(3, 20);
        assert_eq!(level.entries.len(), 3);
        assert_eq!(level.count, 35);
        assert!(!level.is_empty());
    }

    #[test]
    fn test_fifo_all_orders_in_sequence() {
        let mut level = PriceLevel::new();
        for i in 0..5 {
            level.add_order(i, 1);
        }
        for i in 0..5 {
            assert_eq!(level.entries.pop_front().unwrap(), i);
        }
    }

    #[test]
    fn test_is_empty_after_all_popped() {
        let mut level = PriceLevel::new();
        level.add_order(1, 5);
        level.add_order(2, 5);
        level.entries.pop_front();
        assert!(!level.is_empty());
        level.entries.pop_front();
        assert!(level.is_empty());
    }

    #[test]
    fn test_large_quantity() {
        let mut level = PriceLevel::new();
        level.add_order(1, u64::MAX / 2);
        level.add_order(2, 1);
        assert_eq!(level.count, u64::MAX / 2 + 1);
    }
}