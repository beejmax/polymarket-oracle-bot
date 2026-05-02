use std::collections::{BTreeSet, HashMap, VecDeque};

use crate::model::{MarketWindow, PriceTick};
use crate::orderbook::OrderBookState;

#[derive(Debug, Clone, Default)]
pub struct RuntimeSnapshot {
    pub latest_ticks: HashMap<String, PriceTick>,
    pub markets: HashMap<String, MarketWindow>,
    pub orderbook: OrderBookState,
}

#[derive(Debug)]
pub struct RuntimeState {
    opening_capture_grace_seconds: i64,
    pub latest_ticks: HashMap<String, PriceTick>,
    recent_ticks: HashMap<String, VecDeque<PriceTick>>,
    pub markets: HashMap<String, MarketWindow>,
    pub orderbook: OrderBookState,
}

impl RuntimeState {
    pub fn new(opening_capture_grace_seconds: i64) -> Self {
        Self {
            opening_capture_grace_seconds,
            latest_ticks: HashMap::new(),
            recent_ticks: HashMap::new(),
            markets: HashMap::new(),
            orderbook: OrderBookState::default(),
        }
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            latest_ticks: self.latest_ticks.clone(),
            markets: self.markets.clone(),
            orderbook: self.orderbook.clone(),
        }
    }

    pub fn apply_tick(&mut self, tick: PriceTick) {
        let ticks = self.recent_ticks.entry(tick.asset.clone()).or_default();
        ticks.push_back(tick.clone());
        while ticks.len() > 2_000 {
            ticks.pop_front();
        }
        self.latest_ticks.insert(tick.asset.clone(), tick.clone());

        for market in self.markets.values_mut() {
            if market.asset != tick.asset || market.price_to_beat.is_some() {
                continue;
            }
            let feed_ts = tick.feed_ts_ms / 1000;
            if feed_ts >= market.start_ts
                && feed_ts <= market.start_ts + self.opening_capture_grace_seconds
            {
                market.price_to_beat = Some(tick.price);
            }
        }
    }

    pub fn upsert_market(&mut self, mut market: MarketWindow) {
        if let Some(prior) = self.markets.get(&market.key()) {
            if market.price_to_beat.is_none() {
                market.price_to_beat = prior.price_to_beat;
            }
        }
        if market.price_to_beat.is_none() {
            self.try_backfill_price_to_beat(&mut market);
        }
        self.markets.insert(market.key(), market);
    }

    pub fn live_token_ids(&self) -> BTreeSet<String> {
        let mut token_ids = BTreeSet::new();
        for market in self.markets.values() {
            if market.closed {
                continue;
            }
            token_ids.insert(market.up_token_id.clone());
            token_ids.insert(market.down_token_id.clone());
        }
        token_ids
    }

    fn try_backfill_price_to_beat(&self, market: &mut MarketWindow) {
        let start_ms = market.start_ts * 1000;
        let end_ms = (market.start_ts + self.opening_capture_grace_seconds) * 1000;
        let Some(ticks) = self.recent_ticks.get(&market.asset) else {
            return;
        };
        market.price_to_beat = ticks
            .iter()
            .filter(|tick| tick.feed_ts_ms >= start_ms && tick.feed_ts_ms <= end_ms)
            .min_by_key(|tick| tick.feed_ts_ms)
            .map(|tick| tick.price);
    }
}
