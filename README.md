# Rust Arbitrage Bot

A high-performance arbitrage bot written in Rust that monitors price differences between Bybit (CEX) and HyperSwap (DEX) on Hyperliquid network, looking for profitable trading opportunities.

## Features

- Real-time price monitoring via WebSocket (Bybit) and on-chain quotes (HyperSwap)
- REVM-based simulation for ultra-fast DEX quote fetching
- ERC20 contract mocking to eliminate external calls during simulation
- Comprehensive fee and gas cost calculation
- Docker support for easy deployment

## Project Structure

```
rust-arb-bot/
├── Cargo.toml                    # Project dependencies and metadata
├── Cargo.lock                    # Dependency lock file
├── docker-compose.yml            # Docker deployment configuration
├── Dockerfile                    # Container build instructions
├── env.example                   # Environment variables template
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

### Prerequisites

- Rust (latest stable version)
- Docker (optional, for containerized deployment)
- Access to Hyperliquid RPC endpoint

### Installation

1. **Clone the repository:**
   ```bash
   git clone <repository-url>
   cd rust-arb-bot
   ```

2. **Install dependencies:**
   ```bash
   cargo build --release
   ```

3. **Configure environment:**
   ```bash
   cp env.example .env
   # Edit .env file with your settings
   ```

4. **Set required environment variables:**
   ```bash
   export RPC_URL="https://rpc.hyperliquid.xyz/evm"
   export RUST_LOG="info"
   ```

## CLI Commands

### Main Application
```bash
# Run the arbitrage bot
cargo run

# Run with debug logging
RUST_LOG=debug cargo run
```

### Benchmarking
```bash
# Run DEX quote performance benchmark
cargo run --bin dex-quotes-bench
```

### Docker Deployment
```bash
# Build and run with Docker Compose
docker-compose up --build

# Run in detached mode
docker-compose up -d
```

## Required API Keys and RPC Settings

### Environment Variables

- **RPC_URL**: Hyperliquid RPC endpoint
  - Example: `https://rpc.hyperliquid.xyz/evm`
  - Required for on-chain interactions

- **RUST_LOG**: Logging level
  - Options: `error`, `warn`, `info`, `debug`, `trace`
  - Recommended: `info`

### No API Keys Required

This bot operates using:
- **Bybit**: Public WebSocket feeds (no authentication needed)
- **HyperSwap**: Public on-chain data via RPC

## Configuration (`config/default.toml`)

```toml
# Network endpoints
rpc_url = "https://rpc.hyperliquid.xyz/evm"              # Hyperliquid RPC
bybit_ws_endpoint = "wss://stream.bybit.com/v5/public/spot"  # Bybit WebSocket

# Contract addresses
self_addr = "0x1234567890123456789012345678901234567890"   # Your wallet address
weth_addr = "0x5555555555555555555555555555555555555555"   # Wrapped ETH token
usdt_addr = "0xb8ce59fc3717ada4c02eadf9682a9e934f625ebb"   # USDT token  
pool_addr = "0x56abfaf40f5b7464e9cc8cff1af13863d6914508"   # HyperSwap pool
quoter_v2_addr = "0x03A918028f22D9E1473B7959C927AD7425A45C7C" # Uniswap V3 quoter
quoter_custom_addr = "0x03A918028f22D9E1473B7959C927AD7425A45C7D" # Custom quoter

# Trading pair
bybit_ticker = "HYPEUSDT"  # Bybit trading pair

# Fee settings (in basis points)
bybit_fee_bps = 100        # 1% Bybit trading fee
hyperswap_fee_bps = 3000   # 30% HyperSwap pool fee

# Gas estimation
gas_used = 200000          # Estimated gas for arbitrage transaction
```

## Optimizations

### REVM Integration

The bot implements two major REVM optimizations:

