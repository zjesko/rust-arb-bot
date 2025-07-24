use std::time::Duration;

use alloy::{
    primitives::U256,
    uint,
};
use tokio::time::Instant;

pub static ONE_ETHER: U256 = uint!(1_000_000_000_000_000_000_U256);

pub fn measure_start(label: &str) -> (String, Instant) {
    (label.to_string(), Instant::now())
}

pub fn measure_end(start: (String, Instant)) -> Duration {
    let elapsed = start.1.elapsed();
    println!("Elapsed: {:.2?} for '{}'", elapsed, start.0);
    elapsed
}
