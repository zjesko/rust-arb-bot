use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use serde_json::{json, Value};
use tokio::time::sleep;
use tokio::sync::watch::Sender;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::settings;
use crate::arbitrage::{PriceData};

pub async fn run_bybit_listener(tx: Sender<Option<PriceData>>) {
    loop {
        match connect_and_subscribe(tx.clone()).await {
            Ok(_) => info!("bybit ws connection closed normally"),
            Err(e) => error!("bybit ws connection error: {}", e),
        }
        
        info!("reconnecting in 5 seconds...");
        sleep(Duration::from_secs(5)).await;
    }
}

async fn connect_and_subscribe(tx: Sender<Option<PriceData>>) -> Result<()> {
    let cfg = settings::Settings::load()?;

    let (ws_stream, _) = connect_async(&cfg.bybit_ws_endpoint).await?;
    info!("connected to bybit webSocket: {}", cfg.bybit_ws_endpoint);

    let (mut write, mut read) = ws_stream.split();

    let subscribe_msg = json!({
        "op": "subscribe",
        "args": [format!("orderbook.1.{}", cfg.bybit_ticker)]
    });

    write.send(Message::Text(subscribe_msg.to_string())).await?;
    info!("subscribed to {} orderbook", cfg.bybit_ticker);

    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    // skip subscription confirmations
                    if data.get("op").is_some() {
                        continue;
                    }

                    let Some(orderbook_data) = data.get("data") else {
                        continue;
                    };

                    let bid = orderbook_data.get("b")
                        .and_then(|b| b.as_array())
                        .and_then(|bids| bids.first())
                        .and_then(|bid| bid.as_array())
                        .and_then(|bid| bid.first())
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);

                    let ask = orderbook_data.get("a")
                        .and_then(|a| a.as_array())
                        .and_then(|asks| asks.first())
                        .and_then(|ask| ask.as_array())
                        .and_then(|ask| ask.first())
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);

                    let price_data = PriceData {
                        bid,
                        ask
                    };

                    if let Err(e) = tx.send(Some(price_data.clone())) {
                        error!("failed to send CEX price update: {}", e);
                    }

                    info!("{}: {} / {} ", cfg.bybit_ticker, bid, ask); 
                }
            }
            Message::Ping(ping) => write.send(Message::Pong(ping)).await?,
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
