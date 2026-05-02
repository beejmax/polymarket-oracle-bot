#![allow(dead_code)]

#[path = "../config.rs"]
mod config;
#[path = "../model.rs"]
mod model;
#[path = "../orderbook.rs"]
mod orderbook;
#[path = "../signal.rs"]
mod signal;

use std::env;
use std::time::Instant;

use config::RiskConfig;
use model::{MarketWindow, PriceTick};
use orderbook::OrderBookState;
use serde_json::json;
use signal::SignalEngine;

fn percentile(values: &[f64], pct: f64) -> f64 {
    let mut ordered = values.to_vec();
    ordered.sort_by(|left, right| left.total_cmp(right));
    let idx = (ordered.len() - 1) as f64 * pct;
    let low = idx.floor() as usize;
    let high = (low + 1).min(ordered.len() - 1);
    let weight = idx - low as f64;
    ordered[low] * (1.0 - weight) + ordered[high] * weight
}

fn arg_value(name: &str, default: usize) -> usize {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args
                .next()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default);
        }
    }
    default
}

fn main() {
    let iterations = arg_value("--iterations", 1_000_000);
    let sample_every = arg_value("--sample-every", 1);
    let market = MarketWindow {
        asset: "BTC".to_string(),
        slug: "btc-updown-15m-1777738500".to_string(),
        event_id: "1".to_string(),
        market_id: "2".to_string(),
        condition_id: "0xabc".to_string(),
        start_ts: 1_777_738_500,
        end_ts: 1_777_739_400,
        up_token_id: "up-token".to_string(),
        down_token_id: "down-token".to_string(),
        tick_size: 0.01,
        min_order_size: 5.0,
        neg_risk: false,
        active: true,
        closed: false,
        accepting_orders: true,
        price_to_beat: Some(100.0),
    };
    let tick = PriceTick {
        asset: "BTC".to_string(),
        symbol: "btc/usd".to_string(),
        price: 101.0,
        feed_ts_ms: 1_777_738_510_000,
        received_ts_ms: 1_777_738_510_001,
    };
    let book_msg = json!({
        "event_type": "book",
        "asset_id": "up-token",
        "bids": [{"price": "0.52", "size": "100"}, {"price": "0.53", "size": "40"}],
        "asks": [{"price": "0.56", "size": "50"}, {"price": "0.55", "size": "20"}],
        "timestamp": "1777738510000"
    });

    let mut orderbook = OrderBookState::default();
    let signal_engine = SignalEngine::new(RiskConfig::default());
    let now_ms = 1_777_738_510_000;
    let mut samples_us = Vec::with_capacity(iterations / sample_every.max(1));

    for _ in 0..1_000 {
        orderbook.update_from_message(&book_msg);
        let quote = orderbook.quote("up-token");
        let _signal = quote.and_then(|quote| signal_engine.evaluate(&market, &tick, quote, now_ms));
    }
    let mut accepted = 0usize;

    let started = Instant::now();
    for index in 0..iterations {
        let sample = index % sample_every == 0;
        let before = if sample { Some(Instant::now()) } else { None };
        orderbook.update_from_message(&book_msg);
        let quote = orderbook.quote("up-token");
        let signal = quote.and_then(|quote| signal_engine.evaluate(&market, &tick, quote, now_ms));
        if matches!(
            signal.as_ref().map(|signal| signal.reason.as_str()),
            Some("accepted")
        ) {
            accepted += 1;
        }
        if let Some(before) = before {
            samples_us.push(before.elapsed().as_secs_f64() * 1_000_000.0);
        }
    }
    let total_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let mean_us = samples_us.iter().sum::<f64>() / samples_us.len() as f64;
    let max_us = samples_us.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    println!("runtime=rust");
    println!("iterations={iterations}");
    println!("accepted={accepted}");
    println!("total_ms={total_ms:.3}");
    println!(
        "throughput_ops_per_sec={:.3}",
        iterations as f64 / (total_ms / 1000.0)
    );
    println!("mean_us={mean_us:.3}");
    println!("p50_us={:.3}", percentile(&samples_us, 0.50));
    println!("p90_us={:.3}", percentile(&samples_us, 0.90));
    println!("p99_us={:.3}", percentile(&samples_us, 0.99));
    println!("max_us={max_us:.3}");
}
