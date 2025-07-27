use std::sync::Arc;
use std::time::Duration;

use alloy::{
    network::{TransactionBuilder, Ethereum},
    primitives::{Address, Bytes, U160, U256, aliases::U24},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    sol,
    sol_types::{SolCall, SolValue},
    uint
};

use revm::{
    context::result::{ExecutionResult, Output},
    database::{AlloyDB, CacheDB, WrapDatabaseAsync},
    primitives::{keccak256, TxKind},
    Context, ExecuteEvm, MainBuilder, MainContext,
    state::{AccountInfo, Bytecode},
};

use anyhow::{Result, anyhow};
use log::{error, info};
use tokio::time::{sleep, Instant};
use tokio::sync::watch;

use crate::settings;
use crate::arbitrage::{PriceData};

pub static ONE_ETHER: U256 = uint!(1_000_000_000_000_000_000_U256);

sol! {
    struct QuoteExactInputSingleParams {
        address tokenIn;
        address tokenOut;
        uint256 amountIn;
        uint24 fee;
        uint160 sqrtPriceLimitX96;
    }

    function quoteExactInputSingle(QuoteExactInputSingleParams memory params)
    public
    override
    returns (
        uint256 amountOut,
        uint160 sqrtPriceX96After,
        uint32 initializedTicksCrossed,
        uint256 gasEstimate
    );

}

pub fn quote_calldata(token_in: Address, token_out: Address, amount_in: U256, fee: u32) -> Bytes {
    let zero_for_one = token_in < token_out;

    let sqrt_price_limit_x96: U160 = if zero_for_one {
        "4295128749".parse().unwrap()
    } else {
        "1461446703485210103287273052203988822378723970341"
            .parse()
            .unwrap()
    };

    let params = QuoteExactInputSingleParams {
        tokenIn: token_in,
        tokenOut: token_out,
        amountIn: amount_in,
        fee: U24::from(fee),
        sqrtPriceLimitX96: sqrt_price_limit_x96,
    };

    Bytes::from(quoteExactInputSingleCall { params }.abi_encode())
}


pub fn decode_quote_response(response: Bytes) -> Result<u128> {
    let (amount_out, _, _, _) = <(u128, u128, u32, u128)>::abi_decode(&response)?;
    Ok(amount_out)
}

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
pub fn revm_call<P: Provider + Clone>(
    from: Address,
    to: Address,
    calldata: Bytes,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
) -> Result<Bytes> {
    let mut evm = Context::mainnet()
        .with_db(cache_db)
        .modify_tx_chained(|tx| {
            tx.caller = from;
            tx.kind = TxKind::Call(to);
            tx.data = calldata;
            tx.value = U256::ZERO;
        })
        .build_mainnet();

    let ref_tx = evm.replay().unwrap();
    let result = ref_tx.result;

    let value = match result {
        ExecutionResult::Success {
            output: Output::Call(value),
            ..
        } => value,
        result => {
            return Err(anyhow!("execution failed: {result:?}"));
        }
    };

    Ok(value)
}

pub fn init_cache_db<P: Provider + Clone>(provider: Arc<P>) -> CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>> {
    CacheDB::new(WrapDatabaseAsync::new(AlloyDB::new((*provider).clone(), Default::default())).unwrap())
}

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
        bid: usdt_out * 1.000,
        ask: usdt_out * 1.000,
    };

    if let Err(e) = price_tx.send(Some(price_data.clone())) {
        error!("failed to send DEX price update: {}", e);
    }

    info!("WHYPE/USDT: {:.2} / {:.2} (took {:.2}ms REVM)", price_data.bid, price_data.ask, start.elapsed().as_millis());

    Ok(())
}

pub async fn init_account_with_bytecode<P: Provider + Clone>(
    address: Address,
    bytecode: Bytecode,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>
) -> Result<()> {
    let code_hash = bytecode.hash_slow();
    let acc_info = AccountInfo {
        balance: U256::ZERO,
        nonce: 0_u64,
        code: Some(bytecode),
        code_hash,
    };

    cache_db.insert_account_info(address, acc_info);
    Ok(())
}

pub async fn insert_mapping_storage_slot<P: Provider + Clone>(
    contract: Address,
    slot: U256,
    slot_address: Address,
    value: U256,
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>
) -> Result<()> {
    let hashed_balance_slot = keccak256((slot_address, slot).abi_encode());

    cache_db.insert_account_storage(contract, hashed_balance_slot.into(), value)?;
    Ok(())
}

async fn hydrate_pool_state<P: Provider + Clone>(
    cache_db: &mut CacheDB<WrapDatabaseAsync<AlloyDB<Ethereum, P>>>,
    provider: &Arc<P>,
    pool: Address
) -> Result<()> {
    // slot0 (position 0)
    let slot0 = provider.get_storage_at(pool, U256::ZERO).await?;
    cache_db.insert_account_storage(pool, U256::from(0), slot0)?;

    // liquidity (slot 2)
    let liq = provider.get_storage_at(pool, U256::from(2)).await?;
    cache_db.insert_account_storage(pool, U256::from(2), liq)?;

    Ok(())
}