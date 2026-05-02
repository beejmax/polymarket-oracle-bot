use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::time;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use crate::model::{Event, PriceTick};

pub async fn run_chainlink_rtds(
    url: String,
    symbols_by_asset: HashMap<String, String>,
    event_tx: mpsc::Sender<Event>,
) {
    let asset_by_symbol = symbols_by_asset
        .into_iter()
        .map(|(asset, symbol)| (symbol.to_lowercase(), asset.to_uppercase()))
        .collect::<HashMap<_, _>>();
    let mut backoff = Duration::from_secs(1);
    loop {
        match run_once(&url, &asset_by_symbol, &event_tx).await {
            Ok(()) => backoff = Duration::from_secs(1),
            Err(err) => {
                warn!(error = %err, "Chainlink RTDS disconnected");
                time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
}

async fn run_once(
    url: &str,
    asset_by_symbol: &HashMap<String, String>,
    event_tx: &mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    let (mut ws, _) = connect_async(url).await?;
    info!(url = %url, symbols = asset_by_symbol.len(), "Chainlink RTDS connected");
    ws.send(Message::Text(
        subscription_message(asset_by_symbol).to_string().into(),
    ))
    .await?;
    let mut ping = time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = ping.tick() => ws.send(Message::Text("PING".into())).await?,
            maybe_message = ws.next() => {
                let Some(message) = maybe_message else {
                    return Ok(());
                };
                let message = message?;
                if let Message::Text(text) = message {
                    handle_text(&text, asset_by_symbol, event_tx).await;
                }
            }
        }
    }
}

fn subscription_message(asset_by_symbol: &HashMap<String, String>) -> Value {
    let subscriptions = asset_by_symbol
        .keys()
        .map(|symbol| {
            json!({
                "topic": "crypto_prices_chainlink",
                "type": "update",
                "filters": json!({"symbol": symbol}).to_string()
            })
        })
        .collect::<Vec<_>>();
    json!({"action": "subscribe", "subscriptions": subscriptions})
}

async fn handle_text(
    text: &str,
    asset_by_symbol: &HashMap<String, String>,
    event_tx: &mpsc::Sender<Event>,
) {
    if text.is_empty() || text == "PONG" {
        return;
    }
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let Some(payload) = message.get("payload") else {
        return;
    };
    if let Some(data) = payload.get("data").and_then(Value::as_array) {
        let symbol = payload
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        for item in data {
            emit_tick(&symbol, item, asset_by_symbol, event_tx).await;
        }
    } else {
        let symbol = payload
            .get("symbol")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        emit_tick(&symbol, payload, asset_by_symbol, event_tx).await;
    }
}

async fn emit_tick(
    symbol: &str,
    payload: &Value,
    asset_by_symbol: &HashMap<String, String>,
    event_tx: &mpsc::Sender<Event>,
) {
    let Some(asset) = asset_by_symbol.get(symbol) else {
        return;
    };
    let Some(price) = payload.get("value").and_then(Value::as_f64) else {
        return;
    };
    let Some(feed_ts_ms) = payload.get("timestamp").and_then(Value::as_i64) else {
        return;
    };
    let tick = PriceTick {
        asset: asset.clone(),
        symbol: symbol.to_string(),
        price,
        feed_ts_ms,
        received_ts_ms: chrono::Utc::now().timestamp_millis(),
    };
    let _ = event_tx.send(Event::Tick(tick)).await;
}
