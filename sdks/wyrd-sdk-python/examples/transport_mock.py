"""Configure the in-memory mock transport as JSON-compatible config.

Run with:
    python sdks/wyrd-sdk-python/examples/transport_mock.py
"""

from __future__ import annotations

import json


def main() -> None:
    transport = {
        "transport": "mock",
        "params": {"label": "demo", "fail_on_drain": None},
    }
    encoded = json.dumps(transport, indent=2, sort_keys=True)
    assert json.loads(encoded) == transport
    print(encoded)


if __name__ == "__main__":
    main()
