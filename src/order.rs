use serde::{Deserialize, Serialize};
use crate::Trade;

/// Which side of the order book an order belongs to.
/// Bids are buy orders, asks are sell orders.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Bid, // buy order
    Ask, // sell order
}

/// A single order placed into the order book.
/// Price is stored as an integer scaled by 100 (e.g. £100.50 = 10050)
/// to avoid floating point rounding errors.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Order {
    // unique order identifier
    pub id: u64,
    // bid or ask
    pub side: Side,
    pub price: u64,
    // remaining unfilled quantity
    pub quantity: u64, 
    // timestamp in ms
    pub timestamp: u64,
}

impl Order {
    pub fn new(id: u64, side: Side, price: u64, quantity: u64, timestamp: u64) -> Self {
        Self {
            id,
            side,
            price,
            quantity,
            timestamp,
        }
    }

    /// Attempt to match this incoming order against a resting order.
    ///
    /// Fills as much quantity as possible — limited by whichever side has less.
    /// Decrements quantity on both orders by the filled amount.
    ///
    /// Returns a tuple of:
    /// - `Trade`  — the executed trade record
    /// - `bool`   — whether the incoming order (`self`) is now fully filled
    /// - `bool`   — whether the resting order is now fully filled
    pub fn match_against(&mut self, resting: &mut Order, timestamp: u64) -> (Trade, bool, bool) {
        // fill as much as both sides allow
        let qty = self.quantity.min(resting.quantity);

        // trade is always recorded as (buy_id, sell_id) regardless of which
        // side is the incoming order and which is resting
        let (bid_id, ask_id) = match self.side {
            Side::Bid => (self.id, resting.id),
            Side::Ask => (resting.id, self.id),
        };

        // trade executes at the resting order's price — the incomer accepts
        // whatever price was already in the book
        let trade = Trade::new(bid_id, ask_id, resting.price, qty, timestamp);

        // decrement remaining quantity on both sides
        self.quantity -= qty;
        resting.quantity -= qty;

        (trade, self.quantity == 0, resting.quantity == 0)
    }
}