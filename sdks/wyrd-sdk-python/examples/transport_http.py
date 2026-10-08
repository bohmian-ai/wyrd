"""Configure the HTTP transport as JSON-compatible config.

Run with:
    python sdks/wyrd-sdk-python/examples/transport_http.py
"""

from __future__ import annotations

import json


def main() -> None:
    transport = {
        "transport": "http",
        "params": {
            "base_url": "https://wyrd-ingest.example.com",
            "timeout_ms": 30_000,
            "compression": True,
        },
    }
    encoded = json.dumps(transport, indent=2, sort_keys=True)
    assert json.loads(encoded) == transport
    print(encoded)


if __name__ == "__main__":
    main()
