"""Tune queue policy as JSON-compatible config.

Run with:
    python python/py-wyrd/examples/transport_queue_policy.py
"""

from __future__ import annotations

import json


def main() -> None:
    queue = {
        "transport": {
            "transport": "mock",
            "params": {
                "label": "queue-policy-demo",
                "fail_on_flush": None,
            },
        },
        "flush_max_rows": 5_000,
        "flush_interval_ms": 1_000,
        "channel_capacity": 500,
        "sample_ratio": 0.5,
    }
    encoded = json.dumps(queue, indent=2, sort_keys=True)
    assert json.loads(encoded) == queue
    print(encoded)


if __name__ == "__main__":
    main()
