use std::sync::Arc;

use alloy::{
    network::TransactionBuilder,
    primitives::{Address, Bytes, U160, U256, aliases::U24},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
    sol,
    sol_types::{SolCall, SolValue},
};
use anyhow::Result;
use log::info;

use crate::helpers::{measure_end, measure_start, ONE_ETHER};
use crate::settings;


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

pub async fn get_quote() -> Result<()> {
    let cfg = settings::Settings::load()?;
    
    let provider = ProviderBuilder::new().connect_http(cfg.rpc_url.parse()?);
    let provider = Arc::new(provider);

    let base_fee = provider.get_gas_price().await?;
    let volume = ONE_ETHER;
    let calldata = quote_calldata(cfg.weth_addr.parse()?, cfg.usdt_addr.parse()?, volume, 3000);

    let tx = build_tx(cfg.quoter_v2_addr.parse()?, cfg.self_addr.parse()?, calldata, base_fee);
    let start = measure_start("eth_call_one");
    let call = provider.call(tx).await?;

    let amount_out = decode_quote_response(call)?;
    info!("HyperSwap {} WETH -> USDT {}", volume, amount_out);

    measure_end(start);
    
    Ok(())
}