from __future__ import annotations

import asyncio
import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from .config import AppConfig
from .execution import live_executor_dry_run, missing_live_env_vars
from .models import MarketWindow
from .polymarket import GammaClient


@dataclass(slots=True)
class CheckResult:
    name: str
    ok: bool
    message: str


async def run_preflight(
    cfg: AppConfig,
    db_path: Path,
    timeout_seconds: float = 10.0,
    executor_preflight: bool = False,
) -> list[CheckResult]:
    results: list[CheckResult] = [
        _path_writable("db_parent", db_path.parent),
        _path_writable("telemetry_parent", Path(cfg.telemetry.path).parent),
        _live_credentials_check(cfg, executor_preflight),
        _telegram_check(cfg),
    ]

    markets = []
    open_markets: list[MarketWindow] = []
    try:
        async with GammaClient(cfg.polymarket) as gamma:
            markets = await gamma.discover_windows(
                cfg.enabled_assets,
                cfg.trading.timeframe,
                int(time.time()),
                cfg.trading.lookback_windows,
                cfg.trading.lookahead_windows,
            )
        open_markets = [market for market in markets if market.active and not market.closed]
        results.append(
            CheckResult(
                "gamma_market_discovery",
                bool(open_markets),
                f"discovered {len(markets)} windows, {len(open_markets)} active/open",
            )
        )
    except Exception as exc:
        results.append(CheckResult("gamma_market_discovery", False, str(exc)))

    token_ids = [token for market in markets if not market.closed for token in market.tokens.values()]
    if token_ids:
        results.append(await _clob_ws_check(cfg.polymarket.market_ws_url, token_ids[:2], timeout_seconds))
    else:
        results.append(CheckResult("clob_market_ws", False, "no token ids from Gamma discovery"))

    if executor_preflight:
        market = next((item for item in open_markets if item.accepting_orders and item.tokens), None)
        results.append(await _executor_dry_run_check(cfg, market, timeout_seconds))

    if cfg.enabled_assets:
        asset = cfg.enabled_assets[0]
        results.append(
            await _chainlink_ws_check(
                cfg.polymarket.rtds_ws_url,
                asset.symbol.upper(),
                asset.chainlink_symbol.lower(),
                timeout_seconds,
            )
        )
    else:
        results.append(CheckResult("chainlink_rtds", False, "no enabled assets"))

    return results


def format_results(results: list[CheckResult]) -> str:
    return "\n".join(
        f"[{'OK' if result.ok else 'FAIL'}] {result.name}: {result.message}"
        for result in results
    )


def _path_writable(name: str, path: Path) -> CheckResult:
    try:
        path.mkdir(parents=True, exist_ok=True)
        probe = path / ".preflight_write_test"
        probe.write_text("ok", encoding="utf-8")
        probe.unlink()
        return CheckResult(name, True, str(path))
    except Exception as exc:
        return CheckResult(name, False, str(exc))


def _live_credentials_check(cfg: AppConfig, force_required: bool = False) -> CheckResult:
    if cfg.trading.mode != "live" and not force_required:
        return CheckResult("live_credentials", True, "not required in paper mode")
    missing = missing_live_env_vars()
    if missing:
        return CheckResult("live_credentials", False, "missing " + ", ".join(missing))
    reason = (
        "required by executor preflight"
        if force_required and cfg.trading.mode != "live"
        else "required variables present"
    )
    return CheckResult("live_credentials", True, reason)


def _telegram_check(cfg: AppConfig) -> CheckResult:
    if not cfg.telegram.enabled:
        return CheckResult("telegram_config", True, "disabled")
    missing = [
        name
        for name in (cfg.telegram.bot_token_env, cfg.telegram.chat_id_env)
        if not os.getenv(name)
    ]
    if missing:
        return CheckResult("telegram_config", False, "missing " + ", ".join(missing))
    return CheckResult("telegram_config", True, "required variables present")


async def _chainlink_ws_check(
    url: str,
    asset: str,
    symbol: str,
    timeout_seconds: float,
) -> CheckResult:
    try:
        import websockets

        async with websockets.connect(url, ping_interval=None) as ws:
            await ws.send(
                json.dumps(
                    {
                        "action": "subscribe",
                        "subscriptions": [
                            {
                                "topic": "crypto_prices_chainlink",
                                "type": "update",
                                "filters": json.dumps({"symbol": symbol}),
                            }
                        ],
                    }
                )
            )
            deadline = asyncio.get_running_loop().time() + timeout_seconds
            while asyncio.get_running_loop().time() < deadline:
                try:
                    raw = await asyncio.wait_for(
                        ws.recv(),
                        timeout=max(0.1, deadline - asyncio.get_running_loop().time()),
                    )
                except TimeoutError:
                    break
                if not raw:
                    continue
                try:
                    message = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                payload = message.get("payload") or {}
                if str(payload.get("symbol") or "").lower() == symbol:
                    return CheckResult("chainlink_rtds", True, f"received {asset} tick/snapshot")
        return CheckResult("chainlink_rtds", False, f"no {asset} tick within {timeout_seconds}s")
    except Exception as exc:
        return CheckResult("chainlink_rtds", False, str(exc))


async def _clob_ws_check(url: str, token_ids: list[str], timeout_seconds: float) -> CheckResult:
    try:
        import websockets

        async with websockets.connect(url, ping_interval=None) as ws:
            await ws.send(
                json.dumps(
                    {
                        "assets_ids": token_ids,
                        "type": "market",
                        "custom_feature_enabled": True,
                    }
                )
            )
            empty_snapshots = 0
            deadline = asyncio.get_running_loop().time() + timeout_seconds
            while asyncio.get_running_loop().time() < deadline:
                try:
                    raw = await asyncio.wait_for(
                        ws.recv(),
                        timeout=max(0.1, deadline - asyncio.get_running_loop().time()),
                    )
                except TimeoutError:
                    break
                if not raw:
                    continue
                try:
                    message = json.loads(raw)
                except json.JSONDecodeError:
                    continue
                items: list[Any] = message if isinstance(message, list) else [message]
                if isinstance(message, list) and not message:
                    empty_snapshots += 1
                    continue
                if any(isinstance(item, dict) and item.get("event_type") == "book" for item in items):
                    return CheckResult("clob_market_ws", True, f"received book for {len(token_ids)} token(s)")
            if empty_snapshots:
                return CheckResult(
                    "clob_market_ws",
                    True,
                    f"subscription reachable; received {empty_snapshots} empty snapshot(s)",
                )
        return CheckResult("clob_market_ws", False, f"no book snapshot within {timeout_seconds}s")
    except Exception as exc:
        return CheckResult("clob_market_ws", False, str(exc))


async def _executor_dry_run_check(
    cfg: AppConfig,
    market: MarketWindow | None,
    timeout_seconds: float,
) -> CheckResult:
    if market is None:
        return CheckResult("executor_dry_run", False, "no active/open accepting-orders market from Gamma")
    started = time.perf_counter_ns()
    try:
        result = await asyncio.wait_for(
            asyncio.to_thread(live_executor_dry_run, cfg, market),
            timeout=timeout_seconds,
        )
    except TimeoutError:
        return CheckResult("executor_dry_run", False, f"timed out after {timeout_seconds}s; no order submitted")
    except Exception as exc:
        return CheckResult("executor_dry_run", False, f"{exc}; no order submitted")

    thread_wall_ms = (time.perf_counter_ns() - started) / 1_000_000.0
    return CheckResult(
        "executor_dry_run",
        result.ok,
        f"{result.message}; async_thread_wall_ms={thread_wall_ms:.3f}",
    )
