use std::sync::Arc;
use std::time::Duration;

use alloy::{
    network::TransactionBuilder,
    primitives::{Address, Bytes, U160, U256, aliases::U24},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    sol,
    sol_types::{SolCall, SolValue},
};
use anyhow::Result;
use log::{error, info};
use tokio::time::{sleep, Instant};
use tokio::sync::watch;

use crate::helpers::{ONE_ETHER};
use crate::settings;
use crate::arbitrage::{PriceData};


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

    loop {
        match fetch_quote(&tx, &provider, &cfg).await {
            Ok(_) => {},
            Err(e) => error!("DEX price fetch error: {}", e),
        }
        
        // Fetch DEX prices every 1 seconds
        sleep(Duration::from_millis(200)).await;
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
        error!("Failed to send DEX price update: {}", e);
    }

    info!("WHYPE/USDT: {:.2} / {:.2} (took {:.2}ms)", price_data.bid, price_data.ask, start.elapsed().as_millis());

    Ok(())
}