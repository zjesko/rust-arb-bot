use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct PriceData {
    pub bid: f64,
    pub ask: f64,
    pub timestamp: u64,
}

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
} 