mod adapters;
mod helpers;
mod settings;

use anyhow::Result;
use log::{error, info};

use crate::adapters::bybit::run_bybit_listener;
use crate::adapters::hyperswap::get_quote;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cfg = settings::Settings::load()?;

    println!("{:#?}", cfg);

    get_quote().await?;

    // inititate bybit websocket
    info!("initializing bybit rpc ws connection...");
    let bybit_task = tokio::spawn(run_bybit_listener());

    // Wait for the bybit task to complete (it runs indefinitely)
    if let Err(e) = bybit_task.await {
        error!("bybit listener task failed: {}", e);
    }

    Ok(())
}