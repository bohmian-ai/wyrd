"""The canonical support desk, run from the checked-in example.

The desk deploys, refuses a conflicting table and an under-privileged caller,
answers every request, earns 100 ``answer-quality`` passes and 90
``no-refund-promise`` passes with 10 failures, and explains a passing and a
failing request from joined evidence.
"""

from __future__ import annotations

import importlib.util
import json
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.bifrost import TableConfig
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .gateway.support import OPENAI_COMPLETION
from .support import FIXTURES, Delivery, Receiver
from .test_local_development import configure_gateway

pytestmark = pytest.mark.integration

EXAMPLE = Path(__file__).resolve().parents[4] / "examples/support-desk/python/support_desk.py"
_spec = importlib.util.spec_from_file_location("support_desk", EXAMPLE)
assert _spec is not None and _spec.loader is not None
desk_example = importlib.util.module_from_spec(_spec)
sys.modules["support_desk"] = desk_example
_spec.loader.exec_module(desk_example)

ANSWERS = (
    ("Grade the support answer", '{"passed":true}'),
    ("refund", "You have a guaranteed refund."),
    ("Answer the customer", "Your order is on its way."),
)
"""Upstream answers by the first marker the request body contains."""


class DeskUpstream(BaseHTTPRequestHandler):
    """Chat Completions upstream answering by body marker, recording each request."""

    upstream: Receiver

    def do_POST(self) -> None:
        raw = self.rfile.read(int(self.headers["content-length"])).decode()
        self.upstream.record(Delivery(self.path, dict(self.headers), json.loads(raw)))
        content = next(answer for marker, answer in ANSWERS if marker in raw)
        message = {**OPENAI_COMPLETION["choices"][0]["message"], "content": content}
        body = json.dumps(
            {
                **OPENAI_COMPLETION,
                "choices": [{**OPENAI_COMPLETION["choices"][0], "message": message}],
            }
        ).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format: str, *args: object) -> None:
        """Keep the per-request access log out of the test output."""


@pytest.mark.usefixtures("fresh_tracer_provider")
def test_support_desk_answers_verifies_and_explains_every_request(tmp_path: Path) -> None:
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), DeskUpstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    DeskUpstream.upstream = recorded
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        with WyrdTestServer(
            mutate_env=False, verification_runtime=True, provider_base_url=recorded.url
        ) as server:
            admin = WyrdClient(
                server_url=server.base_url,
                credential=server.tenant_admin_key(),
                grpc_url=server.grpc_url,
            )
            configure_gateway(admin)
            bundle = tmp_path / "bundle"

            desk = desk_example.deploy(admin, bundle)

            tickets = TableConfig.describe("vala.datasets.tickets", admin)
            assert tickets.arrow_schema.names[:4] == ["ticket_id", "question", "answer", "refund"]
            with pytest.raises(WyrdError) as conflict:
                Cards(admin).register_from_path(
                    FIXTURES / "cards/support_desk/conflicting-desk.yaml"
                )
            assert conflict.value.code == "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
            viewer = WyrdClient(
                server_url=server.base_url,
                credential=server.bootstrap_service(["viewer"], name="desk-viewer"),
                grpc_url=server.grpc_url,
            )
            with pytest.raises(WyrdError) as denied:
                WyrdState.from_path(bundle, viewer).run("agent").invoke(
                    {"question": "Where is order 1?"}
                )
            assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
            assert recorded.deliveries == [], "the refusal precedes upstream IO"

            served = desk_example.serve(desk)
            assert [request.passed for request in served] == [
                "refund" not in desk_example.question(index)
                for index in range(desk_example.REQUESTS)
            ]
            assert desk_example.wait_for_verdicts(admin, desk, timeout=180) == {
                "answer-quality": (100, 0),
                "no-refund-promise": (90, 10),
            }

            agent = desk.uid("support-agent")
            for index, realtime in ((1, "passed"), (0, "failed")):
                explained = desk_example.explain(admin, served[index].run_id)
                assert explained["run_id"] == served[index].run_id
                assert explained["ticket_id"] == f"T-{index}"
                assert explained["span"] == "support-desk.request"
                assert explained["call_card_uid"] == agent
                assert explained["continuous"] == "passed"
                assert explained["realtime"] == realtime, explained
    finally:
        upstream.shutdown()
