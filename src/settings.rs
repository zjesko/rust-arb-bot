use alloy::primitives::Address;
use anyhow::Result;
use config;
use dotenvy;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub self_addr: Address,
    pub weth_addr: Address,
    pub usdt_addr: Address,
    pub quoter_v2_addr: Address,
    pub pool_addr: Address,
    pub quoter_custom_addr: Address,

    pub bybit_ticker: String,
    pub bybit_fee_bps: u32,
    pub hyperswap_fee_bps: u32,
    pub gas_used: u64,

    // from env
    pub rpc_url: String,
    pub bybit_ws_endpoint: String,
}

impl Settings {
    /// Loads `config/{stage}.toml`,
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        let cfg = config::Config::builder()
            .add_source(config::File::with_name("config/default.toml"))
            .add_source(config::Environment::default())
            .build()?;

        Ok(cfg.try_deserialize()?)
    }
}
