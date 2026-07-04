"""Stdlib HTTP mock for the Vala eval pull protocol."""

from __future__ import annotations

import json
import threading
from collections.abc import Iterator
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Any

TEST_RUN_ID = "01900000-0000-7000-8000-000000000001"
TEST_LEASE = "test-lease-abc-123"


class ProtocolMock:
    """In-process scripted mock of the eval protocol router."""

    def __init__(self, script: list[dict[str, Any]]):
        self._script: Iterator[dict[str, Any]] = iter(script)
        self.received_agent_turns: list[dict[str, Any]] = []
        self.received_user_turns: list[dict[str, Any]] = []
        self.received_open: dict[str, Any] | None = None
        self.received_auth_headers: list[tuple[str, str | None]] = []
        self.enforce_lease = False

    def next_directive(self) -> dict[str, Any]:
        """Return the next scripted directive."""
        return next(self._script)


def serve(mock: ProtocolMock) -> tuple[str, HTTPServer, threading.Thread]:
    """Run the mock server on a free localhost port."""

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
            pass

        def _read_json(self) -> dict[str, Any]:
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length)
            if not raw:
                return {}
            return json.loads(raw)

        def _write_json(self, status: int, body: dict[str, Any]) -> None:
            payload = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def _check_lease(self) -> bool:
            auth = self.headers.get("Authorization")
            mock.received_auth_headers.append((self.path, auth))
            if not mock.enforce_lease:
                return True
            if auth != f"Bearer {TEST_LEASE}":
                self._write_json(401, {"code": "WYRD_EVAL_401_MISSING_LEASE"})
                return False
            return True

        def do_POST(self) -> None:  # noqa: N802
            body = self._read_json()
            if self.path == "/v1/eval/runs":
                mock.received_open = body
                self._write_json(200, {"run_id": TEST_RUN_ID, "lease_token": TEST_LEASE})
                return
            if self.path == f"/v1/eval/runs/{TEST_RUN_ID}/next":
                if self._check_lease():
                    self._write_json(200, mock.next_directive())
                return
            if self.path == f"/v1/eval/runs/{TEST_RUN_ID}/agent-turn":
                if self._check_lease():
                    mock.received_agent_turns.append(body)
                    self._write_json(202, {})
                return
            if self.path == f"/v1/eval/runs/{TEST_RUN_ID}/user-turn":
                if self._check_lease():
                    mock.received_user_turns.append(body)
                    self._write_json(202, {})
                return
            self._write_json(404, {"code": "WYRD_SPEC_404_NOT_FOUND"})

    server = HTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return f"http://127.0.0.1:{server.server_port}", server, thread
