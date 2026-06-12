"""Configure mTLS and bearer-token secrets as JSON-compatible config.

Run with:
    python python/py-wyrd/examples/transport_secrets.py
"""

from __future__ import annotations

import json


def secret_file(path: str) -> dict[str, str]:
    return {"source": "file", "path": path}


def secret_vault(key: str) -> dict[str, str]:
    return {"source": "vault", "key": key}


def main() -> None:
    grpc = {
        "endpoint": "https://wyrd-ingest.internal:50051",
        "timeout_ms": 30_000,
        "tls": {
            "ca_cert": secret_file("/var/run/secrets/wyrd/ca.pem"),
            "client_cert": secret_file("/var/run/secrets/wyrd/client.crt"),
            "client_key": secret_file("/var/run/secrets/wyrd/client.key"),
            "server_name_override": "ingest.internal",
            "insecure_skip_verify": False,
        },
        "auth": secret_vault("secret/data/wyrd/api"),
        "connect_retries": 3,
    }
    queue = {
        "transport": {"transport": "grpc", "params": grpc},
        "flush_max_rows": 10_000,
        "flush_interval_ms": 5_000,
        "channel_capacity": 100,
    }
    encoded = json.dumps(queue, indent=2, sort_keys=True)
    assert json.loads(encoded) == queue
    print(encoded)


if __name__ == "__main__":
    main()
