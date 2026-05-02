use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AssetConfig {
    pub symbol: String,
    pub slug_prefix: String,
    pub chainlink_symbol: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct TradingConfig {
    pub mode: String,
    pub timeframe: String,
    pub market_poll_interval_seconds: f64,
    pub signal_interval_seconds: f64,
    pub lookback_windows: i32,
    pub lookahead_windows: i32,
    pub opening_capture_grace_seconds: i64,
    pub live_enable_orders: bool,
    pub live_confirm_real_money: bool,
    pub live_confirm_polymarket_terms: bool,
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            mode: "paper".to_string(),
            timeframe: "15m".to_string(),
            market_poll_interval_seconds: 4.0,
            signal_interval_seconds: 0.25,
            lookback_windows: 1,
            lookahead_windows: 2,
            opening_capture_grace_seconds: 20,
            live_enable_orders: false,
            live_confirm_real_money: false,
            live_confirm_polymarket_terms: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RiskConfig {
    pub min_signal_distance_bps: f64,
    pub probability_scale_bps: f64,
    pub probability_cap: f64,
    pub min_probability_edge: f64,
    pub min_entry_price: f64,
    pub max_entry_price: f64,
    pub min_elapsed_seconds: f64,
    pub max_seconds_to_expiry: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            min_signal_distance_bps: 4.0,
            probability_scale_bps: 12.0,
            probability_cap: 0.88,
            min_probability_edge: 0.04,
            min_entry_price: 0.05,
            max_entry_price: 0.92,
            min_elapsed_seconds: 1.0,
            max_seconds_to_expiry: 8.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PolymarketConfig {
    pub gamma_base_url: String,
    pub market_ws_url: String,
    pub rtds_ws_url: String,
    pub market_ws_ping_payload: String,
}

impl Default for PolymarketConfig {
    fn default() -> Self {
        Self {
            gamma_base_url: "https://gamma-api.polymarket.com".to_string(),
            market_ws_url: "wss://ws-subscriptions-clob.polymarket.com/ws/market".to_string(),
            rtds_ws_url: "wss://ws-live-data.polymarket.com".to_string(),
            market_ws_ping_payload: "PING".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub trading: TradingConfig,
    pub risk: RiskConfig,
    pub polymarket: PolymarketConfig,
    pub assets: Vec<AssetConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            trading: TradingConfig::default(),
            risk: RiskConfig::default(),
            polymarket: PolymarketConfig::default(),
            assets: vec![
                AssetConfig::new("BTC", "btc", "btc/usd"),
                AssetConfig::new("ETH", "eth", "eth/usd"),
                AssetConfig::new("SOL", "sol", "sol/usd"),
                AssetConfig::new("XRP", "xrp", "xrp/usd"),
            ],
        }
    }
}

impl AssetConfig {
    fn new(symbol: &str, slug_prefix: &str, chainlink_symbol: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            slug_prefix: slug_prefix.to_string(),
            chainlink_symbol: chainlink_symbol.to_string(),
            enabled: true,
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw =
            fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parse config {}", path.display()))
    }

    pub fn enabled_assets(&self) -> Vec<AssetConfig> {
        self.assets
            .iter()
            .filter(|asset| asset.enabled)
            .cloned()
            .collect()
    }

    pub fn validate(&self) -> Result<()> {
        if self.trading.mode != "paper" {
            bail!("Rust skeleton only supports paper mode for now");
        }
        if self.enabled_assets().is_empty() {
            bail!("at least one asset must be enabled");
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}
