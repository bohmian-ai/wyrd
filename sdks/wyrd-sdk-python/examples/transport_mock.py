"""Configure the in-memory mock transport as JSON-compatible config.

Run with:
    python sdks/wyrd-sdk-python/examples/transport_mock.py
"""

from __future__ import annotations

import json


def main() -> None:
    mock = {
        "label": "demo",
        "fail_on_flush": None,
    }
    queue = {
        "transport": {"transport": "mock", "params": mock},
        "flush_max_rows": 100,
        "flush_interval_ms": 100,
        "channel_capacity": 10,
    }
    encoded = json.dumps(queue, indent=2, sort_keys=True)
    assert json.loads(encoded) == queue
    print(encoded)


if __name__ == "__main__":
    main()
