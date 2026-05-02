from __future__ import annotations

from typing import Any

from .config import TelegramConfig


class TelegramNotifier:
    def __init__(self, cfg: TelegramConfig) -> None:
        self.cfg = cfg

    @property
    def enabled(self) -> bool:
        return bool(self.cfg.enabled and self.cfg.bot_token and self.cfg.chat_id)

    async def send(self, text: str) -> None:
        if not self.enabled:
            return
        import httpx

        url = f"https://api.telegram.org/bot{self.cfg.bot_token}/sendMessage"
        payload: dict[str, Any] = {
            "chat_id": self.cfg.chat_id,
            "text": text,
            "disable_web_page_preview": True,
        }
        async with httpx.AsyncClient(timeout=5.0) as client:
            response = await client.post(url, json=payload)
            response.raise_for_status()

