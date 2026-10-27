"""The support desk, run from the checked-in examples.

The canonical desk deploys, refuses a conflicting table and an
under-privileged caller, fails a refund promise immediately, answers every
request, resumes each background ``policy-check`` by ID to 90 passes and 10
refund-promise failures that the judge, citing the policy, still passes, and
explains a passing and a failing request from joined evidence.

The published quickstart runs ``examples/support-desk-app`` exactly as the
docs page does: its gateway document, local check, deploy script, HTTP routes,
and evidence query, against a local model provider.
"""

from __future__ import annotations

import importlib
import importlib.util
import json
import shutil
import subprocess
import sys
import threading
import time
from collections.abc import Generator
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from types import ModuleType
from typing import Any

import openai
import pytest
from wyrd import WyrdError
from wyrd.bifrost import TableConfig
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.gateway_admin import Gateway
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from ..agent_readiness.observe import Observer
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
    (
        "list its refund rules",
        '{"findings":[{"rule":"Refunds of up to 100 dollars are approved within 30 days.",'
        '"page":1}]}',
    ),
    (
        "Claim:",
        '{"outcome":"passed","reason":"consistent with the policy",'
        '"citations":[{"slot":"policy","locator":{"page":1}}]}',
    ),
    ("refund", "You have a guaranteed refund."),
    ("Answer the customer", "Your order is on its way."),
)
"""Upstream answers by the first marker the request body contains."""


class DeskUpstream(BaseHTTPRequestHandler):
    """Chat Completions upstream answering by body marker, recording each request."""

    upstream: Receiver
    answers: tuple[tuple[str, str], ...] = ANSWERS

    def do_POST(self) -> None:
        raw = self.rfile.read(int(self.headers["content-length"])).decode()
        self.upstream.record(Delivery(self.path, dict(self.headers), json.loads(raw)))
        content = next(answer for marker, answer in self.answers if marker in raw)
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


@contextmanager
def upstream_answering(answers: tuple[tuple[str, str], ...]) -> Generator[Receiver]:
    """Serve a recording local model provider that answers by body marker."""
    handler = type("Answering", (DeskUpstream,), {"answers": answers})
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    handler.upstream = recorded
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        yield recorded
    finally:
        upstream.shutdown()


@pytest.mark.usefixtures("fresh_telemetry_providers")
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
            Gateway(admin).put_deployment(
                {
                    "name": "anthropic",
                    "adapter": "anthropic",
                    "auth": "none",
                }
            )
            with pytest.raises(
                LookupError, match="configure a gateway provider deployment for openai/gpt-4o"
            ):
                desk_example.deploy(admin, tmp_path / "refused")
            configure_gateway(admin)
            bundle = tmp_path / "bundle"

            desk = desk_example.deploy(admin, bundle)

            tickets = TableConfig.describe("bifrost.datasets.tickets", admin)
            assert tickets.arrow_schema.names[:4] == ["ticket_id", "question", "answer", "refund"]
            with pytest.raises(WyrdError) as conflict:
                Cards(admin).declare_from_path(
                    FIXTURES / "cards/support_desk/conflicting-desk.yaml"
                )
            assert conflict.value.code == "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
            viewer = WyrdClient(
                server_url=server.base_url,
                credential=server.bootstrap_service(["viewer"], name="desk-viewer"),
                grpc_url=server.grpc_url,
            )
            viewing = WyrdState.from_path(bundle, viewer)
            with pytest.raises(openai.PermissionDeniedError) as denied:
                desk_example.answer(
                    desk_example.gateway_client(viewer),
                    viewing,
                    viewing.run("agent"),
                    "Where is order 1?",
                )
            assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
            assert recorded.deliveries == [], "the refusal precedes upstream IO"

            promise = desk_example.check(desk, "You have a guaranteed refund.")
            assert promise.verdict == "failed"
            assert [(each["id"], each["outcome"]) for each in promise.requirements] == [
                ("no-refund-promise", "failed"),
                ("follows-policy", "passed"),
            ]
            assert promise.requirements[1]["citations"] == [
                {"slot": "policy", "locator": {"page": 1}}
            ]

            served = desk_example.serve(desk)
            assert len(served) == desk_example.REQUESTS
            leaked = {
                name
                for delivery in recorded.deliveries
                for name in delivery.headers
                if name.lower().startswith(("wyrd-", "x-wyrd"))
            }
            assert leaked == set(), "Wyrd correlation and credentials stay out of the upstream"
            results = desk_example.wait_for_verdicts(admin, served, timeout=180)
            for index, result in enumerate(results):
                refund = "refund" in desk_example.question(index)
                assert result.verdict == ("failed" if refund else "passed"), index
                assert result.run_id == served[index].run_id, index

            agent = desk.uid("support-agent")
            for index, verdict in ((1, "passed"), (0, "failed")):
                explained = desk_example.explain(admin, served[index].run_id)
                assert explained["run_id"] == served[index].run_id
                assert explained["ticket_id"] == f"T-{index}"
                assert explained["span"] == "support-desk.request"
                assert explained["call_card_uid"] == agent
                assert explained["verdict"] == verdict, explained
    finally:
        upstream.shutdown()


