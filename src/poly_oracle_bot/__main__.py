from __future__ import annotations

import argparse
import asyncio
from pathlib import Path

from .bot import Bot
from .config import load_config


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Polymarket 15m oracle bot")
    parser.add_argument("--config", type=Path, default=Path("config.toml"))
    parser.add_argument("--db", type=Path, default=Path("data/bot.sqlite3"))
    parser.add_argument("--no-dashboard", action="store_true")
    return parser


def main() -> None:
    args = build_parser().parse_args()
    cfg = load_config(args.config if args.config.exists() else None)
    if args.no_dashboard:
        cfg.trading.dashboard_interval_seconds = 0.0
    asyncio.run(Bot(cfg, args.db).run())


if __name__ == "__main__":
    main()

