from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass, field
from typing import Any

from .models import Quote


def _float_or_none(value: Any) -> float | None:
    if value is None or value == "":
        return None
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def _best_bid(levels: Iterable[dict[str, Any]]) -> tuple[float | None, float | None]:
    best_price: float | None = None
    best_size: float | None = None
    for level in levels:
        price = _float_or_none(level.get("price"))
        size = _float_or_none(level.get("size"))
        if price is None:
            continue
        if best_price is None or price > best_price:
            best_price = price
            best_size = size
    return best_price, best_size


def _best_ask(levels: Iterable[dict[str, Any]]) -> tuple[float | None, float | None]:
    best_price: float | None = None
    best_size: float | None = None
    for level in levels:
        price = _float_or_none(level.get("price"))
        size = _float_or_none(level.get("size"))
        if price is None:
            continue
        if best_price is None or price < best_price:
            best_price = price
            best_size = size
    return best_price, best_size


@dataclass(slots=True)
class OrderBookState:
    quotes: dict[str, Quote] = field(default_factory=dict)

    def update_from_message(self, message: dict[str, Any]) -> list[Quote]:
        event_type = message.get("event_type")
        if event_type == "book":
            quote = self._update_book(message)
            return [quote] if quote else []
        if event_type == "price_change":
            return self._update_price_change(message)
        if event_type == "best_bid_ask":
            quote = self._update_best_bid_ask(message)
            return [quote] if quote else []
        return []

    def quote(self, token_id: str) -> Quote | None:
        return self.quotes.get(token_id)

    def _update_book(self, message: dict[str, Any]) -> Quote | None:
        token_id = str(message.get("asset_id") or "")
        if not token_id:
            return None
        bid, bid_size = _best_bid(message.get("bids") or [])
        ask, ask_size = _best_ask(message.get("asks") or [])
        quote = Quote(
            token_id=token_id,
            best_bid=bid,
            best_ask=ask,
            bid_size=bid_size,
            ask_size=ask_size,
            ts_ms=_int_or_none(message.get("timestamp")),
        )
        self.quotes[token_id] = quote
        return quote

    def _update_price_change(self, message: dict[str, Any]) -> list[Quote]:
        updated = []
        ts_ms = _int_or_none(message.get("timestamp"))
        for change in message.get("price_changes") or []:
            token_id = str(change.get("asset_id") or "")
            if not token_id:
                continue
            prior = self.quotes.get(token_id, Quote(token_id=token_id))
            quote = Quote(
                token_id=token_id,
                best_bid=_float_or_none(change.get("best_bid")) or prior.best_bid,
                best_ask=_float_or_none(change.get("best_ask")) or prior.best_ask,
                bid_size=prior.bid_size,
                ask_size=prior.ask_size,
                ts_ms=ts_ms,
            )
            self.quotes[token_id] = quote
            updated.append(quote)
        return updated

    def _update_best_bid_ask(self, message: dict[str, Any]) -> Quote | None:
        token_id = str(message.get("asset_id") or "")
        if not token_id:
            return None
        prior = self.quotes.get(token_id, Quote(token_id=token_id))
        quote = Quote(
            token_id=token_id,
            best_bid=_float_or_none(message.get("best_bid")),
            best_ask=_float_or_none(message.get("best_ask")),
            bid_size=prior.bid_size,
            ask_size=prior.ask_size,
            ts_ms=_int_or_none(message.get("timestamp")),
        )
        self.quotes[token_id] = quote
        return quote


def _int_or_none(value: Any) -> int | None:
    if value is None or value == "":
        return None
    try:
        return int(float(value))
    except (TypeError, ValueError):
        return None

