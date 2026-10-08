"""Configure the default gRPC transport as JSON-compatible config.

Run with:
    python sdks/wyrd-sdk-python/examples/transport_grpc.py
"""

from __future__ import annotations

import json


def main() -> None:
    transport = {
        "transport": "grpc",
        "params": {
            "endpoint": "https://wyrd-ingest.example.com:50051",
            "timeout_ms": 30_000,
            "connect_retries": 3,
            "keepalive_interval_ms": 20_000,
            "keepalive_timeout_ms": 5_000,
            "max_message_bytes": 4 * 1024 * 1024,
        },
    }
    encoded = json.dumps(transport, indent=2, sort_keys=True)
    assert json.loads(encoded) == transport
    print(encoded)


if __name__ == "__main__":
    main()
