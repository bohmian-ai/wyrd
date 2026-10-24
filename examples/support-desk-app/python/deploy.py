"""Declare the Cards in ``service/`` and download them into ``.wyrd-bundle``.

Registering the Service creates its ``bifrost.datasets.tickets`` table. A
repeat deploy replaces the bundle only once the new pull is complete, so a
refused pull keeps the previous one. The script also turns on gateway call
metadata capture, so every model call is recorded with its Run.

    uv run python deploy.py
"""

from __future__ import annotations

from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway

from desk import BUNDLE, SERVICE


def deploy(client: WyrdClient | None = None) -> None:
    # ANCHOR: deploy
    cards = Cards(client)
    receipt = cards.declare_from_path(SERVICE / "support-desk.yaml")
    cards.pull(receipt.root, BUNDLE)
    Gateway(client).put_capture_policy({"mode": "metadata", "payload_fields": []})
    print(f"declared {receipt.root} into {BUNDLE.name}")
    # ANCHOR_END: deploy


if __name__ == "__main__":
    deploy()
