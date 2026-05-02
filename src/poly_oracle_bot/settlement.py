from __future__ import annotations

import json
from typing import Any

from .models import Outcome, Position


def local_winning_outcome(price_to_beat: float, final_price: float) -> Outcome:
    return "Up" if final_price >= price_to_beat else "Down"


def position_settlement_pnl(position: Position, winning_outcome: Outcome) -> float:
    payout = position.shares if position.outcome == winning_outcome else 0.0
    return payout - position.cost_usd


def gamma_winning_outcome(event: dict[str, Any]) -> Outcome | None:
    markets = event.get("markets") or []
    if not markets:
        return None
    market = markets[0]
    status = str(market.get("umaResolutionStatus") or "").lower()
    if status and status != "resolved":
        return None

    outcomes = _json_list(market.get("outcomes"))
    prices = _json_list(market.get("outcomePrices"))
    if len(outcomes) != len(prices):
        return None
    for outcome, price in zip(outcomes, prices, strict=False):
        try:
            value = float(price)
        except (TypeError, ValueError):
            continue
        if value >= 0.999 and outcome in {"Up", "Down"}:
            return outcome
    return None


def _json_list(value: Any) -> list[Any]:
    if isinstance(value, list):
        return value
    if not isinstance(value, str):
        return []
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError:
        return []
    return parsed if isinstance(parsed, list) else []

