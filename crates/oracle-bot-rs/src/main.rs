mod chainlink;
mod clob_ws;
mod config;
mod gamma;
mod model;
mod orderbook;
mod signal;
mod state;
mod timeframe;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time;
use tracing::{info, warn};

use crate::chainlink::run_chainlink_rtds;
use crate::clob_ws::run_clob_market_ws;
use crate::config::AppConfig;
use crate::gamma::GammaClient;
use crate::model::{Event, Outcome};
use crate::signal::SignalEngine;
use crate::state::RuntimeState;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "config.example.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oracle_bot_rs=info,info".into()),
        )
        .init();

    let args = Args::parse();
    let cfg = Arc::new(AppConfig::load(&args.config)?);
    cfg.validate()?;
    info!(mode = %cfg.trading.mode, "starting Rust oracle bot skeleton");

    let state = Arc::new(RwLock::new(RuntimeState::new(
        cfg.trading.opening_capture_grace_seconds,
    )));
    let (event_tx, event_rx) = mpsc::channel::<Event>(4096);
    let (sub_tx, sub_rx) = watch::channel::<BTreeSet<String>>(BTreeSet::new());

    let symbols_by_asset = cfg
        .enabled_assets()
        .iter()
        .map(|asset| {
            (
                asset.symbol.to_uppercase(),
                asset.chainlink_symbol.to_lowercase(),
            )
        })
        .collect::<HashMap<_, _>>();

    tokio::spawn(run_chainlink_rtds(
        cfg.polymarket.rtds_ws_url.clone(),
        symbols_by_asset,
        event_tx.clone(),
    ));
    tokio::spawn(run_clob_market_ws(
        cfg.polymarket.market_ws_url.clone(),
        cfg.polymarket.market_ws_ping_payload.clone(),
        sub_rx,
        event_tx.clone(),
    ));
    tokio::spawn(market_poll_loop(cfg.clone(), state.clone(), sub_tx));
    tokio::spawn(signal_loop(cfg.clone(), state.clone()));
    tokio::spawn(status_loop(state.clone()));

    event_loop(state, event_rx).await
}

async fn event_loop(
    state: Arc<RwLock<RuntimeState>>,
    mut events: mpsc::Receiver<Event>,
) -> Result<()> {
    loop {
        tokio::select! {
            maybe_event = events.recv() => {
                let Some(event) = maybe_event else {
                    warn!("event channel closed");
                    return Ok(());
                };
                match event {
                    Event::Tick(tick) => state.write().await.apply_tick(tick),
                    Event::MarketMessage(message) => {
                        state.write().await.orderbook.update_from_message(&message);
                    }
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal?;
                info!("shutdown requested");
                return Ok(());
            }
        }
    }
}

async fn market_poll_loop(
    cfg: Arc<AppConfig>,
    state: Arc<RwLock<RuntimeState>>,
    sub_tx: watch::Sender<BTreeSet<String>>,
) {
    let gamma = GammaClient::new(cfg.polymarket.gamma_base_url.clone());
    let mut interval = time::interval(Duration::from_secs_f64(
        cfg.trading.market_poll_interval_seconds.max(0.5),
    ));
    loop {
        interval.tick().await;
        let now_ts = chrono::Utc::now().timestamp();
        match gamma
            .discover_windows(
                &cfg.enabled_assets(),
                &cfg.trading.timeframe,
                now_ts,
                cfg.trading.lookback_windows,
                cfg.trading.lookahead_windows,
            )
            .await
        {
            Ok(markets) => {
                let market_count = markets.len();
                let token_ids = {
                    let mut locked = state.write().await;
                    for market in markets {
                        locked.upsert_market(market);
                    }
                    locked.live_token_ids()
                };
                info!(
                    markets = market_count,
                    tokens = token_ids.len(),
                    "market discovery refreshed"
                );
                if sub_tx.send(token_ids).is_err() {
                    warn!("CLOB subscription receiver dropped");
                }
            }
            Err(err) => warn!(error = %err, "market discovery failed"),
        }
    }
}

async fn status_loop(state: Arc<RwLock<RuntimeState>>) {
    let mut interval = time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;
        let snapshot = state.read().await.snapshot();
        info!(
            tick_assets = snapshot.latest_ticks.len(),
            markets = snapshot.markets.len(),
            quotes = snapshot.orderbook.len(),
            "runtime status"
        );
    }
}

async fn signal_loop(cfg: Arc<AppConfig>, state: Arc<RwLock<RuntimeState>>) {
    let engine = SignalEngine::new(cfg.risk.clone());
    let mut interval = time::interval(Duration::from_secs_f64(
        cfg.trading.signal_interval_seconds.max(0.05),
    ));
    let mut last_logged: HashMap<(String, String, Outcome, String), i64> = HashMap::new();

    loop {
        interval.tick().await;
        let snapshot = state.read().await.snapshot();
        let now_ms = chrono::Utc::now().timestamp_millis();

        for market in snapshot.markets.values() {
            let Some(tick) = snapshot.latest_ticks.get(&market.asset) else {
                continue;
            };
            let Some(price_to_beat) = market.price_to_beat else {
                continue;
            };
            let outcome = if tick.price >= price_to_beat {
                Outcome::Up
            } else {
                Outcome::Down
            };
            let token_id = market.token_for(outcome);
            let Some(quote) = snapshot.orderbook.quote(token_id) else {
                continue;
            };
            let Some(signal) = engine.evaluate(market, tick, quote, now_ms) else {
                continue;
            };

            let key = (
                signal.asset.clone(),
                signal.slug.clone(),
                signal.outcome,
                signal.reason.clone(),
            );
            if now_ms - last_logged.get(&key).copied().unwrap_or_default() < 2_000 {
                continue;
            }
            last_logged.insert(key, now_ms);

            if signal.reason == "accepted" {
                info!(
                    asset = %signal.asset,
                    slug = %signal.slug,
                    outcome = ?signal.outcome,
                    ask = signal.ask_price,
                    edge = signal.edge,
                    distance_bps = signal.distance_bps,
                    "paper signal accepted"
                );
            } else {
                info!(
                    asset = %signal.asset,
                    slug = %signal.slug,
                    outcome = ?signal.outcome,
                    reason = %signal.reason,
                    edge = signal.edge,
                    "paper signal rejected"
                );
            }
        }
    }
}
