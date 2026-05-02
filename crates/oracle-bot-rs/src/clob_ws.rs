use std::collections::BTreeSet;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio::time;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use crate::model::Event;

pub async fn run_clob_market_ws(
    url: String,
    ping_payload: String,
    mut sub_rx: watch::Receiver<BTreeSet<String>>,
    event_tx: mpsc::Sender<Event>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        match run_once(&url, &ping_payload, &mut sub_rx, &event_tx).await {
            Ok(()) => backoff = Duration::from_secs(1),
            Err(err) => {
                warn!(error = %err, "CLOB market websocket disconnected");
                time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    }
}

async fn run_once(
    url: &str,
    ping_payload: &str,
    sub_rx: &mut watch::Receiver<BTreeSet<String>>,
    event_tx: &mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    let (mut ws, _) = connect_async(url).await?;
    info!(url = %url, "CLOB market websocket connected");
    let mut subscribed = BTreeSet::new();
    let initial = sub_rx.borrow().clone();
    if !initial.is_empty() {
        ws.send(Message::Text(
            initial_subscription(&initial).to_string().into(),
        ))
        .await?;
        subscribed = initial;
    }

    let mut ping = time::interval(Duration::from_secs(10));
    loop {
        tokio::select! {
            _ = ping.tick() => ws.send(Message::Text(ping_payload.to_string().into())).await?,
            changed = sub_rx.changed() => {
                changed?;
                let wanted = sub_rx.borrow().clone();
                let new_ids = wanted.difference(&subscribed).cloned().collect::<BTreeSet<_>>();
                if !new_ids.is_empty() {
                    ws.send(Message::Text(subscription_update(&new_ids).to_string().into())).await?;
                    subscribed.extend(new_ids);
                }
            }
            maybe_message = ws.next() => {
                let Some(message) = maybe_message else {
                    return Ok(());
                };
                let message = message?;
                if let Message::Text(text) = message {
                    handle_text(&text, event_tx).await;
                }
            }
        }
    }
}

fn initial_subscription(asset_ids: &BTreeSet<String>) -> Value {
    json!({
        "assets_ids": asset_ids.iter().cloned().collect::<Vec<_>>(),
        "type": "market",
        "custom_feature_enabled": true
    })
}

fn subscription_update(asset_ids: &BTreeSet<String>) -> Value {
    json!({
        "operation": "subscribe",
        "assets_ids": asset_ids.iter().cloned().collect::<Vec<_>>()
    })
}

async fn handle_text(text: &str, event_tx: &mpsc::Sender<Event>) {
    if text.is_empty() || text == "PONG" {
        return;
    }
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if let Some(items) = message.as_array() {
        for item in items {
            let _ = event_tx.send(Event::MarketMessage(item.clone())).await;
        }
    } else {
        let _ = event_tx.send(Event::MarketMessage(message)).await;
    }
}
