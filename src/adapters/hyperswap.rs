use std::sync::Arc;
use std::time::Duration;

use alloy::{
    network::{TransactionBuilder, Ethereum},
    primitives::{Address, Bytes, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
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
use crate::helpers::revm::{init_cache_db, init_account_with_bytecode, insert_mapping_storage_slot, hydrate_pool_state, revm_call, revm_revert};
use crate::helpers::abi::{ONE_ETHER, quote_calldata, decode_quote_response, get_amount_out_calldata, decode_get_amount_out_response};


pub fn build_tx(to: Address, from: Address, calldata: Bytes, base_fee: u128) -> TransactionRequest {
    TransactionRequest::default()
        .to(to)
        .from(from)
        .with_input(calldata)
        .nonce(0)
        .gas_limit(1000000)
        .max_fee_per_gas(base_fee)
        .max_priority_fee_per_gas(0)
        .with_chain_id(999) // Hyperliquid mainnet chain ID
        .build_unsigned()
        .unwrap()
        .into()
}


pub async fn run_hyperswap_listener(tx: watch::Sender<Option<PriceData>>) -> Result<()> {
    let cfg: settings::Settings = settings::Settings::load()?;

    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let provider = Arc::new(provider);

    // Initialize cache_db once outside the loop for better performance
    let mut cache_db = init_cache_db(provider.clone());

    // replace ERC‑20s with 300‑byte “generic_erc20” stub
    let mocked_erc20 = include_str!("../bytecode/generic_erc20.hex");
    let mocked_erc20 = mocked_erc20.parse::<Bytes>().unwrap();
    let mocked_erc20 = Bytecode::new_raw(mocked_erc20);
    init_account_with_bytecode(cfg.weth_addr.parse()?, mocked_erc20.clone(), &mut cache_db).await?;
    // init_account_with_bytecode(cfg.usdt_addr.parse()?, mocked_erc20.clone(), &mut cache_db).await?;

    let big = U256::MAX / U256::from(2);
    insert_mapping_storage_slot(cfg.weth_addr.parse()?, U256::ZERO, cfg.pool_addr.parse()?, big, &mut cache_db).await?;
    insert_mapping_storage_slot(cfg.usdt_addr.parse()?, U256::ZERO, cfg.pool_addr.parse()?, big, &mut cache_db).await?;

    // let mocked_custom_quoter = include_str!("../bytecode/custom_quoter.hex");
    // let mocked_custom_quoter = mocked_custom_quoter.parse::<Bytes>().unwrap();
    // let mocked_custom_quoter = Bytecode::new_raw(mocked_custom_quoter);
    // init_account_with_bytecode(cfg.quoter_custom_addr.parse()?, mocked_custom_quoter.clone(), &mut cache_db).await?;


    loop {
        // match fetch_quote(&tx, &provider, &cfg).await {
            // Ok(_) => {},
            // Err(e) => error!("DEX price fetch error: {}", e),
        // }
        match fetch_quote_revm(&tx, &cfg, &mut cache_db, provider.clone()).await {
            Ok(_) => {},
            Err(e) => error!("DEX price fetch error: {}", e),
        }
        
        // Fetch DEX prices every 1 seconds
        sleep(Duration::from_millis(1000)).await;
    }
}

async fn fetch_quote(
    price_tx: &watch::Sender<Option<PriceData>>, 
    provider: &Arc<impl Provider>, 
    cfg: &settings::Settings
) -> Result<()> {
    let base_fee = provider.get_gas_price().await?;
    // Use constant 1.0 ETH trade size for consistency with arbitrage engine
    let volume = ONE_ETHER;
    
    // Get WETH -> USDT quote
    let calldata = quote_calldata(
        cfg.weth_addr.parse()?, 
        cfg.usdt_addr.parse()?, 
        volume, 
        3000
    );
    let quote_tx = build_tx(
        cfg.quoter_v2_addr.parse()?, 
        cfg.self_addr.parse()?, 
        calldata, 
        base_fee
    );

    let start = Instant::now();
    let response = provider.call(quote_tx).await?;
    let usdt_out = decode_quote_response(response)? as f64 / 1e6; // USDT has 6 decimals

    let price_data = PriceData {
        bid: usdt_out * 1.000,
        ask: usdt_out * 1.000,
    };

    if let Err(e) = price_tx.send(Some(price_data.clone())) {
        error!("failed to send DEX price update: {}", e);
    }

    info!("WHYPE/USDT: {:.2} / {:.2} (took {:.2}ms)", price_data.bid, price_data.ask, start.elapsed().as_millis());

    Ok(())
}

// revm


async fn fetch_quote_revm<P: Provider + Clone>(
    price_tx: &watch::Sender<Option<PriceData>>, 
    cfg: &settings::Settings,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
    provider: Arc<P>
) -> Result<()> {
    // Use constant 1.0 ETH trade size for consistency with arbitrage engine
    let volume = ONE_ETHER;

    // Get WETH -> USDT quote
    let calldata = quote_calldata(
        cfg.weth_addr.parse()?, 
        cfg.usdt_addr.parse()?, 
        volume, 
        3000
    );

    hydrate_pool_state(cache_db, &provider, cfg.pool_addr.parse()?).await?;

    let start = Instant::now();
    let response = revm_call(cfg.self_addr.parse()?, cfg.quoter_v2_addr.parse()?, calldata, cache_db)?;
    let usdt_out = decode_quote_response(response)? as f64 / 1e6; // USDT has 6 decimals

    let price_data = PriceData {
        bid: usdt_out,
        ask: usdt_out,
    };

    if let Err(e) = price_tx.send(Some(price_data.clone())) {
        error!("failed to send DEX price update: {}", e);
    }

    info!("WHYPE/USDT: {:.2} / {:.2} (took {:.2}ms REVM)", price_data.bid, price_data.ask, start.elapsed().as_millis());

    Ok(())
}

async fn fetch_quote_revm_custom<P: Provider + Clone>(
    price_tx: &watch::Sender<Option<PriceData>>, 
    cfg: &settings::Settings,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
    provider: Arc<P>
) -> Result<()> {
    // Use constant 1.0 ETH trade size for consistency with arbitrage engine
    let volume = ONE_ETHER;
    
    hydrate_pool_state(cache_db, &provider, cfg.pool_addr.parse()?).await?;

    let calldata = get_amount_out_calldata(cfg.pool_addr.parse()?, cfg.weth_addr.parse()?, cfg.usdt_addr.parse()?, volume);

    let start = Instant::now();
    let response = revm_revert(cfg.self_addr.parse()?, cfg.quoter_custom_addr.parse()?, calldata, cache_db)?;
    let usdt_out = decode_get_amount_out_response(response)? as f64 / 1e6;

    let price_data = PriceData {
        bid: usdt_out,
        ask: usdt_out,
    };

    if let Err(e) = price_tx.send(Some(price_data.clone())) {
        error!("failed to send DEX price update: {}", e);
    }

    info!("WHYPE/USDT: {:.2} / {:.2} (took {:.2}ms REVM)", price_data.bid, price_data.ask, start.elapsed().as_millis());

    Ok(())
}
