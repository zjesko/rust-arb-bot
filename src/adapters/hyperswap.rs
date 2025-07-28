use std::sync::Arc;
use std::time::Duration;

use alloy::{
    network::Ethereum,
    primitives::{Bytes, U256},
    providers::{Provider, ProviderBuilder},
};

use revm::{
    database::{AlloyDB, CacheDB, WrapDatabaseAsync},
    state::Bytecode,
};

use anyhow::Result;
use log::{error, info};
use tokio::time::{sleep, Instant};
use tokio::sync::watch;

use crate::settings;
use crate::arbitrage::{PriceData};
use crate::helpers::revm::{init_cache_db, init_account_with_bytecode, insert_mapping_storage_slot, hydrate_pool_state, revm_call};
use crate::helpers::abi::{ONE_ETHER, quote_calldata, decode_quote_response, quote_exact_output_calldata, decode_quote_output_response, build_tx};

pub async fn run_hyperswap_listener(tx: watch::Sender<Option<PriceData>>) -> Result<()> {
    let cfg: settings::Settings = settings::Settings::load()?;

    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let provider = Arc::new(provider);

    // initialize cache_db
    let mut cache_db = init_cache_db(provider.clone());

    // mock ERC‑20s with generic_erc20 bytecode
    let mocked_erc20 = include_str!("../bytecode/generic_erc20.hex");
    let mocked_erc20 = mocked_erc20.parse::<Bytes>().unwrap();
    let mocked_erc20 = Bytecode::new_raw(mocked_erc20);
    init_account_with_bytecode(cfg.weth_addr, mocked_erc20.clone(), &mut cache_db).await?;
    // init_account_with_bytecode(cfg.usdt_addr, mocked_erc20.clone(), &mut cache_db).await?;

    // mock pool state balances
    let big = U256::MAX / U256::from(2);
    insert_mapping_storage_slot(cfg.weth_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db).await?;
    insert_mapping_storage_slot(cfg.usdt_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db).await?;

    loop {
        // match fetch_quote(&tx, &provider, &cfg).await {
            // Ok(_) => {},
            // Err(e) => error!("DEX price fetch error: {}", e),
        // }
        match fetch_quote_revm(&cfg, provider.clone(), &tx, &mut cache_db).await {
            Ok(_) => {},
            Err(e) => error!("DEX price fetch error: {}", e),
        }
        
        // Fetch DEX prices every 1 seconds
        sleep(Duration::from_millis(1000)).await;
    }
}

pub async fn fetch_quote(
    cfg: &settings::Settings,
    provider: &Arc<impl Provider>, 
    price_tx: &watch::Sender<Option<PriceData>>, 
) -> Result<()> {
    let volume = ONE_ETHER;
    let base_fee = provider.get_gas_price().await?;
    
    let start = Instant::now();

    let sell_weth_calldata = quote_calldata(
        cfg.weth_addr, 
        cfg.usdt_addr, 
        volume, 
        cfg.hyperswap_fee_bps
    );
    let sell_response = provider.call(build_tx(
        cfg.quoter_v2_addr, 
        cfg.self_addr, 
        sell_weth_calldata, 
        base_fee
    )).await?;

    let buy_weth_calldata = quote_exact_output_calldata(
        cfg.usdt_addr, 
        cfg.weth_addr, 
        volume, 
        cfg.hyperswap_fee_bps
    );
    let buy_response = provider.call(build_tx(
        cfg.quoter_v2_addr, 
        cfg.self_addr, 
        buy_weth_calldata, 
        base_fee
    )).await?;

    let price_data = PriceData {
        bid: decode_quote_response(sell_response)? as f64 / 1e6,
        ask: decode_quote_output_response(buy_response)? as f64 / 1e6
    };

    if let Err(e) = price_tx.send(Some(price_data.clone())) {
        error!("failed to send DEX price update: {}", e);
    }

    info!("WHYPE/USDT: {:.2} / {:.2} (took {:.2}ms)", price_data.bid, price_data.ask, start.elapsed().as_millis());

    Ok(())
}

// REVM-based quote fetching for better performance
pub async fn fetch_quote_revm<P: Provider + Clone>(
    cfg: &settings::Settings,
    provider: Arc<P>,
    price_tx: &watch::Sender<Option<PriceData>>, 
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
) -> Result<()> {
    let volume = ONE_ETHER;

    // ensure pool state is up to date
    hydrate_pool_state(cache_db, &provider, cfg.pool_addr).await?;

    let start = Instant::now();

    let sell_weth_calldata = quote_calldata(
        cfg.weth_addr, 
        cfg.usdt_addr, 
        volume, 
        cfg.hyperswap_fee_bps
    );
    let sell_response = revm_call(cfg.self_addr, cfg.quoter_v2_addr, sell_weth_calldata, cache_db)?;

    let buy_weth_calldata = quote_exact_output_calldata(
        cfg.usdt_addr, 
        cfg.weth_addr, 
        volume, 
        cfg.hyperswap_fee_bps
    );
    let ask_response = revm_call(cfg.self_addr, cfg.quoter_v2_addr, buy_weth_calldata, cache_db)?;

    let price_data = PriceData {
        bid: decode_quote_response(sell_response)? as f64 / 1e6,
        ask: decode_quote_output_response(ask_response)? as f64 / 1e6,
    };

    if let Err(e) = price_tx.send(Some(price_data.clone())) {
        error!("failed to send DEX price update: {}", e);
    }

    info!("WHYPE/USDT: {:.2} / {:.2} (took {:.2}ms REVM)", price_data.bid, price_data.ask, start.elapsed().as_millis());

    Ok(())
}
