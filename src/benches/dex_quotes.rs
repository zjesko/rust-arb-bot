use std::sync::Arc;
use std::time::Instant;

use alloy::providers::ProviderBuilder;
use revm::{primitives::{Bytes, U256}, state::Bytecode};
use anyhow::Result;
use tokio::sync::watch;
use log::info;

use rust_arb_bot::adapters::hyperswap::{fetch_quote, fetch_quote_revm};
use rust_arb_bot::arbitrage::PriceData;
use rust_arb_bot::helpers::revm::{
    init_account_with_bytecode, init_cache_db, insert_mapping_storage_slot,
};
use rust_arb_bot::settings::Settings;

pub async fn run_benchmark() -> Result<()> {
    info!("DEX Quotes Benchmark");
    info!("=======================");

    let cfg = Settings::load()?;
    let provider = Arc::new(ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?));
    let (price_tx, _price_rx) = watch::channel(None::<PriceData>);

    let mut cache_db = init_cache_db(provider.clone());
    let mut cache_db_unmocked = init_cache_db(provider.clone());

    // Setup mocked ERC20 contracts
    let mocked_erc20 = include_str!("../bytecode/generic_erc20.hex").parse::<Bytes>()?;
    let mocked_erc20 = Bytecode::new_raw(mocked_erc20);
    init_account_with_bytecode(cfg.weth_addr, mocked_erc20.clone(), &mut cache_db).await?;

    let big = U256::MAX / U256::from(2);
    insert_mapping_storage_slot(cfg.weth_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db).await?;
    insert_mapping_storage_slot(cfg.usdt_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db).await?;

    // Benchmark fetch_quote
    info!("1. Standard fetch_quote:");
    let start = Instant::now();
    fetch_quote(&cfg, &provider, &price_tx).await?;
    info!("First call: {:?}", start.elapsed());

    let start = Instant::now();
    for _ in 0..10 {
        fetch_quote(&cfg, &provider, &price_tx).await?;
    }
    info!("10 calls avg: {:?}", start.elapsed() / 10);

    // Benchmark fetch_quote_revm (no mocking)
    info!("2. REVM without mocking:");
    let start = Instant::now();
    fetch_quote_revm(&cfg, provider.clone(), &price_tx, &mut cache_db_unmocked).await?;
    info!("First call: {:?}", start.elapsed());

    let start = Instant::now();
    for _ in 0..10 {
        fetch_quote_revm(&cfg, provider.clone(), &price_tx, &mut cache_db_unmocked).await?;
    }
    info!("10 calls avg: {:?}", start.elapsed() / 10);

    // Benchmark fetch_quote_revm (with mocking)
    info!("3. REVM with mocking:");
    let start = Instant::now();
    fetch_quote_revm(&cfg, provider.clone(), &price_tx, &mut cache_db).await?;
    info!("First call: {:?}", start.elapsed());

    let start = Instant::now();
    for _ in 0..10 {
        fetch_quote_revm(&cfg, provider.clone(), &price_tx, &mut cache_db).await?;
    }
    info!("10 calls avg: {:?}", start.elapsed() / 10);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    run_benchmark().await
}
