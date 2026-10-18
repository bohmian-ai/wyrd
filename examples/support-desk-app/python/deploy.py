"""Register the Cards in ``service/`` and download them into ``.wyrd-bundle``.

Registering the Service creates its ``vala.datasets.tickets`` table. The
script also turns on gateway call metadata capture, so every model call is
recorded with its Run.

    uv run python deploy.py
"""

from __future__ import annotations

import shutil

from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway import Gateway

from desk import BUNDLE, SERVICE


def deploy(client: WyrdClient | None = None) -> None:
    # ANCHOR: deploy
    cards = Cards(client)
    receipt = cards.declare_from_path(SERVICE / "support-desk.yaml")
    shutil.rmtree(BUNDLE, ignore_errors=True)
    cards.hydrate(receipt.root, BUNDLE)
    Gateway(client).put_capture_policy({"mode": "metadata", "payload_fields": []})
    print(f"registered {receipt.root} into {BUNDLE.name}")
    # ANCHOR_END: deploy


if __name__ == "__main__":
    deploy()
