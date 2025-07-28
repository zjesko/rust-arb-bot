mod adapters;
mod arbitrage;
mod helpers;
mod settings;

use alloy::providers::ProviderBuilder;
use anyhow::Result;
use log::{error, info};
use std::sync::Arc;
use tokio::sync::watch;

use crate::adapters::bybit::run_bybit_listener;
use crate::adapters::gateio::run_gateio_listener;
use crate::adapters::hyperswap::run_hyperswap_listener;
use crate::arbitrage::{ArbEngine, PriceData};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let cfg = settings::Settings::load()?;

    println!("{:#?}", cfg);

    // Create provider for real-time gas price fetching
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let provider = Arc::new(provider);

    let (bybit_tx, bybit_rx) = watch::channel::<Option<PriceData>>(None);
    let (gateio_tx, gateio_rx) = watch::channel::<Option<PriceData>>(None);
    let (hyperswap_tx, hyperswap_rx) = watch::channel::<Option<PriceData>>(None);

    info!("initializing bybit rpc ws connection...");
    let bybit_task = tokio::spawn(run_bybit_listener(bybit_tx));

    info!("initializing gateio rpc ws connection...");
    let gateio_task = tokio::spawn(run_gateio_listener(gateio_tx));

    info!("initializing hyperswap price fetcher...");
    let dex_task = tokio::spawn(run_hyperswap_listener(hyperswap_tx));

    info!("initializing bybit-hyperswap arbitrage detection engine...");
    let mut bybit_arbitrage_engine = ArbEngine::new(cfg.clone(), bybit_rx, hyperswap_rx.clone(), provider.clone());

    info!("initializing gateio-hyperswap arbitrage detection engine...");
    let mut gateio_arbitrage_engine = ArbEngine::new(cfg.clone(), gateio_rx, hyperswap_rx, provider);

    let bybit_arbitrage_task = tokio::spawn(async move {
        if let Err(e) = bybit_arbitrage_engine.run().await {
            error!("bybit arbitrage engine error: {}", e);
        }
    });

    let gateio_arbitrage_task = tokio::spawn(async move {
        if let Err(e) = gateio_arbitrage_engine.run().await {
            error!("gateio arbitrage engine error: {}", e);
        }
    });

    tokio::select! {
        result = bybit_task => {
            if let Err(e) = result {
                error!("bybit listener task failed: {}", e);
            }
        }
        result = gateio_task => {
            if let Err(e) = result {
                error!("gateio listener task failed: {}", e);
            }
        }
        result = dex_task => {
            if let Err(e) = result {
                error!("dex price fetcher task failed: {}", e);
            }
        }
        result = bybit_arbitrage_task => {
            if let Err(e) = result {
                error!("bybit arbitrage engine task failed: {}", e);
            }
        }
        result = gateio_arbitrage_task => {
            if let Err(e) = result {
                error!("gateio arbitrage engine task failed: {}", e);
            }
        }
    }

    Ok(())
}
