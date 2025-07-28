use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use serde_json::{Value, json};
use tokio::sync::watch::Sender;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::arbitrage::PriceData;
use crate::settings;

pub async fn run_gateio_listener(tx: Sender<Option<PriceData>>) {
    loop {
        match connect_and_subscribe(tx.clone()).await {
            Ok(_) => info!("gateio ws connection closed normally"),
            Err(e) => error!("gateio ws connection error: {}", e),
        }

        info!("reconnecting in 5 seconds...");
        sleep(Duration::from_secs(5)).await;
    }
}

async fn connect_and_subscribe(tx: Sender<Option<PriceData>>) -> Result<()> {
    let cfg = settings::Settings::load()?;

    let (ws_stream, _) = connect_async(&cfg.gateio_ws_endpoint).await?;
    info!("connected to gateio webSocket: {}", cfg.gateio_ws_endpoint);

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to ticker updates using Gate.io WebSocket v4 format
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let subscribe_msg = json!({
        "time": current_time,
        "channel": "spot.tickers",
        "event": "subscribe",
        "payload": [cfg.gateio_ticker.clone()]
    });

    write.send(Message::Text(subscribe_msg.to_string())).await?;
    info!("subscribed to {} ticker", cfg.gateio_ticker);

    // Track previous price to avoid duplicate updates
    let mut last_price: Option<PriceData> = None;

    while let Some(msg) = read.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    // Skip subscription confirmations but allow update events
                    if let Some(event) = data.get("event").and_then(|e| e.as_str()) {
                        if event == "subscribe" || event == "unsubscribe" {
                            continue;
                        }
                    }

                    // Handle ping messages
                    if let Some(channel) = data.get("channel").and_then(|c| c.as_str()) {
                        if channel == "spot.ping" {
                            continue;
                        }
                    }

                    // Parse ticker data from update events
                    let Some(result) = data.get("result") else {
                        continue;
                    };

                    // Extract bid and ask from ticker data
                    let bid = result
                        .get("highest_bid")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);

                    let ask = result
                        .get("lowest_ask")
                        .and_then(|p| p.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0);

                    let price_data = PriceData { bid, ask };

                    // Only send update if price has changed
                    if last_price.as_ref() != Some(&price_data) {
                        if let Err(e) = tx.send(Some(price_data.clone())) {
                            error!("failed to send CEX price update: {}", e);
                        }

                        info!("⚠️ GATEIO {}: bid ${:.2} ask ${:.2}", cfg.gateio_ticker, bid, ask);
                        last_price = Some(price_data);
                    }
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