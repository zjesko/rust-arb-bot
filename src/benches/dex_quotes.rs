use std::sync::Arc;
use std::time::Instant;

use alloy::providers::ProviderBuilder;

use revm::{
    primitives::{Bytes, U256},
    state::Bytecode,
};

use anyhow::Result;
use tokio::sync::watch;

use rust_arb_bot::adapters::hyperswap::{fetch_quote, fetch_quote_revm};
use rust_arb_bot::arbitrage::PriceData;
use rust_arb_bot::helpers::revm::{
    init_account_with_bytecode, init_cache_db, insert_mapping_storage_slot,
};
use rust_arb_bot::settings::Settings;

pub async fn run_benchmark() -> Result<()> {
    println!("Starting DEX Quotes Benchmark");
    println!("============================");

    let cfg = Settings::load()?;
    println!("cfg: {:?}", cfg);

    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let provider = Arc::new(provider);

    let (price_tx, _price_rx) = watch::channel(None::<PriceData>);

    let mut cache_db = init_cache_db(provider.clone());

    // Initialize mocked ERC20 contracts for REVM
    let mocked_erc20 = include_str!("../bytecode/generic_erc20.hex");
    let mocked_erc20 = mocked_erc20.parse::<Bytes>().unwrap();
    let mocked_erc20 = Bytecode::new_raw(mocked_erc20);
    init_account_with_bytecode(cfg.weth_addr, mocked_erc20.clone(), &mut cache_db).await?;

    let big = U256::MAX / U256::from(2);
    insert_mapping_storage_slot(cfg.weth_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db)
        .await?;
    insert_mapping_storage_slot(cfg.usdt_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db)
        .await?;

    let checkpoints = [1, 5, 20];
    let max_calls = 20;

    println!("\n--- benchmarking fetch_quote ---");

    // Benchmark fetch_quote
    let start = Instant::now();
    for i in 1..=max_calls {
        if let Err(e) = fetch_quote(&cfg, &provider, &price_tx).await {
            eprintln!("fetch_quote error on call {}: {}", i, e);
        }

        // Report at checkpoints
        if checkpoints.contains(&i) {
            let elapsed = start.elapsed();
            let avg_time = elapsed / i;
            println!(
                "fetch_quote:      {} calls took {:?} (avg: {:?} per call)",
                i, elapsed, avg_time
            );
        }
    }

    println!("\n--- benchmarking fetch_quote_revm ---");

    // Benchmark fetch_quote_revm
    let start = Instant::now();
    for i in 1..=max_calls {
        if let Err(e) = fetch_quote_revm(&cfg, provider.clone(), &price_tx, &mut cache_db).await {
            eprintln!("fetch_quote_revm error on call {}: {}", i, e);
        }

        // Report at checkpoints
        if checkpoints.contains(&i) {
            let elapsed = start.elapsed();
            let avg_time = elapsed / i;
            println!(
                "fetch_quote_revm: {} calls took {:?} (avg: {:?} per call)",
                i, elapsed, avg_time
            );
        }
    }

    println!("\nBenchmark completed!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    run_benchmark().await
}
