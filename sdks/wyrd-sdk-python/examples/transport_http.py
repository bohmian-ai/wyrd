"""Configure the HTTP fallback transport as JSON-compatible config.

Run with:
    python sdks/wyrd-sdk-python/examples/transport_http.py
"""

from __future__ import annotations

import json
from typing import Any


def secret_env(name: str) -> dict[str, str]:
    return {"source": "env", "name": name}


def secret_file(path: str) -> dict[str, str]:
    return {"source": "file", "path": path}


def without_none(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: without_none(item) for key, item in value.items() if item is not None}
    return value


def main() -> None:
    http = without_none(
        {
            "base_url": "https://wyrd-ingest.example.com",
            "timeout_ms": 30_000,
            "tls": {
                "ca_cert": secret_file("/etc/ssl/ca.pem"),
                "client_cert": None,
                "client_key": None,
                "server_name_override": None,
                "insecure_skip_verify": False,
            },
            "auth": secret_env("WYRD_API_KEY"),
            "compression": True,
        }
    )
    queue = {
        "transport": {"transport": "http", "params": http},
        "flush_max_rows": 10_000,
        "flush_interval_ms": 5_000,
        "channel_capacity": 100,
    }
    encoded = json.dumps(queue, indent=2, sort_keys=True)
    assert json.loads(encoded) == queue
    print(encoded)


if __name__ == "__main__":
    main()
