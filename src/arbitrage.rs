use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use crate::settings::Settings;
use anyhow::Result;
use log::{info, warn, debug};

#[derive(Debug, Clone)]
pub struct PriceData {
    pub bid: f64,
    pub ask: f64,
    pub timestamp: u64,
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
    pub gross_profit_bps: f64,
    pub net_profit_bps: f64,
    pub estimated_profit_usd: f64,
    pub timestamp: u64,
}

pub struct ArbEngine {
    pub config: Settings,
    pub cex_rx: watch::Receiver<Option<PriceData>>,
    pub dex_rx: watch::Receiver<Option<PriceData>>,
}

impl ArbEngine {
    pub fn new(
        config: Settings,
        cex_rx: watch::Receiver<Option<PriceData>>,
        dex_rx: watch::Receiver<Option<PriceData>>
    ) -> Self {
        Self {
            config,
            cex_rx,
            dex_rx
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

        if let Some(opportunity) = self.calculate_arbitrage(cex_price.ask, dex_price.bid, ArbDirection::BuyCex)? {
            self.alert_opportunity(opportunity).await;
        }

        if let Some(opportunity) = self.calculate_arbitrage(dex_price.ask, cex_price.bid, ArbDirection::BuyDex)? {
            self.alert_opportunity(opportunity).await;
        }

        Ok(())
    }

    fn calculate_arbitrage(&self, buy_price: f64, sell_price: f64, direction: ArbDirection) -> Result<Option<ArbOpportunity>> {
        let gross_profit_bps = ((sell_price - buy_price) / buy_price) * 10000.0;
        
        // only proceed if positive gross profit
        if gross_profit_bps <= 0.0 {
            return Ok(None);
        }

        let total_fees_bps = (self.config.bybit_fee_bps + self.config.hyperswap_fee_bps) as f64;
        let net_profit_after_fees = gross_profit_bps - total_fees_bps;

        if net_profit_after_fees <= 0.0 {
            return Ok(None);
        }

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
            gross_profit_bps: 0.0,
            net_profit_bps: net_profit_after_fees,
            estimated_profit_usd: 0.0,
            timestamp: current_timestamp()
        }))
        
    }

    async fn alert_opportunity(&self, opportunity: ArbOpportunity) {
        let direction_text = match opportunity.direction {
            ArbDirection::BuyCex => "CEX→DEX",
            ArbDirection::BuyDex => "DEX→CEX",
        };
        
        warn!(
            "\n🚨 ARBITRAGE OPPORTUNITY FOUND!\n\
            Direction: {}\n\
            CEX Price: ${:.4}\n\
            DEX Price: ${:.4}\n\
            Net Profit: {:.2} bps (${:.2})",
            direction_text,
            opportunity.cex_price,
            opportunity.dex_price,
            opportunity.net_profit_bps,
            opportunity.estimated_profit_usd
        );
    }

}



pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
} 