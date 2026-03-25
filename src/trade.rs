use chrono::{DateTime, TimeZone, Utc};
use std::fmt;

#[inline]
fn format_timestamp(ts_millis: u64) -> String {
    let secs = (ts_millis / 1000) as i64;
    let millis = (ts_millis % 1000) as u32;
    let dt: DateTime<Utc> = Utc.timestamp_opt(secs, millis * 1_000_000).unwrap();
    dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub buy_order_id: u64,
    pub sell_order_id: u64,
    pub price: u64,
    pub quantity: u64,
    pub timestamp: u64,
}

impl Trade {
    pub fn new(buy_order_id: u64, sell_order_id: u64, price: u64, quantity: u64, timestamp: u64) -> Self {
        Self {
            buy_order_id,
            sell_order_id,
            price,
            quantity,
            timestamp,
        }
    }
}
impl fmt::Display for Trade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let time = format_timestamp(self.timestamp);
        write!(
            f,
            "┌─ TRADE ─────────────────────────────┐\n\
             │  buy:  #{:<6}  sell: #{:<6}       │\n\
             │  price:  ${:<10.2}                │\n\
             │  qty:    {:<10}                 │\n\
             │  time:  {}     │\n\
             └─────────────────────────────────────┘",
            self.buy_order_id,
            self.sell_order_id,
            self.price as f64 / 100.0,
            self.quantity,
            time
        )
    }
}
