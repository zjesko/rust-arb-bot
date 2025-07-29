# Rust Arbitrage Bot

https://github.com/zjesko/rust-arb-bot

A high-performance arbitrage bot written in Rust that monitors price differences between centralized exchanges (Bybit, Gate.io) and HyperSwap (DEX) on Hyperliquid network, looking for profitable arbitrage opportunities.

#### Features
- Optimised REVM simulations for lightning fast quote calls.
- Async/Multi-threaded, uses Tokio for concurrent price monitoring
- Realtime WebSocket for CEX prices, with heartbeat and reconnections   
- Uses Uniswap V3 sqrtPriceLimitX96` parameters to enforce maximum slippage
- Cached network database state (AlloyDB) with selective updates
- Multi-stage docker build leveraging cached dependencies
- Github Actions CI/CD to run cargo check and build docker image

### Supported Exchanges

#### Centralized Exchanges (CEX)
- **Bybit**: Real-time WebSocket price feeds for HYPEUSDT
- **Gate.io**: Real-time WebSocket price feeds for HYPE_USDT

#### Decentralized Exchange (DEX)
- **HyperSwap**: Uniswap V3-style AMM on Hyperliquid network

## Project Structure
```
rust-arb-bot/
├── Cargo.toml                    # Project dependencies and metadata
├── Cargo.lock                    # Dependency lock file
├── docker-compose.yml            # Docker deployment configuration
├── Dockerfile                    # Container build instructions
├── env.example                   # Environment variables template
├── .github/                      # GitHub Actions CI/CD
│   └── workflows/
│       └── ci.yml                # CI pipeline configuration
├── config/
│   └── default.toml              # Main configuration file
├── custom-quoter-contracts/      # Solidity contracts for DEX quotes
└── src/                          # Main Rust source code
    ├── main.rs                   # Application entry point
    ├── lib.rs                    # Library root
    ├── settings.rs               # Configuration management
    ├── arbitrage.rs              # Core arbitrage logic
    ├── adapters/                 # Exchange integrations
    │   ├── mod.rs
    │   ├── bybit.rs              # Bybit WebSocket client
    │   ├── gateio.rs             # Gate.io WebSocket client
    │   └── hyperswap.rs          # HyperSwap DEX integration
    ├── helpers/                  # Utility modules
    │   ├── mod.rs
    │   ├── abi.rs                # ABI encoding/decoding
    │   └── revm.rs               # REVM optimization helpers
    ├── benches/                  # Performance benchmarks
    │   └── dex_quotes.rs         # DEX quote benchmarking
    └── bytecode/                 # Precompiled contract bytecode
        └── generic_erc20.hex     # Generic ERC20 bytecode
```

## Setup Instructions

1. **Clone the repository:**
   ```bash
   git clone git@github.com/zjesko/rust-arb-bot
   cd rust-arb-bot
   ```

### Docker Deployment
```bash
docker-compose up --build
docker-compose up -d
```

### Or Native Rust

2. **Install dependencies:**
   ```bash
   cargo build --release
   ```

3. **Set required environment variables:**
   ```bash
   export RPC_URL="https://rpc.hyperliquid.xyz/evm"
   export RUST_LOG="info"
   ```

## CLI Commands

### Main Application
```bash
cargo run --bin rust-arb-bot
```

### Benchmarking
```bash
cargo run --bin dex-quotes-bench
```

### No API Keys Required

This bot operates using:
- **Bybit**: Public WebSocket feeds (no authentication needed)
- **Gate.io**: Public WebSocket feeds (no authentication needed)
- **HyperSwap**: Public on-chain data via RPC

## Configuration (`config/default.toml`)

```toml
# Network endpoints
rpc_url = "https://rpc.hyperliquid.xyz/evm"                    # Hyperliquid RPC
bybit_ws_endpoint = "wss://stream.bybit.com/v5/public/spot"   # Bybit WebSocket
gateio_ws_endpoint = "wss://api.gateio.ws/ws/v4/"             # Gate.io WebSocket

# Contract addresses
self_addr = "0x1234567890123456789012345678901234567890"       # Your wallet address
weth_addr = "0x5555555555555555555555555555555555555555"       # Wrapped ETH token
usdt_addr = "0xb8ce59fc3717ada4c02eadf9682a9e934f625ebb"       # USDT token  
pool_addr = "0x56abfaf40f5b7464e9cc8cff1af13863d6914508"       # HyperSwap pool
quoter_v2_addr = "0x03A918028f22D9E1473B7959C927AD7425A45C7C"   # Uniswap V3 quoter

