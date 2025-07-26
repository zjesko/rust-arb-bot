use tokio::sync::watch;
use crate::settings::Settings;
use anyhow::Result;
use log::{info, warn};
use alloy::providers::Provider;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PriceData {
    pub bid: f64,
    pub ask: f64,
}

#[derive(Debug, Clone)]
pub enum ArbDirection {
    BuyCex,
    BuyDex,
}
pub struct ArbOpportunity {
    pub direction: ArbDirection,
    pub cex_price: f64,
    pub dex_price: f64,
    pub net_profit_bps: f64,
    pub estimated_profit_usd: f64,
    pub gas_cost_usd: f64,
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
        provider: Arc<dyn Provider>
    ) -> Self {
        Self {
            config,
            cex_rx,
            dex_rx,
            provider
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
            _ => return Ok(())
        };

        let gas_price_wei = self.provider.get_gas_price().await?;

        if let Some(opportunity) = self.calculate_arbitrage(
            cex_price.ask, 
            dex_price.bid, 
            ArbDirection::BuyCex, 
            gas_price_wei
        )? {
            self.alert_opportunity(opportunity).await;
        }

        if let Some(opportunity) = self.calculate_arbitrage(
            dex_price.ask, 
            cex_price.bid, 
            ArbDirection::BuyDex, 
            gas_price_wei
        )? {
            self.alert_opportunity(opportunity).await;
        }

        Ok(())
    }

    fn calculate_arbitrage(
        &self, 
        buy_price: f64, 
        sell_price: f64, 
        direction: ArbDirection, 
        gas_price_wei: u128
    ) -> Result<Option<ArbOpportunity>> {
        let gross_profit_bps = ((sell_price - buy_price) / buy_price) * 10000.0;
        
        // only proceed if positive gross profit
        if gross_profit_bps <= 0.0 {
            return Ok(None);
        }

        // Calculate gas cost in HYPE tokens
        let gas_cost_wei = gas_price_wei * self.config.gas_used as u128;
        let gas_cost_hype = gas_cost_wei as f64 / 1e18;
        
        let hype_price = match direction {
            ArbDirection::BuyCex => sell_price, // DEX price for selling HYPE
            ArbDirection::BuyDex => buy_price,  // CEX price for buying HYPE
        };
        let gas_cost_usd = gas_cost_hype * hype_price;
        
        let gas_cost_bps = (gas_cost_usd / buy_price) * 10000.0;
        
        let total_fees_bps = (self.config.bybit_fee_bps + self.config.hyperswap_fee_bps) as f64;
        let net_profit_bps = gross_profit_bps - total_fees_bps - gas_cost_bps;

        if net_profit_bps <= 0.0 {
            info!("No arbitrage opportunity: net profit {:.2}bps (buy: ${:.4}, sell: ${:.4})", net_profit_bps, buy_price, sell_price);
            return Ok(None);
        }

        let estimated_profit_usd = (net_profit_bps / 10000.0) * buy_price;

        Ok(Some(ArbOpportunity {
            direction: direction.clone(),
            cex_price: match direction {
                ArbDirection::BuyCex => buy_price,
                ArbDirection::BuyDex => sell_price,
            },
            dex_price: match direction {
                ArbDirection::BuyCex => sell_price,
                ArbDirection::BuyDex => buy_price,
            },
            net_profit_bps,
            estimated_profit_usd,
            gas_cost_usd,
        }))
        
    }

    async fn alert_opportunity(&self, opportunity: ArbOpportunity) {
        let action_text = match opportunity.direction {
            ArbDirection::BuyCex => format!("buy CEX @ ${:.4}, SELL DEX @ ${:.4}", opportunity.cex_price, opportunity.dex_price),
            ArbDirection::BuyDex => format!("buy DEX @ ${:.4}, SELL CEX @ ${:.4}", opportunity.dex_price, opportunity.cex_price),
        };
        
        warn!(
            "🚨 ARBITRAGE: {}, Net: {:.2}bps (${:.4}), Gas: ${:.4}",
            action_text,
            opportunity.net_profit_bps,
            opportunity.estimated_profit_usd,
            opportunity.gas_cost_usd
        );
    }

}
