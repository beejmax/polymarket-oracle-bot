use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures_util::future::join_all;
use reqwest::StatusCode;
use serde_json::Value;

use crate::config::AssetConfig;
use crate::model::MarketWindow;
use crate::timeframe::{candidate_window_starts, slug_for};

#[derive(Clone)]
pub struct GammaClient {
    http: reqwest::Client,
    base_url: String,
}

impl GammaClient {
    pub fn new(base_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn fetch_event_by_slug(&self, slug: &str) -> Result<Option<Value>> {
        let url = format!("{}/events/slug/{}", self.base_url, slug);
        let response = self.http.get(url).send().await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response
            .error_for_status()?
            .json::<Value>()
            .await
            .map(Some)
            .context("decode Gamma event")
    }

    pub async fn discover_windows(
        &self,
        assets: &[AssetConfig],
        timeframe: &str,
        now_ts: i64,
        lookback: i32,
        lookahead: i32,
    ) -> Result<Vec<MarketWindow>> {
        let starts = candidate_window_starts(now_ts, timeframe, lookback, lookahead)?;
        let mut requests = Vec::new();
        for asset in assets {
            for start_ts in &starts {
                requests.push((
                    asset.clone(),
                    slug_for(&asset.slug_prefix, *start_ts, timeframe),
                ));
            }
        }

        let responses = join_all(
            requests
                .iter()
                .map(|(_, slug)| self.fetch_event_by_slug(slug)),
        )
        .await;
        let mut markets = Vec::new();
        for ((asset, _slug), response) in requests.into_iter().zip(responses) {
            let Some(event) = response? else {
                continue;
            };
            if let Some(market) = parse_market_event(&asset, &event) {
                markets.push(market);
            }
        }
        Ok(markets)
    }
}

pub fn parse_market_event(asset: &AssetConfig, event: &Value) -> Option<MarketWindow> {
    let raw_market = event.get("markets")?.as_array()?.first()?;
    let outcomes = json_string_array(raw_market.get("outcomes")?)?;
    let token_ids = json_string_array(raw_market.get("clobTokenIds")?)?;
    if outcomes.len() != token_ids.len() {
        return None;
    }

    let mut up_token_id = None;
    let mut down_token_id = None;
    for (outcome, token_id) in outcomes.iter().zip(token_ids.iter()) {
        match outcome.as_str() {
            "Up" => up_token_id = Some(token_id.clone()),
            "Down" => down_token_id = Some(token_id.clone()),
            _ => {}
        }
    }

    let slug = event
        .get("slug")
        .or_else(|| raw_market.get("slug"))?
        .as_str()?
        .to_string();
    let start_ts = parse_time_to_ts(raw_market.get("eventStartTime"))
        .or_else(|| parse_time_to_ts(event.get("startTime")))
        .or_else(|| start_from_slug(&slug))?;
    let end_ts = parse_time_to_ts(raw_market.get("endDate"))
        .or_else(|| parse_time_to_ts(event.get("endDate")))?;

    Some(MarketWindow {
        asset: asset.symbol.to_uppercase(),
        slug,
        event_id: as_string(event.get("id")),
        market_id: as_string(raw_market.get("id")),
        condition_id: as_string(raw_market.get("conditionId")),
        start_ts,
        end_ts,
        up_token_id: up_token_id?,
        down_token_id: down_token_id?,
        tick_size: as_f64(raw_market.get("orderPriceMinTickSize")).unwrap_or(0.01),
        min_order_size: as_f64(raw_market.get("orderMinSize")).unwrap_or(5.0),
        neg_risk: raw_market
            .get("negRisk")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        active: raw_market
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && event.get("active").and_then(Value::as_bool).unwrap_or(true),
        closed: raw_market
            .get("closed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || event
                .get("closed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        accepting_orders: raw_market
            .get("acceptingOrders")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        price_to_beat: as_f64(raw_market.get("priceToBeat")).filter(|price| *price > 0.0),
    })
}

fn parse_time_to_ts(value: Option<&Value>) -> Option<i64> {
    let text = value?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    let mut normalized = text.replace(' ', "T");
    if normalized.ends_with("+00") {
        normalized.push_str(":00");
    }
    DateTime::parse_from_rfc3339(&normalized)
        .ok()
        .map(|dt| dt.with_timezone(&Utc).timestamp())
}

fn start_from_slug(slug: &str) -> Option<i64> {
    slug.rsplit('-').next()?.parse().ok()
}

fn json_string_array(value: &Value) -> Option<Vec<String>> {
    let parsed = match value {
        Value::Array(items) => Value::Array(items.clone()),
        Value::String(text) => serde_json::from_str(text).ok()?,
        _ => return None,
    };
    parsed
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(ToString::to_string))
        .collect()
}

fn as_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn as_f64(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn parses_gamma_event() {
        let asset = AssetConfig {
            symbol: "BTC".into(),
            slug_prefix: "btc".into(),
            chainlink_symbol: "btc/usd".into(),
            enabled: true,
        };
        let event = json!({
            "id": "1",
            "slug": "btc-updown-15m-1777738500",
            "active": true,
            "closed": false,
            "markets": [{
                "id": "2",
                "conditionId": "0xabc",
                "eventStartTime": "2026-05-02T16:15:00Z",
                "endDate": "2026-05-02T16:30:00Z",
                "outcomes": "[\"Up\",\"Down\"]",
                "clobTokenIds": "[\"up-token\",\"down-token\"]",
                "active": true,
                "closed": false,
                "acceptingOrders": true
            }]
        });
        let market = parse_market_event(&asset, &event).unwrap();
        assert_eq!(market.start_ts, 1777738500);
        assert_eq!(market.up_token_id, "up-token");
    }
}