# Trading pairs
bybit_ticker = "HYPEUSDT"   # Bybit trading pair
gateio_ticker = "HYPE_USDT" # Gate.io trading pair

# Fee settings (in basis points)
cex_fee_bps = 10            # 0.1% CEX trading fee
dex_fee_tier = 3000          # 0.3% fee tier

# Gas estimation
dex_gas_used = 130000       # Estimated gas for arbitrage transaction (https://hyperevmscan.io/tx/0x3d7af811cd8fdbe6d756946eccca2f3f1d6c1540321af46181f3a87e46429002)
```

## Optimizations (HyperSwap Quoting)

**Method 1: Direct RPC Call**
Using direct RPC call and quote2 contract to get quote.

**Method 2: REVM Execution + Cache Engine**
- Replaces expensive `eth_call` RPC requests with local EVM simulation
- Uses cached blockchain state for rapid execution  
- Updates the slot0 storage mapping to get new quote every time

```rust
// Update pool state with current tick and sqrt price
let slot0_storage_key = keccak256_hash(&[&encode(&U256::ZERO)]);
let new_slot0_value = encode_slot0(tick, sqrt_price_x96, 0, 1, 1, 0, false);
cache_db.insert_account_storage(pool_addr, slot0_storage_key.into(), new_slot0_value.into())?;
```

**Method 3: ERC20 Contract Mocking**
```rust
// Mock ERC20s with generic bytecode to avoid external calls
let mocked_erc20 = include_str!("../bytecode/generic_erc20.hex");
let mocked_erc20 = mocked_erc20.parse::<Bytes>().unwrap();
let mocked_erc20 = Bytecode::new_raw(mocked_erc20);
init_account_with_bytecode(cfg.weth_addr, mocked_erc20.clone(), &mut cache_db).await?;
init_account_with_bytecode(cfg.usdt_addr, mocked_erc20.clone(), &mut cache_db).await?;

