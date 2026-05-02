#![allow(dead_code)]

use std::fmt;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct PriceTick {
    pub asset: String,
    pub symbol: String,
    pub price: f64,
    pub feed_ts_ms: i64,
    pub received_ts_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Up,
    Down,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Up => write!(f, "Up"),
            Outcome::Down => write!(f, "Down"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Quote {
    pub token_id: String,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub bid_size: Option<f64>,
    pub ask_size: Option<f64>,
    pub ts_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MarketWindow {
    pub asset: String,
    pub slug: String,
    pub event_id: String,
    pub market_id: String,
    pub condition_id: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub up_token_id: String,
    pub down_token_id: String,
    pub tick_size: f64,
    pub min_order_size: f64,
    pub neg_risk: bool,
    pub active: bool,
    pub closed: bool,
    pub accepting_orders: bool,
    pub price_to_beat: Option<f64>,
}

impl MarketWindow {
    pub fn key(&self) -> String {
        format!("{}:{}", self.asset, self.start_ts)
    }

    pub fn token_for(&self, outcome: Outcome) -> &str {
        match outcome {
            Outcome::Up => &self.up_token_id,
            Outcome::Down => &self.down_token_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub asset: String,
    pub slug: String,
    pub condition_id: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub outcome: Outcome,
    pub token_id: String,
    pub price_to_beat: f64,
    pub observed_price: f64,
    pub distance_bps: f64,
    pub estimated_prob: f64,
    pub ask_price: f64,
    pub edge: f64,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug)]
pub enum Event {
    Tick(PriceTick),
    MarketMessage(Value),
}
