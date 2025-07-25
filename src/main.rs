mod adapters;
mod arbitrage;
mod helpers;
mod settings;

use anyhow::Result;
use log::{error, info};
use tokio::sync::watch;

use crate::adapters::bybit::run_listener;
use crate::adapters::hyperswap::get_quote;
use crate::arbitrage::PriceData;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cfg = settings::Settings::load()?;

    println!("{:#?}", cfg);

    get_quote().await?;

    let (bybit_tx, _bybit_rx) = watch::channel::<Option<PriceData>>(None);

    // inititate bybit websocket
    info!("initializing bybit rpc ws connection...");
    let bybit_task = tokio::spawn(run_listener(bybit_tx));

    // Wait for the bybit task to complete (it runs indefinitely)
    if let Err(e) = bybit_task.await {
        error!("bybit listener task failed: {}", e);
    }

    Ok(())
}