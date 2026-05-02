from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Literal

Outcome = Literal["Up", "Down"]
Mode = Literal["paper", "live"]


@dataclass(slots=True, frozen=True)
class PriceTick:
    asset: str
    symbol: str
    price: float
    feed_ts_ms: int
    received_ts_ms: int


@dataclass(slots=True)
class Quote:
    token_id: str
    best_bid: float | None = None
    best_ask: float | None = None
    bid_size: float | None = None
    ask_size: float | None = None
    ts_ms: int | None = None


@dataclass(slots=True)
class MarketWindow:
    asset: str
    slug: str
    event_id: str
    market_id: str
    condition_id: str
    start_ts: int
    end_ts: int
    tokens: dict[Outcome, str]
    tick_size: float
    min_order_size: float
    neg_risk: bool
    active: bool
    closed: bool
    accepting_orders: bool
    price_to_beat: float | None = None
    raw: dict[str, Any] = field(default_factory=dict)

    @property
    def key(self) -> str:
        return f"{self.asset}:{self.start_ts}"

    def token_for(self, outcome: Outcome) -> str:
        return self.tokens[outcome]

    def outcome_for_token(self, token_id: str) -> Outcome | None:
        for outcome, candidate in self.tokens.items():
            if candidate == token_id:
                return outcome
        return None


@dataclass(slots=True, frozen=True)
class Signal:
    asset: str
    slug: str
    condition_id: str
    start_ts: int
    end_ts: int
    outcome: Outcome
    token_id: str
    price_to_beat: float
    observed_price: float
    distance_bps: float
    estimated_prob: float
    ask_price: float
    edge: float
    reason: str
    created_at_ms: int


@dataclass(slots=True, frozen=True)
class SizeDecision:
    shares: float
    cost_usd: float
    kelly_fraction: float
    reason: str

    @property
    def accepted(self) -> bool:
        return self.shares > 0.0 and self.cost_usd > 0.0


@dataclass(slots=True, frozen=True)
class OrderResult:
    filled: bool
    order_id: str | None
    fill_price: float
    shares: float
    message: str
    raw: Any = None


@dataclass(slots=True)
class Position:
    trade_id: str
    mode: Mode
    asset: str
    slug: str
    condition_id: str
    outcome: Outcome
    token_id: str
    start_ts: int
    end_ts: int
    shares: float
    entry_price: float
    cost_usd: float
    order_id: str | None
    price_to_beat: float
    entry_oracle_price: float
    opened_at_ms: int
    status: str = "open"

