#!/usr/bin/env python3
from __future__ import annotations

import argparse
import statistics
import time

from poly_oracle_bot.config import AppConfig
from poly_oracle_bot.models import MarketWindow, PriceTick
from poly_oracle_bot.orderbook import OrderBookState
from poly_oracle_bot.risk import RiskManager
from poly_oracle_bot.signal import SignalEngine


def percentile(values: list[float], pct: float) -> float:
    ordered = sorted(values)
    idx = (len(ordered) - 1) * pct
    low = int(idx)
    high = min(low + 1, len(ordered) - 1)
    weight = idx - low
    return ordered[low] * (1.0 - weight) + ordered[high] * weight


def main() -> None:
    parser = argparse.ArgumentParser(description="Benchmark local Python signal hot path")
    parser.add_argument("--iterations", type=int, default=200_000)
    parser.add_argument("--sample-every", type=int, default=1)
    args = parser.parse_args()

    cfg = AppConfig()
    market = MarketWindow(
        asset="BTC",
        slug="btc-updown-15m-1777738500",
        event_id="1",
        market_id="2",
        condition_id="0xabc",
        start_ts=1_777_738_500,
        end_ts=1_777_739_400,
        tokens={"Up": "up-token", "Down": "down-token"},
        tick_size=0.01,
        min_order_size=5.0,
        neg_risk=False,
        active=True,
        closed=False,
        accepting_orders=True,
        price_to_beat=100.0,
    )
    tick = PriceTick(
        asset="BTC",
        symbol="btc/usd",
        price=101.0,
        feed_ts_ms=1_777_738_510_000,
        received_ts_ms=1_777_738_510_001,
    )
    book_msg = {
        "event_type": "book",
        "asset_id": "up-token",
        "bids": [{"price": "0.52", "size": "100"}, {"price": "0.53", "size": "40"}],
        "asks": [{"price": "0.56", "size": "50"}, {"price": "0.55", "size": "20"}],
        "timestamp": "1777738510000",
    }

    orderbook = OrderBookState()
    signal_engine = SignalEngine(cfg.risk)
    risk = RiskManager(cfg.risk)
    now_ms = 1_777_738_510_000
    samples_us: list[float] = []
    accepted = 0

    # Warmup.
    for _ in range(1_000):
        orderbook.update_from_message(book_msg)
        quote = orderbook.quote("up-token")
        signal = signal_engine.evaluate(market, tick, quote, now_ms=now_ms) if quote else None
        if signal and signal.reason == "accepted":
            risk.size_for_signal(signal, market)

    started = time.perf_counter_ns()
    for index in range(args.iterations):
        sample = index % args.sample_every == 0
        before = time.perf_counter_ns() if sample else 0
        orderbook.update_from_message(book_msg)
        quote = orderbook.quote("up-token")
        signal = signal_engine.evaluate(market, tick, quote, now_ms=now_ms) if quote else None
        if signal and signal.reason == "accepted":
            risk.size_for_signal(signal, market)
            accepted += 1
        if sample:
            samples_us.append((time.perf_counter_ns() - before) / 1_000.0)
    total_ms = (time.perf_counter_ns() - started) / 1_000_000.0

    print(f"runtime=python")
    print(f"iterations={args.iterations}")
    print(f"accepted={accepted}")
    print(f"total_ms={total_ms:.3f}")
    print(f"throughput_ops_per_sec={args.iterations / (total_ms / 1000.0):.3f}")
    print(f"mean_us={statistics.fmean(samples_us):.3f}")
    print(f"p50_us={percentile(samples_us, 0.50):.3f}")
    print(f"p90_us={percentile(samples_us, 0.90):.3f}")
    print(f"p99_us={percentile(samples_us, 0.99):.3f}")
    print(f"max_us={max(samples_us):.3f}")


if __name__ == "__main__":
    main()

