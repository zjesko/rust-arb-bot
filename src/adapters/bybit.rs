use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use log::{error, info};
use serde_json::{json, Value};
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use crate::settings;

pub async fn run_bybit_listener() {
    loop {
        match connect_and_subscribe().await {
            Ok(_) => info!("bybit ws connection closed normally"),
            Err(e) => error!("bybit ws connection error: {}", e),
        }
        
        info!("reconnecting in 5 seconds...");
        sleep(Duration::from_secs(5)).await;
    }
}

async fn connect_and_subscribe() -> Result<()> {
    let cfg = settings::Settings::load()?;

    let (ws_stream, _) = connect_async(&cfg.bybit_ws_endpoint).await?;
    info!("connected to bybit webSocket: {}", cfg.bybit_ws_endpoint);

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let subscribe_msg = json!({
        "op": "subscribe",
        "args": [format!("orderbook.1.{}", cfg.bybit_ticker)]
    });

    ws_sender.send(Message::Text(subscribe_msg.to_string())).await?;
    info!("subscribed to {} orderbook", cfg.bybit_ticker);

    while let Some(msg) = ws_receiver.next().await {
        match msg? {
            Message::Text(text) => {
                if let Ok(data) = serde_json::from_str::<Value>(&text) {
                    handle_orderbook_message(data).await?;
                }
            }
            Message::Ping(ping) => ws_sender.send(Message::Pong(ping)).await?,
            Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }

    Ok(())
}

async fn handle_orderbook_message(data: Value) -> Result<()> {
    // skip subscription confirmations
    if data.get("op").is_some() {
        return Ok(());
    }

    let Some(orderbook_data) = data.get("data") else {
        return Ok(());
    };

    let bid = orderbook_data.get("b")
        .and_then(|b| b.as_array())
        .and_then(|bids| bids.first())
        .and_then(|bid| bid.as_array())
        .and_then(|bid| bid.first())
        .and_then(|p| p.as_str())
        .unwrap_or("N/A");

    let ask = orderbook_data.get("a")
        .and_then(|a| a.as_array())
        .and_then(|asks| asks.first())
        .and_then(|ask| ask.as_array())
        .and_then(|ask| ask.first())
        .and_then(|p| p.as_str())
        .unwrap_or("N/A");

    info!("HYPEUSDT - Bid: {} | Ask: {}", bid, ask);

    Ok(())
}