QUICKSTART = Path(__file__).resolve().parents[4] / "examples/support-desk-app"
"""The app the published quickstart embeds and runs."""

LETTER = "Dear customer,\n\nYour item arrived damaged.\nWe approve a refund of 40 dollars.\nIt reaches your card within 30 days.\n"
"""The refund decision letter the local model writes for ``POST /reviews``."""

QUICKSTART_ANSWERS = (
    (
        "Claim:",
        '{"outcome":"passed","reason":"a 40 dollar refund within 30 days is approved",'
        '"citations":[{"slot":"letter","locator":{"lines":{"start":4,"end":5}}}]}',
    ),
    ("refund decision letter", LETTER),
    ("Answer the customer", "Your order A-1001 ships tomorrow."),
)
"""Local model answers: the policy judge, the letter writer, then the support agent."""

GATEWAY = """{
  "credentials": [{"name": "openai-key", "provider": "openai", "source": {"local_env": "OPENAI_API_KEY"}}],
  "deployments": [{"name": "openai", "adapter": "openai", "auth": {"bearer": {"credential": "openai-key"}}}]
}"""
"""The quickstart's step 2 gateway document, verbatim."""


def quickstart_modules(python: Path) -> dict[str, ModuleType]:
    """Import the copied app's modules by the names its own imports use."""
    names = ("desk", "deploy", "app", "evidence")
    sys.path.insert(0, str(python))
    try:
        for name in names:
            sys.modules.pop(name, None)
        return {name: importlib.import_module(name) for name in names}
    finally:
        sys.path.remove(str(python))
        for name in names:
            sys.modules.pop(name, None)


