from __future__ import annotations

import os
import time
from dataclasses import dataclass

from .models import MarketWindow, Position, PriceTick, Quote


@dataclass(slots=True)
class DashboardSnapshot:
    ticks: dict[str, PriceTick]
    markets: dict[str, MarketWindow]
    quotes: dict[str, Quote]
    positions: dict[str, Position]
    realized_pnl_today: float
    drawdown_blocked: bool


class Dashboard:
    def render(self, snapshot: DashboardSnapshot) -> None:
        lines: list[str] = []
        lines.append("Polymarket Oracle Bot")
        lines.append(time.strftime("%Y-%m-%d %H:%M:%S %Z"))
        status = "BLOCKED" if snapshot.drawdown_blocked else "OK"
        lines.append(f"Risk: {status} | Realized today: {snapshot.realized_pnl_today:.2f}")
        lines.append("")
        lines.append("Prices")
        for asset in sorted(snapshot.ticks):
            tick = snapshot.ticks[asset]
            age = max(0.0, (time.time() * 1000 - tick.received_ts_ms) / 1000.0)
            lines.append(f"  {asset:>4} {tick.price:>12.6f} age={age:>5.1f}s")
        lines.append("")
        lines.append("Markets")
        for market in sorted(snapshot.markets.values(), key=lambda m: (m.asset, m.start_ts)):
            if market.closed:
                continue
            up_quote = snapshot.quotes.get(market.tokens["Up"])
            down_quote = snapshot.quotes.get(market.tokens["Down"])
            up = _fmt_ask(up_quote)
            down = _fmt_ask(down_quote)
            ptb = "n/a" if market.price_to_beat is None else f"{market.price_to_beat:.6f}"
            lines.append(
                f"  {market.asset:>4} {market.slug} ptb={ptb} up_ask={up} down_ask={down}"
            )
        lines.append("")
        lines.append("Open Positions")
        if not snapshot.positions:
            lines.append("  none")
        for pos in snapshot.positions.values():
            lines.append(
                f"  {pos.trade_id[:8]} {pos.asset} {pos.outcome} "
                f"shares={pos.shares:.4f} entry={pos.entry_price:.3f} cost={pos.cost_usd:.2f}"
            )

        os.system("clear")
        print("\n".join(lines), flush=True)


def _fmt_ask(quote: Quote | None) -> str:
    if quote is None or quote.best_ask is None:
        return "n/a"
    return f"{quote.best_ask:.3f}"

