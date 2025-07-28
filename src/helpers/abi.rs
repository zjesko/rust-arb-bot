use alloy::{
    network::TransactionBuilder,
    primitives::{Address, Bytes, U160, U256, aliases::U24},
    rpc::types::TransactionRequest,
    sol,
    sol_types::{SolCall, SolValue},
    uint,
};

use anyhow::Result;

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

sol! {
    struct QuoteExactOutputSingleParams {
        address tokenIn;
        address tokenOut;
        uint256 amountOut;
        uint24 fee;
        uint160 sqrtPriceLimitX96;
    }

    function quoteExactOutputSingle(QuoteExactOutputSingleParams memory params)
    public
    override
    returns (
        uint256 amountIn,
        uint160 sqrtPriceX96After,
        uint32 initializedTicksCrossed,
        uint256 gasEstimate
    );
}

pub fn decode_quote_response(response: Bytes) -> Result<u128> {
    let (amount_out, _, _, _) = <(u128, u128, u32, u128)>::abi_decode(&response)?;
    Ok(amount_out)
}

pub fn decode_quote_output_response(response: Bytes) -> Result<u128> {
    let (amount_in, _, _, _) = <(u128, u128, u32, u128)>::abi_decode(&response)?;
    Ok(amount_in)
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

pub fn quote_exact_output_calldata(
    token_in: Address,
    token_out: Address,
    amount_out: U256,
    fee: u32,
) -> Bytes {
    let zero_for_one = token_in < token_out;

    let sqrt_price_limit_x96: U160 = if zero_for_one {
        "4295128749".parse().unwrap()
    } else {
        "1461446703485210103287273052203988822378723970341"
            .parse()
            .unwrap()
    };

    let params = QuoteExactOutputSingleParams {
        tokenIn: token_in,
        tokenOut: token_out,
        amountOut: amount_out,
        fee: U24::from(fee),
        sqrtPriceLimitX96: sqrt_price_limit_x96,
    };

    Bytes::from(quoteExactOutputSingleCall { params }.abi_encode())
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