#### 1. **REVM Execution Engine**
- Replaces expensive `eth_call` RPC requests with local EVM simulation
- **Performance gain**: ~5-10x faster quote fetching
- Uses cached blockchain state for rapid execution

#### 2. **ERC20 Contract Mocking**
```rust
// Mock ERC20s with generic bytecode to avoid external calls
let mocked_erc20 = include_str!("../bytecode/generic_erc20.hex");
init_account_with_bytecode(weth_addr, mocked_erc20, &mut cache_db).await?;

// Mock unlimited balances for the pool
let big_balance = U256::MAX / U256::from(2);
insert_mapping_storage_slot(weth_addr, U256::ZERO, pool_addr, big_balance, &mut cache_db).await?;
```

**Benefits:**
- Eliminates external contract calls during simulation
- Provides unlimited token balances for accurate quote simulation
- Dramatically reduces quote latency

### Performance Comparison
| Method | First Call | 10 Calls Avg |
|--------|------------|--------------|
| Standard `eth_call` | ~200ms | ~150ms |
| REVM (no mocking) | ~100ms | ~80ms |
| REVM (with mocking) | ~50ms | ~20ms |

## Quote Function and Slippage/Fee Handling

### DEX Quote Mechanism

The `getQuote` function performs two types of quotes:

```rust
// Exact input: "How much USDT for 1 HYPE?"
let sell_calldata = quote_calldata(weth_addr, usdt_addr, volume, fee_bps);
let bid_price = decode_quote_response(response)? as f64 / 1e6;

// Exact output: "How much HYPE needed for 1 USDT worth?"  
let buy_calldata = quote_exact_output_calldata(usdt_addr, weth_addr, volume, fee_bps);
let ask_price = decode_quote_output_response(response)? as f64 / 1e6;
```

### Fee and Slippage Accounting

#### 1. **DEX Fees**
- Built into the quote via `hyperswap_fee_bps` parameter
- Uniswap V3-style concentrated liquidity fees
- Applied automatically during quote calculation

#### 2. **CEX Fees**  
- Bybit trading fees calculated separately: `bybit_fee_bps / 10000 * trade_amount`
- Applied during arbitrage profitability calculation

#### 3. **Gas Costs**
```rust
let gas_cost_wei = gas_price_wei * gas_used;
let gas_cost_hype = gas_cost_wei as f64 / 1e18;
let gas_cost_usd = gas_cost_hype * hype_price;
```

#### 4. **Net Profit Calculation**
```rust
let gross_profit = sell_price - buy_price;
let bybit_fee_usd = (bybit_fee_bps as f64 / 10000.0) * cex_price;
let net_profit = gross_profit - bybit_fee_usd - gas_cost_usd;
```

### Slippage Protection
- Uses Uniswap V3 `sqrtPriceLimitX96` parameters to enforce maximum slippage
- Quotes include realistic price impact based on pool liquidity
- REVM simulation ensures quotes reflect actual execution conditions

## General Notes

### Architecture
- **Async/Multi-threaded**: Uses Tokio for concurrent price monitoring
- **Real-time**: WebSocket for CEX prices, periodic polling for DEX prices  
- **Fault-tolerant**: Automatic reconnection and error handling
- **Memory-efficient**: Cached database state with selective updates

### Supported Pairs
- Currently configured for HYPE/USDT arbitrage
- Easily configurable for other token pairs by updating contract addresses

### Risk Management
- Calculates all fees and costs before flagging opportunities
- No automatic execution - logs opportunities for manual review
- Conservative gas estimates to avoid failed transactions

### Development
- Comprehensive benchmarking suite included
- Docker support for consistent deployment environments
- Extensive logging for monitoring and debugging

### Custom Quoter Contract
The project includes a custom Solidity quoter (`CustomQuoter.sol`) optimized for gas-efficient quote simulation on Uniswap V3-style pools.

---

**Note**: This bot is for educational and research purposes. Always verify opportunities manually before executing trades and ensure compliance with relevant regulations. 