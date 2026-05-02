from __future__ import annotations

import asyncio
import os
from decimal import Decimal, ROUND_CEILING
from typing import Any

from .config import AppConfig
from .models import MarketWindow, OrderResult, Signal, SizeDecision


class PaperExecutor:
    async def submit(self, signal: Signal, size: SizeDecision, market: MarketWindow) -> OrderResult:
        return OrderResult(
            filled=True,
            order_id=f"paper-{signal.asset}-{signal.start_ts}-{signal.outcome.lower()}",
            fill_price=signal.ask_price,
            shares=size.shares,
            message="paper fill",
            raw=None,
        )


class LiveExecutor:
    def __init__(self, cfg: AppConfig) -> None:
        self.cfg = cfg
        self._client: Any | None = None
        self._variant: str | None = None

    async def submit(self, signal: Signal, size: SizeDecision, market: MarketWindow) -> OrderResult:
        client, variant = await asyncio.to_thread(self._client_or_create)
        price = _round_buy_limit(signal.ask_price, market.tick_size)
        raw = await asyncio.to_thread(
            self._post_buy_order,
            client,
            variant,
            signal.token_id,
            price,
            size.shares,
            size.cost_usd,
            market,
        )
        filled = _is_immediate_fill(raw)
        return OrderResult(
            filled=filled,
            order_id=_extract_order_id(raw),
            fill_price=price,
            shares=_extract_filled_shares(raw, size.shares),
            message="live FOK matched" if filled else f"live order not matched: {_extract_status(raw)}",
            raw=raw,
        )

    def _client_or_create(self) -> tuple[Any, str]:
        if self._client is not None and self._variant is not None:
            return self._client, self._variant
        try:
            self._client = self._create_v2_client()
            self._variant = "v2"
            return self._client, self._variant
        except ImportError:
            self._client = self._create_v1_client()
            self._variant = "v1"
            return self._client, self._variant

    def _create_v2_client(self) -> Any:
        from py_clob_client_v2 import ClobClient

        creds = {
            "apiKey": _required_env("POLYMARKET_API_KEY"),
            "secret": _required_env("POLYMARKET_API_SECRET"),
            "passphrase": _required_env("POLYMARKET_API_PASSPHRASE"),
        }
        return ClobClient(
            host=self.cfg.polymarket.clob_base_url,
            chain_id=self.cfg.polymarket.chain_id,
            key=_required_env("POLYMARKET_PRIVATE_KEY"),
            creds=creds,
            signature_type=int(os.getenv("POLYMARKET_SIGNATURE_TYPE", "1")),
            funder=os.getenv("POLYMARKET_FUNDER_ADDRESS"),
        )

    def _create_v1_client(self) -> Any:
        from py_clob_client.client import ClobClient
        from py_clob_client.clob_types import ApiCreds

        creds = ApiCreds(
            api_key=_required_env("POLYMARKET_API_KEY"),
            api_secret=_required_env("POLYMARKET_API_SECRET"),
            api_passphrase=_required_env("POLYMARKET_API_PASSPHRASE"),
        )
        return ClobClient(
            host=self.cfg.polymarket.clob_base_url,
            key=_required_env("POLYMARKET_PRIVATE_KEY"),
            chain_id=self.cfg.polymarket.chain_id,
            creds=creds,
            signature_type=int(os.getenv("POLYMARKET_SIGNATURE_TYPE", "1")),
            funder=os.getenv("POLYMARKET_FUNDER_ADDRESS"),
        )

    def _post_buy_order(
        self,
        client: Any,
        variant: str,
        token_id: str,
        price: float,
        shares: float,
        cost_usd: float,
        market: MarketWindow,
    ) -> Any:
        if variant == "v2":
            from py_clob_client_v2 import MarketOrderArgs, OrderType, PartialCreateOrderOptions
            from py_clob_client_v2.order_builder.constants import BUY

            return client.create_and_post_market_order(
                MarketOrderArgs(token_id=token_id, amount=cost_usd, price=price, side=BUY),
                options=PartialCreateOrderOptions(
                    tick_size=str(market.tick_size),
                    neg_risk=market.neg_risk,
                ),
                order_type=OrderType.FOK,
            )

        from py_clob_client.clob_types import MarketOrderArgs, OrderType, PartialCreateOrderOptions
        from py_clob_client.order_builder.constants import BUY

        signed = client.create_market_order(
            MarketOrderArgs(token_id=token_id, amount=cost_usd, price=price, side=BUY),
            options=PartialCreateOrderOptions(tick_size=str(market.tick_size), neg_risk=market.neg_risk),
        )
        return client.post_order(signed, OrderType.FOK)


def executor_for_config(cfg: AppConfig) -> PaperExecutor | LiveExecutor:
    if cfg.trading.mode == "live":
        return LiveExecutor(cfg)
    return PaperExecutor()


def _required_env(name: str) -> str:
    value = os.getenv(name)
    if not value:
        raise RuntimeError(f"{name} is required for live execution")
    return value


def _round_buy_limit(price: float, tick_size: float) -> float:
    tick = Decimal(str(tick_size))
    value = Decimal(str(price))
    rounded = (value / tick).to_integral_value(rounding=ROUND_CEILING) * tick
    return float(min(Decimal("0.99"), rounded))


def _extract_order_id(raw: Any) -> str | None:
    if isinstance(raw, dict):
        for key in ("orderID", "order_id", "id"):
            if raw.get(key):
                return str(raw[key])
    return None


def _extract_status(raw: Any) -> str:
    if isinstance(raw, dict):
        return str(raw.get("status") or raw.get("errorMsg") or raw.get("error") or "unknown")
    return "unknown"


def _is_immediate_fill(raw: Any) -> bool:
    if not isinstance(raw, dict):
        return False
    if raw.get("success") is False:
        return False
    return str(raw.get("status") or "").lower() == "matched"


def _extract_filled_shares(raw: Any, fallback: float) -> float:
    if isinstance(raw, dict):
        for key in ("takingAmount", "taking_amount", "filledSize", "sizeMatched"):
            value = raw.get(key)
            if value in (None, ""):
                continue
            try:
                parsed = float(value)
            except (TypeError, ValueError):
                continue
            if parsed > 0.0:
                return parsed
    return fallback
