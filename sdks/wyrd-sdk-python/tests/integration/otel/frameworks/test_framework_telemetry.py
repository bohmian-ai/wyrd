"""Pydantic AI, Google ADK, and OpenAI Agents SDK runs read back from Bifrost.

Each journey launches ``app.py`` as the application process, configured only
through the environment: the session server's URL, the ``observed-service``
key, and an empty config home. The framework's own instrumentation, or for the
Agents SDK ``WyrdAgentsTraceProcessor``, emits spans for real model and tool
calls against a scripted local upstream; the session administrator reads them
back with ``Bifrost.trace_timeline``.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any

import pytest
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, SpanRecord
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

from ...support import Receiver, ancestry, client_of
from ...test_local_development import configure_gateway
from .app import MODEL, PROMPT, TOOL_RESULT

pytestmark = pytest.mark.integration

FRAMEWORKS = ["pydantic_ai", "google_adk", "openai_agents"]

SDK_ROOT = Path(__file__).resolve().parents[4]
"""The Python SDK root, from which ``app.py`` runs as a module."""

MODEL_OPERATIONS = {"chat", "generate_content"}
"""The GenAI ``gen_ai.operation.name`` values a model call carries."""


@pytest.fixture
def app_env(observed_bundle: Path, observed_key: str, tmp_path: Path) -> dict[str, str]:
    """The application's environment: server URL and Service key only, nothing saved."""
    env = {
        name: value
        for name, value in os.environ.items()
        if name not in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN", "WYRD_TENANT")
    }
    env.update(
        WYRD_CONFIG_HOME=str(tmp_path),
        WYRD_API_KEY=observed_key,
        WYRD_TEST_BUNDLE=str(observed_bundle),
        PYTHONPATH=os.pathsep.join(sys.path),
        GOOGLE_API_KEY="local",
        ADK_CAPTURE_MESSAGE_CONTENT_IN_SPANS="false",
    )
    return env


