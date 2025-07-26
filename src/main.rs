mod adapters;
mod arbitrage;
mod helpers;
mod settings;

use anyhow::Result;
use log::{error, info};
use tokio::sync::watch;

use crate::adapters::bybit::run_bybit_listener;
use crate::adapters::hyperswap::run_hyperswap_listener;
use crate::arbitrage::PriceData;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cfg = settings::Settings::load()?;

    println!("{:#?}", cfg);

    let (bybit_tx, _bybit_rx) = watch::channel::<Option<PriceData>>(None);
    let (hyperswap_tx, hyperswap_rx) = watch::channel::<Option<PriceData>>(None);

    // inititate bybit websocket
    info!("initializing bybit rpc ws connection...");
    let bybit_task = tokio::spawn(run_bybit_listener(bybit_tx));

    info!("initializing hyperswap price fetcher...");
    let dex_task = tokio::spawn(run_hyperswap_listener(hyperswap_tx));


    tokio::select! {
        result = bybit_task => {
            if let Err(e) = result {
                error!("bybit listener task failed: {}", e);
            }
        }
        result = dex_task => {
            if let Err(e) = result {
                error!("dex price fetcher task failed: {}", e);
            }
        }
    }

    Ok(())
}