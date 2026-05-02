from __future__ import annotations

import json
import asyncio
from datetime import datetime, timezone
from typing import Any

from .config import AssetConfig, PolymarketConfig
from .models import MarketWindow, Outcome
from .timeframes import candidate_window_starts, slug_for


def parse_time_to_ts(value: Any) -> int | None:
    if not value:
        return None
    if isinstance(value, (int, float)):
        return int(value)
    text = str(value).strip()
    if not text:
        return None
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    if " " in text and "T" not in text:
        text = text.replace(" ", "T", 1)
    try:
        dt = datetime.fromisoformat(text)
    except ValueError:
        return None
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return int(dt.timestamp())


def parse_market_event(asset: AssetConfig, event: dict[str, Any]) -> MarketWindow | None:
    markets = event.get("markets") or []
    if not markets:
        return None
    raw_market = markets[0]
    outcomes = _json_list(raw_market.get("outcomes"))
    token_ids = _json_list(raw_market.get("clobTokenIds"))
    if len(outcomes) != len(token_ids):
        return None

    tokens: dict[Outcome, str] = {}
    for outcome, token_id in zip(outcomes, token_ids, strict=False):
        if outcome in {"Up", "Down"}:
            tokens[outcome] = str(token_id)
    if "Up" not in tokens or "Down" not in tokens:
        return None

    start_ts = (
        parse_time_to_ts(raw_market.get("eventStartTime"))
        or parse_time_to_ts(event.get("startTime"))
        or _start_from_slug(str(event.get("slug") or raw_market.get("slug") or ""))
    )
    end_ts = parse_time_to_ts(raw_market.get("endDate")) or parse_time_to_ts(event.get("endDate"))
    if start_ts is None or end_ts is None:
        return None

    return MarketWindow(
        asset=asset.symbol.upper(),
        slug=str(event.get("slug") or raw_market.get("slug")),
        event_id=str(event.get("id") or ""),
        market_id=str(raw_market.get("id") or ""),
        condition_id=str(raw_market.get("conditionId") or ""),
        start_ts=start_ts,
        end_ts=end_ts,
        tokens=tokens,
        tick_size=float(raw_market.get("orderPriceMinTickSize") or 0.01),
        min_order_size=float(raw_market.get("orderMinSize") or 5.0),
        neg_risk=bool(raw_market.get("negRisk")),
        active=bool(raw_market.get("active")) and bool(event.get("active", True)),
        closed=bool(raw_market.get("closed")) or bool(event.get("closed")),
        accepting_orders=bool(raw_market.get("acceptingOrders")),
        price_to_beat=_float_or_none(raw_market.get("priceToBeat")),
        raw=event,
    )


class GammaClient:
    def __init__(self, cfg: PolymarketConfig) -> None:
        self.cfg = cfg
        self._client: Any | None = None

    async def __aenter__(self) -> "GammaClient":
        import httpx

        self._client = httpx.AsyncClient(base_url=self.cfg.gamma_base_url, timeout=5.0)
        return self

    async def __aexit__(self, *_exc: object) -> None:
        if self._client is not None:
            await self._client.aclose()

    async def fetch_event_by_slug(self, slug: str) -> dict[str, Any] | None:
        if self._client is None:
            raise RuntimeError("GammaClient must be used as an async context manager")
        response = await self._client.get(f"/events/slug/{slug}")
        if response.status_code == 404:
            return None
        response.raise_for_status()
        data = response.json()
        return data if isinstance(data, dict) else None

    async def discover_windows(
        self,
        assets: list[AssetConfig],
        timeframe: str,
        now_ts: int,
        lookback: int,
        lookahead: int,
    ) -> list[MarketWindow]:
        starts = candidate_window_starts(now_ts, timeframe, lookback, lookahead)
        requests: list[tuple[AssetConfig, str]] = []
        for asset in assets:
            for start_ts in starts:
                slug = slug_for(asset.slug_prefix, start_ts, timeframe)
                requests.append((asset, slug))
        events = await asyncio.gather(
            *(self.fetch_event_by_slug(slug) for _asset, slug in requests),
            return_exceptions=True,
        )
        windows: list[MarketWindow] = []
        for (asset, _slug), event in zip(requests, events, strict=False):
            if isinstance(event, Exception) or event is None:
                continue
            market = parse_market_event(asset, event)
            if market is not None:
                windows.append(market)
        return windows


def _start_from_slug(slug: str) -> int | None:
    try:
        return int(slug.rsplit("-", 1)[-1])
    except (ValueError, IndexError):
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


def _float_or_none(value: Any) -> float | None:
    if value is None or value == "":
        return None
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    return parsed if parsed > 0.0 else None
