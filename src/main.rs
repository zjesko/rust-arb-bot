mod adapters;
mod arbitrage;
mod helpers;
mod settings;

use anyhow::Result;
use log::{error, info};
use tokio::sync::watch;
use alloy::providers::ProviderBuilder;
use std::sync::Arc;

use crate::adapters::bybit::run_bybit_listener;
use crate::adapters::hyperswap::run_hyperswap_listener;
use crate::arbitrage::{PriceData, ArbEngine};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cfg = settings::Settings::load()?;

    println!("{:#?}", cfg);

    // Create provider for real-time gas price fetching
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let provider = Arc::new(provider);

    let (bybit_tx, bybit_rx) = watch::channel::<Option<PriceData>>(None);
    let (hyperswap_tx, hyperswap_rx) = watch::channel::<Option<PriceData>>(None);

    info!("initializing bybit rpc ws connection...");
    let bybit_task = tokio::spawn(run_bybit_listener(bybit_tx));

    info!("initializing hyperswap price fetcher...");
    let dex_task = tokio::spawn(run_hyperswap_listener(hyperswap_tx));

    info!("initializing arbitrage detection engine...");

    let mut arbitrage_engine = ArbEngine::new(
        cfg.clone(),
        bybit_rx,
        hyperswap_rx,
        provider,
    );

    let arbitrage_task = tokio::spawn(async move {
        if let Err(e) = arbitrage_engine.run().await {
            error!("arbitrage engine error: {}", e);
        }
    });


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
        result = arbitrage_task => {
            if let Err(e) = result {
                error!("arbitrage engine task failed: {}", e);
            }
        }
    }

    Ok(())
}