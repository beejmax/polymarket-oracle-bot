use crate::config::RiskConfig;
use crate::model::{MarketWindow, Outcome, PriceTick, Quote, Signal};

#[derive(Debug, Clone)]
pub struct SignalEngine {
    risk: RiskConfig,
}

impl SignalEngine {
    pub fn new(risk: RiskConfig) -> Self {
        Self { risk }
    }

    pub fn estimate_probability(&self, distance_bps: f64) -> f64 {
        let excess = (distance_bps.abs() - self.risk.min_signal_distance_bps).max(0.0);
        if excess <= 0.0 {
            return 0.5;
        }
        let scale = self.risk.probability_scale_bps.max(1e-9);
        let span = (self.risk.probability_cap - 0.5).max(0.0);
        self.risk
            .probability_cap
            .min(0.5 + span * (1.0 - (-excess / scale).exp()))
    }

    pub fn evaluate(
        &self,
        market: &MarketWindow,
        tick: &PriceTick,
        quote: &Quote,
        now_ms: i64,
    ) -> Option<Signal> {
        let now_ts = now_ms as f64 / 1000.0;
        let price_to_beat = market.price_to_beat?;
        if market.closed || !market.active || !market.accepting_orders {
            return None;
        }
        if now_ts < market.start_ts as f64 + self.risk.min_elapsed_seconds {
            return None;
        }
        if now_ts > market.end_ts as f64 - self.risk.max_seconds_to_expiry {
            return None;
        }
        let ask = quote.best_ask?;
        let outcome = if tick.price >= price_to_beat {
            Outcome::Up
        } else {
            Outcome::Down
        };
        let token_id = market.token_for(outcome);
        if quote.token_id != token_id {
            return None;
        }

        let distance_bps = ((tick.price / price_to_beat) - 1.0) * 10_000.0;
        let signed_distance = if outcome == Outcome::Up {
            distance_bps
        } else {
            -distance_bps
        };
        if signed_distance < self.risk.min_signal_distance_bps {
            return None;
        }

        let estimated_prob = self.estimate_probability(signed_distance);
        if ask < self.risk.min_entry_price || ask > self.risk.max_entry_price {
            return Some(self.signal(
                market,
                tick,
                outcome,
                token_id,
                signed_distance,
                estimated_prob,
                ask,
                -1.0,
                "quote outside configured price band",
                now_ms,
            ));
        }

        let edge = estimated_prob - ask;
        if edge < self.risk.min_probability_edge {
            return Some(self.signal(
                market,
                tick,
                outcome,
                token_id,
                signed_distance,
                estimated_prob,
                ask,
                edge,
                "edge below threshold",
                now_ms,
            ));
        }

        Some(self.signal(
            market,
            tick,
            outcome,
            token_id,
            signed_distance,
            estimated_prob,
            ask,
            edge,
            "accepted",
            now_ms,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn signal(
        &self,
        market: &MarketWindow,
        tick: &PriceTick,
        outcome: Outcome,
        token_id: &str,
        distance_bps: f64,
        estimated_prob: f64,
        ask_price: f64,
        edge: f64,
        reason: &str,
        now_ms: i64,
    ) -> Signal {
        Signal {
            asset: market.asset.clone(),
            slug: market.slug.clone(),
            condition_id: market.condition_id.clone(),
            start_ts: market.start_ts,
            end_ts: market.end_ts,
            outcome,
            token_id: token_id.to_string(),
            price_to_beat: market.price_to_beat.expect("checked by caller"),
            observed_price: tick.price,
            distance_bps,
            estimated_prob,
            ask_price,
            edge,
            reason: reason.to_string(),
            created_at_ms: now_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::RiskConfig;
    use crate::model::{MarketWindow, PriceTick, Quote};

    use super::*;

    #[test]
    fn accepts_edge_signal() {
        let engine = SignalEngine::new(RiskConfig::default());
        let market = MarketWindow {
            asset: "BTC".into(),
            slug: "btc-updown-15m-1777738500".into(),
            event_id: "1".into(),
            market_id: "2".into(),
            condition_id: "0xabc".into(),
            start_ts: 1777738500,
            end_ts: 1777739400,
            up_token_id: "up-token".into(),
            down_token_id: "down-token".into(),
            tick_size: 0.01,
            min_order_size: 5.0,
            neg_risk: false,
            active: true,
            closed: false,
            accepting_orders: true,
            price_to_beat: Some(100.0),
        };
        let tick = PriceTick {
            asset: "BTC".into(),
            symbol: "btc/usd".into(),
            price: 101.0,
            feed_ts_ms: 1777738510000,
            received_ts_ms: 1777738510000,
        };
        let quote = Quote {
            token_id: "up-token".into(),
            best_bid: None,
            best_ask: Some(0.55),
            bid_size: None,
            ask_size: None,
            ts_ms: None,
        };
        let signal = engine
            .evaluate(&market, &tick, &quote, 1777738510000)
            .unwrap();
        assert_eq!(signal.outcome, Outcome::Up);
        assert_eq!(signal.reason, "accepted");
    }
}
