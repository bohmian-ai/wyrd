"""Admin SDK client round-trips and error surfaces over a loopback server."""

from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Any
from urllib.parse import parse_qs, urlsplit

import pytest
from wyrd import WyrdError
from wyrd.client import WyrdClient


class _AdminMock:
    """Scripted admin router. Records requests; replies by method + path prefix."""

    def __init__(self) -> None:
        self.requests: list[dict[str, Any]] = []
        self._routes: dict[tuple[str, str], tuple[int, Any]] = {}

    def route(self, method: str, prefix: str, status: int, body: Any) -> None:
        """Register a reply for requests whose path starts with ``prefix``."""
        self._routes[(method, prefix)] = (status, body)

    def resolve(self, method: str, path: str) -> tuple[int, Any]:
        """Return the reply for the longest matching prefix, else 404."""
        candidates = [
            (prefix, reply)
            for (m, prefix), reply in self._routes.items()
            if m == method and path.startswith(prefix)
        ]
        if not candidates:
            return 404, {"code": "WYRD_SPEC_404_NOT_FOUND"}
        _, reply = max(candidates, key=lambda item: len(item[0]))
        return reply


def _serve(mock: _AdminMock) -> tuple[str, HTTPServer]:
    """Run ``mock`` on a free localhost port; returns base URL and server."""

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
            pass

        def _record(self, method: str) -> None:
            length = int(self.headers.get("Content-Length", "0"))
            raw = self.rfile.read(length) if length else b""
            split = urlsplit(self.path)
            mock.requests.append(
                {
                    "method": method,
                    "path": split.path,
                    "query": parse_qs(split.query),
                    "body": json.loads(raw) if raw else None,
                    "access_token": self.headers.get("x-wyrd-access-token"),
                }
            )
            status, body = mock.resolve(method, split.path)
            payload = b"" if body is None else json.dumps(body).encode()
            self.send_response(status)
            if payload:
                self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self) -> None:  # noqa: N802
            self._record("POST")

        def do_GET(self) -> None:  # noqa: N802
            self._record("GET")

        def do_DELETE(self) -> None:  # noqa: N802
            self._record("DELETE")

    server = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return f"http://127.0.0.1:{server.server_port}", server


def _trusted_issuer_view() -> dict[str, Any]:
    return {
        "issuer": "https://idp.example.com/",
        "jwks_uri": "https://idp.example.com/jwks",
        "expected_audience": "wyrd",
        "client_id": "client-123",
        "client_auth": "SecretBasic",
        "principal_kind": "Workload",
        "jwks_ttl_secs": 3600,
        "claim_mapping": {"subject": "sub"},
        "group_role_map": {},
        "default_roles": [],
    }


def _trusted_issuer_request() -> dict[str, Any]:
    return {
        "issuer": "https://idp.example.com/",
        "expected_audience": "wyrd",
        "client_id": "client-123",
        "client_auth": "SecretBasic",
        "client_secret": "s3cr3t",
        "claim_mapping": {"subject": "sub"},
        "principal_kind": "Workload",
    }


def _start(mock: _AdminMock) -> tuple[WyrdClient, _AdminMock]:
    base_url, server = _serve(mock)
    import os

    os.environ["WYRD_ACCESS_TOKEN"] = "admin-bearer"
    try:
        client = WyrdClient(base_url=base_url)
    finally:
        os.environ.pop("WYRD_ACCESS_TOKEN", None)
    mock._server = server  # type: ignore[attr-defined]
    return client, mock


def test_create_trusted_issuer_sends_secret_and_returns_redacted_view():
    mock = _AdminMock()
    mock.route("POST", "/admin/trusted-issuers", 200, _trusted_issuer_view())
    client, mock = _start(mock)
    try:
        view = client.admin.trusted_issuers.create(_trusted_issuer_request())
    finally:
        mock._server.shutdown()

    sent = mock.requests[0]["body"]
    assert sent["client_secret"] == "s3cr3t"
    assert mock.requests[0]["access_token"] == "Bearer admin-bearer"
    assert "client_secret" not in view
    assert view["issuer"] == "https://idp.example.com/"


def test_list_trusted_issuers_returns_views():
    mock = _AdminMock()
    mock.route("GET", "/admin/trusted-issuers", 200, [_trusted_issuer_view()])
    client, mock = _start(mock)
    try:
        views = client.admin.trusted_issuers.list()
    finally:
        mock._server.shutdown()

    assert isinstance(views, list)
    assert views[0]["client_id"] == "client-123"


def test_delete_trusted_issuer_sends_issuer_and_cascade_query():
    mock = _AdminMock()
    mock.route("DELETE", "/admin/trusted-issuers", 204, None)
    client, mock = _start(mock)
    try:
        result = client.admin.trusted_issuers.delete("https://idp.example.com/", cascade=True)
    finally:
        mock._server.shutdown()

    assert result is None
    query = mock.requests[0]["query"]
    assert query["issuer"] == ["https://idp.example.com/"]
    assert query["cascade"] == ["true"]


def test_create_conflict_surfaces_admin_conflict_code():
    mock = _AdminMock()
    mock.route(
        "POST",
        "/admin/trusted-issuers",
        409,
        {"code": "WYRD_AUTH_409_ADMIN_CONFLICT", "detail": "issuer already exists"},
    )
    client, mock = _start(mock)
    try:
        with pytest.raises(WyrdError) as excinfo:
            client.admin.trusted_issuers.create(_trusted_issuer_request())
    finally:
        mock._server.shutdown()

    assert excinfo.value.code == "WYRD_AUTH_409_ADMIN_CONFLICT"


def test_delete_missing_binding_surfaces_admin_not_found_code():
    mock = _AdminMock()
    mock.route(
        "DELETE",
        "/admin/workload-bindings",
        404,
        {"code": "WYRD_AUTH_404_ADMIN_NOT_FOUND", "detail": "binding not found"},
    )
    client, mock = _start(mock)
    try:
        with pytest.raises(WyrdError) as excinfo:
            client.admin.workload_bindings.delete("https://idp.example.com/", "sub-123")
    finally:
        mock._server.shutdown()

    assert excinfo.value.code == "WYRD_AUTH_404_ADMIN_NOT_FOUND"


def test_workload_binding_create_and_list_round_trip():
    view = {
        "issuer": "https://idp.example.com/",
        "subject": "sub-123",
        "audience": None,
        "card_ref": {
            "kind": "Service",
            "name": "billing",
            "version": "1.0.0",
            "space": "team",
        },
    }
    mock = _AdminMock()
    mock.route("POST", "/admin/workload-bindings", 200, view)
    mock.route("GET", "/admin/workload-bindings", 200, [view])
    client, mock = _start(mock)
    try:
        created = client.admin.workload_bindings.create(
            {
                "issuer": "https://idp.example.com/",
                "subject": "sub-123",
                "card_ref": {
                    "kind": "Service",
                    "name": "billing",
                    "version": "1.0.0",
                    "space": "team",
                },
            }
        )
        listed = client.admin.workload_bindings.list(subject="sub-123")
    finally:
        mock._server.shutdown()

    assert created["subject"] == "sub-123"
    assert listed[0]["card_ref"]["name"] == "billing"
    list_request = next(r for r in mock.requests if r["method"] == "GET")
    assert list_request["query"]["subject"] == ["sub-123"]