// Mock max balances for the pool
let big = U256::MAX / U256::from(2);
insert_mapping_storage_slot(cfg.weth_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db).await?;
insert_mapping_storage_slot(cfg.usdt_addr, U256::ZERO, cfg.pool_addr, big, &mut cache_db).await?;
```
- Eliminates external contract calls during simulation and makes quotes faster from using generic erc20.

Method 4: Custom Quoter Contract (extension discussed later)

### Performance Comparison
```bash
cargo run --bin dex-quotes-bench
```

```
[2025-07-29T03:38:05Z INFO] 1. Standard fetch_quote (eth_call):
[2025-07-29T03:38:05Z INFO] First call: 499.050125ms
[2025-07-29T03:38:10Z INFO] 10 calls avg: 519.917604ms
[2025-07-29T03:38:10Z INFO] 2. REVM without mocking (revm_call):
[2025-07-29T03:38:14Z INFO] First call: 4.072761167s
[2025-07-29T03:38:16Z INFO] 10 calls avg: 214.5729ms
[2025-07-29T03:38:16Z INFO] 3. REVM with mocking (revm_call):
[2025-07-29T03:38:20Z INFO] First call: 3.932436417s
[2025-07-29T03:38:22Z INFO] 10 calls avg: 181.886529ms
```
## Fee and Accounting

### 1. **DEX Fees**  
The `fetch_quote_revm` function performs two types of quotes using Uniswap V3's quoter contract to accommodate for both fees and slippage:

**quoteExactInputSingle**: Used for "selling" 1 HYPE.

```rust
let sell_quote_params = QuoteExactInputSingleParams {
    tokenIn: cfg.weth_addr,
    tokenOut: cfg.usdt_addr,
    fee: cfg.dex_fee_tier,
    amountIn: volume,
};
let bid_price = decode_quote_response(sell_response)? as f64 / 1e6;
```

**quoteExactOutputSingle**: Used for "buying" 1 HYPE
```rust
let buy_quote_params = QuoteExactOutputSingleParams {
    tokenIn: cfg.usdt_addr,
    tokenOut: cfg.weth_addr,
    fee: cfg.dex_fee_tier,
    amountOut: volume,
};
let ask_price = decode_quote_output_response(ask_response)? as f64 / 1e6;
```

These quotes automatically factor in:
- **Pool fees** (0.3% in this case) - deducted from swap amounts
- **Price impact/slippage** - calculated based on current pool liquidity and reserves
- **Tick spacing** - ensures prices align with valid Uniswap V3 tick boundaries

The quoter simulates the actual swap without executing it, providing accurate pricing that matches real trade execution.


### 2. **CEX Fees**  
- CEX trading fees calculated separately: `cex_fee_bps / 10000 * trade_amount`
- Applied during arbitrage profitability calculation

### 3. **Gas Costs**

Gas costs are calculated using live network conditions

On average, transactions on HyperSwap consume around 140k gas. For example: https://hyperevmscan.io/tx/0x3d7af811cd8fdbe6d756946eccca2f3f1d6c1540321af46181f3a87e46429002

We use that and multiply it with the current gas price from the provider to get real-time gas estimates based on network congestion.

```rust
let gas_cost_wei = gas_price_wei * self.config.dex_gas_used as u128;
let gas_cost_hype = gas_cost_wei as f64 / 1e18;
let gas_cost_usd = gas_cost_hype * hype_price;
```

This ensures arbitrage calculations reflect current network congestion and transaction costs.

### 4. **Net Profit Calculation**
```rust
let gross_profit = sell_price - buy_price;
let cex_fee_usd = (self.config.cex_fee_bps as f64 / 10000.0) * cex_price;
let net_profit = gross_profit - cex_fee_usd - gas_cost_usd;
```
The engine looks for two types of opportunities:

1. **Buy CEX → Sell DEX**: 
   - Buy HYPE cheaper on centralized exchange
   - Sell HYPE higher on HyperSwap DEX

2. **Buy DEX → Sell CEX**:
   - Buy HYPE cheaper on HyperSwap DEX
   - Sell HYPE higher on centralized exchange

It logs all opportunities it finds:

**Profitable:**
```
🟢 ARB: buy $44.9143, sell $44.9600, net $0.0040, cex fee: $0.0449, gas: $0.0048
```

**Unprofitable:**
```
🔴 NO ARB: buy $44.9143, sell $44.9300, net $-0.1540, cex fee: $0.0449, gas: $0.1248
```
---

## Extensions

### REVM Optimizations - Implemented
The bot leverages several REVM optimizations to achieve superior performance:

- **State Caching**: Maintains a cached copy of blockchain state to avoid repeated RPC calls
- **Mock Contracts**: Uses generic ERC20 bytecode to eliminate external dependencies during simulation like balances and approvals
- **Storage Slot Updates**: Direct manipulation of Uniswap V3 pool storage for accurate price simulations

These optimizations reduce quote latency by ~60% compared to standard RPC calls while maintaining accuracy.

### Custom Quoter Contract - Partially Implemented
The project includes a custom Solidity quoter (`CustomQuoter.sol`) optimized for gas-efficient quote simulation on Uniswap V3-style pools.

It tries to execute a swap on target UniswapV3Pool and implements the required uniswapV3SwapCallback callback method. The pool triggers this method with amount0Delta and amount1Delta, precisely the info we need, so we do a revert with this data encoded. Solidity bubbles the revert up the caller where we can retrieve it in Rust.

```solidity
function uniswapV3SwapCallback(
    int256 amount0Delta,
    int256 amount1Delta,
    bytes calldata _data
) external {
    revert(string(abi.encode(amount0Delta, amount1Delta)));
}
```

This quoter can be integrated with the REVM mock environment for even faster quote generation and supports batch operations for multiple pairs simultaneously.

```rust
init_account_with_bytecode(cfg.custom_quoter, custom_quoter.clone(), &mut cache_db).await?;
```

### Multi-Exchange Support (Gate.io) - Implemented
The bot supports concurrent monitoring of multiple CEX feeds:
- **Bybit Integration**: Real-time HYPEUSDT price feed via WebSocket
- **Gate.io Integration**: Real-time HYPE_USDT price feed via WebSocket  

The adapter pattern allows easy addition of new exchanges.

### Multi-Volume Analysis Support
Support for arbitrage analysis across different volume amounts helps optimize trade sizing by testing multiple trade sizes (1, 5, 10, 50, 100 HYPE) simultaneously to analyze how volume affects DEX slippage and profitability. We can find out maximum profitable trade size while considering gas costs and preventing oversized trades that could move market prices significantly.

### Fast Execution with Flash Loans
Execute arbitrage without holding any or very minimal initial capital if a flash loan provider is available on HyperEVM. Complete arbitrage in single transaction or revert entirely, ensuring atomic execution.

Transactions can be sent through bundles (something like flashbots) to avoid frontrunning by MEV searchers.

### Funding Arbitrage
Execute spot as well as funding rate arbitrage within the same Hyperliquid ecosystem. For example, if funding rates are positive (longs pay shorts), you can:
1. Go long on the spot market (buy HYPE on DEX)
2. Go short on perpetuals (short HYPE-USD perp)
3. Collect funding payments while maintaining market-neutral position
4. Unwind positions when funding rates normalize

This strategy capitalizes on funding rate inefficiencies while hedging directional price risk.