def wyrd(*arguments: str, stdin: str | None = None) -> dict[str, Any]:
    """Run the installed ``wyrd`` CLI and return the one JSON result it prints."""
    wyrd_cli = shutil.which("wyrd")
    assert wyrd_cli, "the wyrd-ai package installs the wyrd CLI"
    done = subprocess.run(
        [wyrd_cli, *arguments],
        input=stdin,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert done.returncode == 0, done.stderr
    return json.loads(done.stdout)


def poll(read: Any, done: Any, timeout: float = 120.0) -> Any:
    """Call ``read`` until ``done`` accepts its value, within ``timeout`` seconds."""
    deadline = time.monotonic() + timeout
    while True:
        value = read()
        if done(value):
            return value
        assert time.monotonic() < deadline, f"not done within {timeout} s: {value}"
        time.sleep(0.5)


@pytest.mark.usefixtures("fresh_telemetry_providers")
def test_published_quickstart_declares_observes_and_verifies(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from fastapi.testclient import TestClient

    shutil.copytree(QUICKSTART, tmp_path / "app", ignore=shutil.ignore_patterns(".wyrd-bundle"))
    root = tmp_path / "app"
    with (
        upstream_answering(QUICKSTART_ANSWERS) as recorded,
        WyrdTestServer(
            mutate_env=False, verification_runtime=True, provider_base_url=recorded.url
        ) as server,
    ):
        # The app and CLI resolve the server the way setup leaves it saved.
        monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
        monkeypatch.setenv("WYRD_SERVER_URL", server.base_url)
        monkeypatch.setenv("WYRD_GRPC_URL", server.grpc_url)
        monkeypatch.setenv("WYRD_API_KEY", server.tenant_admin_key())
        monkeypatch.setenv("OPENAI_API_KEY", "sk-local-upstream")
        monkeypatch.delenv("RUST_LOG", raising=False)

        # 2. Connect the model.
        gateway = wyrd("gateway", "deployment", "put", "--file", "-", stdin=GATEWAY)
        assert "sk-local-upstream" not in json.dumps(gateway)

        # 3. Read what is declared: the local check reports all seven Cards.
        checked = wyrd("declare", str(root / "service"), "--check", "--format", "json")
        assert checked["ok"] is True, checked
        assert sorted(card["name"] for card in checked["cards"]) == [
            "follows-policy",
            "no-refund-promise",
            "policy-judge",
            "support-agent",
            "support-desk",
            "support-prompt",
            "ticket-complete",
        ]

        modules = quickstart_modules(root / "python")
        viewer = WyrdClient(
            server_url=server.base_url,
            credential=server.bootstrap_service(["viewer"], name="quickstart-viewer"),
            grpc_url=server.grpc_url,
        )
        with pytest.raises(WyrdError) as denied:
            modules["deploy"].deploy(viewer)
        assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"

        # 4. Deploy the Cards.
        modules["deploy"].deploy()
        assert (root / ".wyrd-bundle/metadata.yaml").is_file()

        with TestClient(modules["app"].app) as http:
            # 5. Ask a question: both model-free checks judge it before the reply.
            asked = http.post("/ask", json={"question": "Where is my order?", "order_id": "A-1001"})
            assert asked.status_code == 200, asked.text
            reply = asked.json()
            assert reply["answer"] == "Your order A-1001 ships tomorrow."
            assert {name: check["verdict"] for name, check in reply["checks"].items()} == {
                "no-refund-promise": "passed",
                "ticket-complete": "passed",
            }
            run_id = reply["run_id"]

            missing = http.post("/ask", json={"question": "Where is my order?"}).json()
            incomplete = missing["checks"]["ticket-complete"]
            assert incomplete["verdict"] == "failed"
            assert incomplete["requirements"][0]["reason"] == "missing order_id"

            desk = modules["app"].app.state.desk
            for verifier, inputs, code in (
                ("not-bound", {"answer": "x"}, "WYRD_SDK_404_UNKNOWN_VERIFIER"),
                ("no-refund-promise", {"letter": "x"}, "WYRD_VERIFICATION_400_INPUT_INVALID"),
            ):
                with pytest.raises(WyrdError) as refused:
                    desk.state.run().observe.verify(verifier, **inputs)
                assert refused.value.code == code

            # 6. Queue a policy review and resume it by execution ID.
            queued = http.post(
                "/reviews", json={"question": "Can I get a refund on a damaged item?"}
            ).json()
            assert queued["letter"] == LETTER
            seen: list[Any] = []

            def read_review() -> Any:
                state = http.get(f"/reviews/{queued['execution_id']}").json()
                seen.append(state)
                return state

            review = poll(
                read_review,
                lambda state: (
                    state.get("status") in {"completed", "errored", "timed_out"}
                    and (state["status"] != "completed" or "verdict" in state)
                ),
            )
            # A just-queued ID can read as not found until the queue accepts it.
            for state in seen[:-1]:
                assert state.get("status") in {"pending", "running"} or state.get("code") == (
                    "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"
                ), state
            assert review["status"] == "completed", review
            assert review["verdict"] == "passed"
            assert review["requirements"][0]["citations"] == [
                {"slot": "letter", "locator": {"lines": {"start": 4, "end": 5}}}
            ]
        # Leaving the app flushed its buffered tickets and spans.

        # 7. Query the evidence.
        server.flush_bifrost()
        rows = poll(
            lambda: modules["evidence"].evidence(run_id),
            lambda rows: {row["verdict"] for row in rows} == {"passed"} and len(rows) >= 2,
        )
        assert {row["run_id"] for row in rows} == {run_id}
        assert all(row["call_id"] and row["trace_id"] for row in rows)
        assert len({row["execution_id"] for row in rows}) == 2

        # The readiness observer accepts the app from server evidence alone:
        # the queued review by its completed status, the direct checks by
        # their results, and another tenant's client sees none of it.
        # The shared test tenant keeps earlier runs' rows, so only this run's
        # Run and execution identify it.
        observer = Observer(WyrdClient())
        desk_ref = Cards(WyrdClient()).resolve_latest("Service", "support-desk")
        evidence = poll(
            lambda: observer.service(desk_ref), lambda evidence: evidence.failure_point() is None
        )
        assert {"Agent", "Prompt", "Service", "Verifier"} <= set(evidence.kinds)
        assert run_id in evidence.custom_runs["tickets"]
        assert run_id in evidence.telemetry_runs
        assert {(each.execution_id, each.verdict) for each in evidence.queued if each.judged()} == {
            (queued["execution_id"], "passed")
        }
        assert {each.run_id for each in evidence.sync if each.judged()} >= {run_id}
        other = WyrdClient(
            server_url=server.base_url,
            credential=server.bootstrap_service_in_tenant(
                server.seed_tenant("other-agent"), ["admin"]
            ),
            grpc_url=server.grpc_url,
        )
        assert Observer(other).observe().failure_point() == "declared"


def test_quickstart_deploy_keeps_the_prior_bundle_until_pull_replaces_it(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A refused repeat pull leaves the previous bundle usable; a successful one replaces it.

    Declaration and pull are stand-ins, so only the published ``deploy.py``
    caller is under test: the shared pull owner's staged replacement and
    rollback have their own proof.
    """
    shutil.copytree(QUICKSTART, tmp_path / "app", ignore=shutil.ignore_patterns(".wyrd-bundle"))
    modules = quickstart_modules(tmp_path / "app" / "python")
    bundle = tmp_path / "app" / ".wyrd-bundle"
    bundle.mkdir()
    (bundle / "metadata.yaml").write_text("prior")

    def refused(destination: Path) -> None:
        raise RuntimeError("pull refused")

    def replaced(destination: Path) -> None:
        (destination / "metadata.yaml").write_text("replaced")

    pulls = [refused, replaced]

    class Declared:
        root = "Service/support-desk@1.0.0"

    class StandInCards:
        def __init__(self, client: object) -> None: ...

        def declare_from_path(self, path: Path) -> Declared:
            return Declared()

        def pull(self, root: str, destination: Path) -> None:
            pulls.pop(0)(destination)

    class StandInGateway:
        def __init__(self, client: object) -> None: ...

        def put_capture_policy(self, policy: dict[str, Any]) -> None: ...

    monkeypatch.setattr(modules["deploy"], "Cards", StandInCards)
    monkeypatch.setattr(modules["deploy"], "Gateway", StandInGateway)
    with pytest.raises(RuntimeError, match="pull refused"):
        modules["deploy"].deploy()
    assert (bundle / "metadata.yaml").read_text() == "prior"
    modules["deploy"].deploy()
    assert (bundle / "metadata.yaml").read_text() == "replaced"
