use crate::settings::Settings;
use alloy::providers::Provider;
use anyhow::Result;
use log::{info};
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq)]
pub struct PriceData {
    pub bid: f64,
    pub ask: f64,
}

#[derive(Debug, Clone)]
pub enum ArbDirection {
    BuyCex,
    BuyDex,
}
pub struct ArbEngine {
    pub config: Settings,
    pub cex_rx: watch::Receiver<Option<PriceData>>,
    pub dex_rx: watch::Receiver<Option<PriceData>>,
    pub provider: Arc<dyn Provider>,
}

impl ArbEngine {
    pub fn new(
        config: Settings,
        cex_rx: watch::Receiver<Option<PriceData>>,
        dex_rx: watch::Receiver<Option<PriceData>>,
        provider: Arc<dyn Provider>,
    ) -> Self {
        Self {
            config,
            cex_rx,
            dex_rx,
            provider,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("starting arbitrage engine...");

        loop {
            tokio::select! {
                _ = self.cex_rx.changed() => {
                    self.check_for_opportunity().await?;
                }
                _ = self.dex_rx.changed() => {
                    self.check_for_opportunity().await?;
                }
            }
        }
    }

    pub async fn check_for_opportunity(&mut self) -> Result<()> {
        let cex_data = self.cex_rx.borrow().clone();
        let dex_data = self.dex_rx.borrow().clone();

        let (cex_price, dex_price) = match (cex_data.as_ref(), dex_data.as_ref()) {
            (Some(cex), Some(dex)) => (cex, dex),
            _ => return Ok(()),
        };

        let gas_price_wei = self.provider.get_gas_price().await?;

        // if dex_price.bid > cex_price.ask {
            self.calculate_arbitrage(
                cex_price.ask,
                dex_price.bid,
                ArbDirection::BuyCex,
                gas_price_wei,
            );
        // }
        // if cex_price.bid > dex_price.ask {
            self.calculate_arbitrage(
                dex_price.ask,
                cex_price.bid,
                ArbDirection::BuyDex,
                gas_price_wei,
            );
        // }

        Ok(())
    }

    fn calculate_arbitrage(
        &self,
        buy_price: f64,
        sell_price: f64,
        direction: ArbDirection,
        gas_price_wei: u128,
    ) {
        let gross_profit = sell_price - buy_price;

        // Calculate gas cost in HYPE tokens
        let gas_cost_wei = gas_price_wei * self.config.dex_gas_used as u128;
        let gas_cost_hype = gas_cost_wei as f64 / 1e18;

        let hype_price = match direction {
            ArbDirection::BuyCex => sell_price,
            ArbDirection::BuyDex => buy_price,
        };
        let gas_cost_usd = gas_cost_hype * hype_price;

        let cex_price = match direction {
            ArbDirection::BuyCex => buy_price,
            ArbDirection::BuyDex => sell_price,
        };
        let cex_fee_usd = (self.config.cex_fee_bps as f64 / 10000.0) * cex_price;
        let net_profit = gross_profit - cex_fee_usd - gas_cost_usd;

        if net_profit <= 0.0 {
            info!(
                "🔴 NO ARB: buy ${:.4}, sell ${:.4}, net ${:.4}, cex fee: ${:.4}, gas: ${:.4}",
                buy_price, sell_price, net_profit, cex_fee_usd, gas_cost_usd
            );
        } else {
            info!(
                "🟢 ARB: buy ${:.4}, sell ${:.4}, net ${:.4}, cex fee: ${:.4}, gas: ${:.4}",
                buy_price, sell_price, net_profit, cex_fee_usd, gas_cost_usd
            );
        }
    }
}
