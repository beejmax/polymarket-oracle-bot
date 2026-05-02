use std::collections::HashMap;

use serde_json::Value;

use crate::model::Quote;

#[derive(Debug, Clone, Default)]
pub struct OrderBookState {
    quotes: HashMap<String, Quote>,
}

impl OrderBookState {
    pub fn len(&self) -> usize {
        self.quotes.len()
    }

    pub fn quote(&self, token_id: &str) -> Option<&Quote> {
        self.quotes.get(token_id)
    }

    pub fn update_from_message(&mut self, message: &Value) {
        match message.get("event_type").and_then(Value::as_str) {
            Some("book") => self.update_book(message),
            Some("price_change") => self.update_price_change(message),
            Some("best_bid_ask") => self.update_best_bid_ask(message),
            _ => {}
        }
    }

    fn update_book(&mut self, message: &Value) {
        let Some(token_id) = message.get("asset_id").and_then(Value::as_str) else {
            return;
        };
        let (best_bid, bid_size) = best_level(message.get("bids"), true);
        let (best_ask, ask_size) = best_level(message.get("asks"), false);
        self.quotes.insert(
            token_id.to_string(),
            Quote {
                token_id: token_id.to_string(),
                best_bid,
                best_ask,
                bid_size,
                ask_size,
                ts_ms: as_i64(message.get("timestamp")),
            },
        );
    }

    fn update_price_change(&mut self, message: &Value) {
        let ts_ms = as_i64(message.get("timestamp"));
        let Some(changes) = message.get("price_changes").and_then(Value::as_array) else {
            return;
        };
        for change in changes {
            let Some(token_id) = change.get("asset_id").and_then(Value::as_str) else {
                continue;
            };
            let prior = self.quotes.get(token_id);
            self.quotes.insert(
                token_id.to_string(),
                Quote {
                    token_id: token_id.to_string(),
                    best_bid: as_f64(change.get("best_bid"))
                        .or_else(|| prior.and_then(|q| q.best_bid)),
                    best_ask: as_f64(change.get("best_ask"))
                        .or_else(|| prior.and_then(|q| q.best_ask)),
                    bid_size: prior.and_then(|q| q.bid_size),
                    ask_size: prior.and_then(|q| q.ask_size),
                    ts_ms,
                },
            );
        }
    }

    fn update_best_bid_ask(&mut self, message: &Value) {
        let Some(token_id) = message.get("asset_id").and_then(Value::as_str) else {
            return;
        };
        let prior = self.quotes.get(token_id);
        self.quotes.insert(
            token_id.to_string(),
            Quote {
                token_id: token_id.to_string(),
                best_bid: as_f64(message.get("best_bid")),
                best_ask: as_f64(message.get("best_ask")),
                bid_size: prior.and_then(|q| q.bid_size),
                ask_size: prior.and_then(|q| q.ask_size),
                ts_ms: as_i64(message.get("timestamp")),
            },
        );
    }
}

fn best_level(value: Option<&Value>, high_is_best: bool) -> (Option<f64>, Option<f64>) {
    let mut best_price = None;
    let mut best_size = None;
    for level in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(price) = as_f64(level.get("price")) else {
            continue;
        };
        let better = match best_price {
            None => true,
            Some(best) if high_is_best => price > best,
            Some(best) => price < best,
        };
        if better {
            best_price = Some(price);
            best_size = as_f64(level.get("size"));
        }
    }
    (best_price, best_size)
}

fn as_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn as_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn tracks_best_book_quote() {
        let mut book = OrderBookState::default();
        book.update_from_message(&json!({
            "event_type": "book",
            "asset_id": "up-token",
            "bids": [{"price": "0.40", "size": "1"}, {"price": "0.42", "size": "2"}],
            "asks": [{"price": "0.45", "size": "1"}, {"price": "0.44", "size": "2"}],
            "timestamp": "10"
        }));
        let quote = book.quote("up-token").unwrap();
        assert_eq!(quote.best_bid, Some(0.42));
        assert_eq!(quote.best_ask, Some(0.44));
    }
}