def launch(env: dict[str, str], framework: str, setup: str, scenario: str) -> dict[str, Any]:
    """Run the application process to completion and return its report."""
    finished = subprocess.run(
        [sys.executable, "-m", "tests.integration.otel.frameworks.app", framework, setup, scenario],
        cwd=SDK_ROOT,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert finished.returncode == 0, finished.stderr
    return json.loads(finished.stdout.splitlines()[-1])


def timeline(
    server: WyrdTestServer, trace_id: str, client: WyrdClient | None = None
) -> list[SpanRecord]:
    """Publish captured rows, then read trace ``trace_id`` as ``client`` (the administrator)."""
    server.flush_bifrost()
    return Bifrost(client=client or client_of(server)).trace_timeline(trace_id)


def correlated(spans: list[SpanRecord], run_id: str) -> list[SpanRecord]:
    """The spans attributed to Run ``run_id``."""
    return [span for span in spans if span["attributes"].get("wyrd.run_id") == run_id]


def operation(spans: list[SpanRecord], names: set[str]) -> list[SpanRecord]:
    """The spans whose ``gen_ai.operation.name`` is one of ``names``."""
    return [span for span in spans if span["attributes"].get("gen_ai.operation.name") in names]


def assert_turn(spans: list[SpanRecord], run_id: str, card_uid: str) -> None:
    """One tool-calling turn: two model calls with usage and one tool call, all in the Run.

    Every span of the trace hangs off the application's ``request`` root, and
    exactly one span records each model and tool call.
    """
    (root,) = [span for span in spans if "parent_span_id" not in span]
    assert root["name"] == "request"
    assert all(ancestry(spans, span)[-1] is root for span in spans)
    in_run = correlated(spans, run_id)
    assert in_run == spans
    assert {span["attributes"]["wyrd.card_uid"] for span in in_run} == {card_uid}
    models = operation(spans, MODEL_OPERATIONS)
    assert len(models) == 2
    for model in models:
        assert model["attributes"]["gen_ai.request.model"] == MODEL
        assert model["attributes"]["gen_ai.usage.input_tokens"] == 7
        assert model["attributes"]["gen_ai.usage.output_tokens"] == 2
    (tool,) = operation(spans, {"execute_tool"})
    assert tool["attributes"]["gen_ai.tool.name"] == "lookup"
    assert len({span["span_id"] for span in spans}) == len(spans)


class Drift(BaseModel):
    """One ``vala.drift.observations`` value."""

    num_value: float


def drift(server: WyrdTestServer, run_id: str) -> list[float]:
    """Publish captured rows, then read Run ``run_id``'s drift values as the administrator."""
    server.flush_bifrost()
    rows = Bifrost(client=client_of(server)).sql(
        "SELECT num_value FROM vala.drift.observations WHERE run_id = $1", [run_id], model=Drift
    )
    return [row.num_value for row in rows]


def assert_observed(server: WyrdTestServer, turn: dict[str, Any], card_uid: str) -> None:
    """``turn``'s spans and its drift observation both read back."""
    assert turn["output"] == "done"
    assert_turn(timeline(server, turn["trace_id"]), turn["run_id"], card_uid)
    assert drift(server, turn["observation"]) == [1.0]


@pytest.mark.parametrize("framework", FRAMEWORKS)
def test_managed_framework_telemetry(
    wyrd_server: WyrdTestServer, app_env: dict[str, str], framework: str, tmp_path: Path
) -> None:
    with (tmp_path / "stderr").open("w") as stderr:
        app = subprocess.Popen(
            [
                sys.executable,
                "-m",
                "tests.integration.otel.frameworks.app",
                framework,
                "managed",
                "live",
            ],
            cwd=SDK_ROOT,
            env=app_env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=stderr,
            text=True,
        )
        try:
            # The app waits after its live ``state.flush``: read that work back first.
            assert app.stdout is not None and app.stdin is not None
            flushed = app.stdout.readline()
            assert flushed, (tmp_path / "stderr").read_text()
            first = json.loads(flushed)
            card_uid = first["card_uid"]
            assert_observed(wyrd_server, first["first"], card_uid)
            app.stdin.write("continue\n")
            app.stdin.flush()
            out, _ = app.communicate(timeout=120)
        finally:
            app.kill()
        assert app.returncode == 0, (tmp_path / "stderr").read_text()
    report = json.loads(out.splitlines()[-1])

    # The same state kept writing after its flush; the open span survived it.
    assert_observed(wyrd_server, report["second"], card_uid)
    assert [span["name"] for span in timeline(wyrd_server, report["held"])] == ["held"]
    # After the state's shutdown, the global provider still exports.
    after = report["after"]
    assert_turn(timeline(wyrd_server, after["trace_id"]), after["run_id"], card_uid)


@pytest.mark.parametrize("framework", FRAMEWORKS)
def test_owned_provider_framework_telemetry(
    wyrd_server: WyrdTestServer, app_env: dict[str, str], framework: str
) -> None:
    report = launch(app_env, framework, "owned", "run")

    turn = report["run"]
    assert_turn(timeline(wyrd_server, turn["trace_id"]), turn["run_id"], report["card_uid"])


@pytest.mark.parametrize("framework", FRAMEWORKS)
def test_framework_execution_edges(
    wyrd_server: WyrdTestServer, app_env: dict[str, str], framework: str
) -> None:
    report = launch(app_env, framework, "managed", "edges")

    failed = report["model_failure"]
    assert failed["error"] is not None
    spans = correlated(timeline(wyrd_server, failed["trace_id"]), failed["run_id"])
    assert any(span["status"]["code"] == "error" for span in operation(spans, MODEL_OPERATIONS))

    failed = report["tool_failure"]
    spans = correlated(timeline(wyrd_server, failed["trace_id"]), failed["run_id"])
    (tool,) = operation(spans, {"execute_tool"})
    assert tool["status"]["code"] == "error"

    # Started inside its Run, finished after the Run exited: still attributed.
    streamed = report["stream"]
    assert streamed["output"] == "done"
    models = operation(timeline(wyrd_server, streamed["trace_id"]), MODEL_OPERATIONS)
    assert {span["attributes"].get("wyrd.run_id") for span in models} == {streamed["run_id"]}
    assert all(span["attributes"].get("gen_ai.usage.output_tokens") == 2 for span in models)

    nested = report["nested"]
    models = operation(timeline(wyrd_server, nested["trace_id"]), MODEL_OPERATIONS)
    assert [span["attributes"]["wyrd.run_id"] for span in models] == [nested["inner"]] * 2 + [
        nested["outer"]
    ] * 2

    concurrent = report["concurrent"]
    models = operation(timeline(wyrd_server, concurrent["trace_id"]), MODEL_OPERATIONS)
    assert sorted(span["attributes"]["wyrd.run_id"] for span in models) == sorted(
        concurrent["run_ids"] * 2
    )

    spans = timeline(wyrd_server, report["uncorrelated"]["trace_id"])
    assert len(operation(spans, MODEL_OPERATIONS)) == 2
    assert not any("wyrd.run_id" in span["attributes"] for span in spans)


@pytest.mark.parametrize("framework", FRAMEWORKS)
def test_framework_sampling_privacy_and_authorization(
    wyrd_server: WyrdTestServer,
    app_env: dict[str, str],
    viewer_key: str,
    other_tenant: str,
    framework: str,
) -> None:
    unsampled = launch(
        {**app_env, "OTEL_TRACES_SAMPLER": "always_off"}, framework, "managed", "run"
    )
    assert timeline(wyrd_server, unsampled["run"]["trace_id"]) == []

    denied = launch({**app_env, "WYRD_API_KEY": viewer_key}, framework, "managed", "run")
    assert denied["run"]["output"] == "done"
    assert timeline(wyrd_server, denied["run"]["trace_id"]) == []

    # Another tenant's credential cannot attribute spans to this tenant's
    # Card: the server rejects each one, so neither tenant reads them.
    key = wyrd_server.bootstrap_service_in_tenant(other_tenant, ["admin"], name=f"{framework}-app")
    elsewhere = launch({**app_env, "WYRD_API_KEY": key}, framework, "managed", "run")
    trace_id = elsewhere["run"]["trace_id"]
    assert timeline(wyrd_server, trace_id) == []
    assert timeline(wyrd_server, trace_id, client_of(wyrd_server, key)) == []

    stored = timeline(wyrd_server, launch(app_env, framework, "managed", "run")["run"]["trace_id"])
    assert len(operation(stored, MODEL_OPERATIONS)) == 2
    assert PROMPT not in json.dumps(stored)
    assert TOOL_RESULT not in json.dumps(stored)


def test_ambient_auth_gateway_telemetry(
    wyrd_server: WyrdTestServer, receiver: Receiver, app_env: dict[str, str]
) -> None:
    configure_gateway(WyrdClient())
    before = len(receiver.to("/v1/chat/completions"))

    report = launch(app_env, "openai_agents", "managed", "gateway")

    turn = report["run"]
    assert turn["output"] == '{"passed":true}'
    (upstream,) = receiver.to("/v1/chat/completions")[before:]
    assert upstream.headers["authorization"] == "Bearer sk-local-upstream"
    assert not any(name.startswith("x-wyrd") for name in upstream.headers)
    spans = correlated(timeline(wyrd_server, turn["trace_id"]), turn["run_id"])
    (model,) = operation(spans, {"chat"})
    assert model["attributes"]["gen_ai.usage.input_tokens"] == 5

    without_credentials = {name: value for name, value in app_env.items() if name != "WYRD_API_KEY"}
    finished = subprocess.run(
        [
            sys.executable,
            "-m",
            "tests.integration.otel.frameworks.app",
            "openai_agents",
            "managed",
            "gateway",
        ],
        cwd=SDK_ROOT,
        env=without_credentials,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert finished.returncode != 0
    # ``WYRD_CLIENT_401_NO_CREDENTIALS``, naming every way to supply one.
    assert "WyrdError: no credentials available; set WYRD_ACCESS_TOKEN" in finished.stderr
