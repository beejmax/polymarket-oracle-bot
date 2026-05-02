from __future__ import annotations

import time


TIMEFRAME_SECONDS = {
    "5m": 5 * 60,
    "15m": 15 * 60,
    "1h": 60 * 60,
}


def timeframe_seconds(timeframe: str) -> int:
    try:
        return TIMEFRAME_SECONDS[timeframe]
    except KeyError as exc:
        raise ValueError(f"unsupported timeframe: {timeframe}") from exc


def floor_window_start(ts: int | None = None, timeframe: str = "15m") -> int:
    current = int(time.time()) if ts is None else int(ts)
    seconds = timeframe_seconds(timeframe)
    return current - (current % seconds)


def candidate_window_starts(
    ts: int | None = None,
    timeframe: str = "15m",
    lookback: int = 1,
    lookahead: int = 2,
) -> list[int]:
    base = floor_window_start(ts, timeframe)
    seconds = timeframe_seconds(timeframe)
    return [base + offset * seconds for offset in range(-lookback, lookahead + 1)]


def slug_for(slug_prefix: str, start_ts: int, timeframe: str = "15m") -> str:
    return f"{slug_prefix}-updown-{timeframe}-{int(start_ts)}"

