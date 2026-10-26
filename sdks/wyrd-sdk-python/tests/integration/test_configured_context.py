"""``WyrdState.configure`` stamps its context on every table the state writes.

Two states over one Service are configured for ``prod`` and ``staging``. Each
starts telemetry and, inside one Run, verifies, submits, calls the gateway,
and emits a span, a log record, and a metric point. Querying any table by a
context column alone then returns exactly the records written under it.
"""

from __future__ import annotations

import logging
import threading
from http.server import ThreadingHTTPServer
from typing import TYPE_CHECKING, Any

import httpx
import openai
import pytest
from opentelemetry import _logs, metrics, trace
from opentelemetry.sdk._logs import LoggerProvider, LoggingHandler
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import SimpleSpanProcessor
from wyrd import verify
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.gateway import Gateway
from wyrd.otel import span_exporter
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .gateway.support import PROVIDER_KEY, Upstream, deploy
from .support import Receiver, client_of, declare, pull, reset_telemetry_providers, service_key

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

pytestmark = pytest.mark.integration

SPANS = "bifrost.traces.spans"
LOGS = "bifrost.logs.records"
METRICS = "bifrost.metrics.points"
CALLS = "bifrost.gateway.calls"
RESULTS = "bifrost.verification.results"
INPUTS = "bifrost.verification.execution_inputs"

CONTEXT = (
    "service_name, service_version, deployment_environment, "
    "subject_kind, subject_name, subject_version, "
    "CAST(workload_card_uid AS VARCHAR) AS workload, "
    "CAST(subject_card_uid AS VARCHAR) AS subject, run_id"
)
"""The context columns every table carries."""

RECORD = {
    SPANS: "name",
    LOGS: "body_text",
    METRICS: "metric_name",
    CALLS: "requested_model['model']",
    RESULTS: "CAST(execution_id AS VARCHAR)",
    INPUTS: "CAST(execution_id AS VARCHAR)",
}
"""The column that names which written record a row is, per table."""


@pytest.fixture
def server(
    fresh_telemetry_providers: None, monkeypatch: pytest.MonkeyPatch
) -> Iterator[WyrdTestServer]:
    """A server running Verifiers whose gateway reaches a mock OpenAI upstream."""
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    upstream.RequestHandlerClass = type("RecordingUpstream", (Upstream,), {"upstream": recorded})
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    monkeypatch.setenv("WYRD_TEST_GATEWAY_PROVIDER_KEY", PROVIDER_KEY)
    try:
        with WyrdTestServer(
            verification_runtime=True, provider_base_url=recorded.url, mutate_env=False
        ) as running:
            deploy(running, "openai")
            Gateway(client_of(running)).put_capture_policy(
                {"mode": "metadata", "payload_fields": []}
            )
            yield running
            shut_down_telemetry()
    finally:
        upstream.shutdown()


def shut_down_telemetry() -> None:
    """Shut down the global tracer, meter, and logger providers ``start_telemetry`` installed."""
    for provider in (
        trace.get_tracer_provider(),
        metrics.get_meter_provider(),
        _logs.get_logger_provider(),
    ):
        if isinstance(provider, (TracerProvider, MeterProvider, LoggerProvider)):
            provider.shutdown()


def configured_service(server: WyrdTestServer, tmp_path: Path) -> tuple[Path, str, str]:
    """Register the ``configured-service`` fixture.

    Returns its pulled bundle, a Service key, and the ``agent-check`` Verifier's UID.
    """
    cards = Cards(client_of(server))
    declared = declare(cards, "cards/observe_a_run/configured-service.yaml")
    bundle = pull(cards, "configured-service", tmp_path)
    key = service_key("configured-service", client=client_of(server))
    return bundle, key, str(declared["agent-check"].uid)


def run_the_agent(state: WyrdState, server: WyrdTestServer) -> tuple[str, str, str]:
    """Do one Run of the agent's work as ``state``, then shut down as an exiting app does.

    Returns the Run id and the execution ids of the direct and submitted
    verifications.
    """
    state.start_telemetry()
    state.start_bifrost()
    llm = openai.OpenAI(
        base_url=f"{server.base_url}/v1",
        api_key="wyrd",
        http_client=openai.DefaultHttpxClient(auth=state.http_auth),
        max_retries=0,
    )
    logger = logging.getLogger("wyrd.tests.configured")
    logger.propagate = False
    handler = LoggingHandler()
    logger.addHandler(handler)

    with state.run("agent") as run:
        verified = run.observe.verify("agent-check", "yes")
        submitted = run.observe.submit("agent-check", "yes")
        assert isinstance(submitted, verify.Execution)
        submitted.wait(60.0)
        llm.chat.completions.create(
            model="openai/gpt-4o", messages=[{"role": "user", "content": "hi"}]
        )
        trace.get_tracer("wyrd.tests.configured").start_span("configured.work").end()
        logger.warning("configured work done")
        metrics.get_meter("wyrd.tests.configured").create_counter("configured.work").add(1)

    logger.removeHandler(handler)
    state.shutdown()
    shut_down_telemetry()
    return run.run_id, verified.execution_id, submitted.id


def query(server: WyrdTestServer, sql: str) -> list[dict[str, Any]]:
    """Run ``sql`` against Bifrost as the tenant administrator."""
    return Bifrost(client=client_of(server)).sql(sql).to_arrow().to_pylist()


def records(server: WyrdTestServer, table: str, where: str) -> list[Any]:
    """Which written records of ``table`` match ``where``, sorted."""
    found = query(server, f"SELECT {RECORD[table]} AS record FROM {table} WHERE {where}")
    return sorted(row["record"] for row in found)


def contexts(server: WyrdTestServer, table: str, where: str) -> list[dict[str, Any]]:
    """The distinct context column values of the ``table`` rows matching ``where``."""
    return query(server, f"SELECT DISTINCT {CONTEXT} FROM {table} WHERE {where}")


def test_configure_stamps_every_table_and_each_context_column_finds_the_records(
    server: WyrdTestServer, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    bundle, key, _ = configured_service(server, tmp_path)

    prod = WyrdState.from_path(bundle, client_of(server, key))
    prod.configure(
        service_name="configured-service", service_version="1.0.0", deployment_environment="prod"
    )
    prod_run, prod_verified, prod_submitted = run_the_agent(prod, server)

    # A process installs each OpenTelemetry provider once, so the staging
    # state gets its own only after prod's are cleared.
    reset_telemetry_providers(monkeypatch)
    staging = WyrdState.from_path(bundle, client_of(server, key))
    staging.configure(
        service_name="configured-service",
        service_version="1.0.0",
        deployment_environment="staging",
    )
    staging_run, staging_verified, staging_submitted = run_the_agent(staging, server)
    server.flush_bifrost()

    service = str(prod.root_ref.uid)
    agent = str(prod.card_ref("agent").uid)
    prod_context = {
        "service_name": "configured-service",
        "service_version": "1.0.0",
        "deployment_environment": "prod",
        "subject_kind": "Agent",
        "subject_name": "observed-agent",
        "subject_version": "1.0.0",
        "workload": service,
        "subject": agent,
        "run_id": prod_run,
    }
    staging_context = {**prod_context, "deployment_environment": "staging", "run_id": staging_run}

    # Every row written inside a Run carries the configured context.
    for table in (SPANS, CALLS, RESULTS, INPUTS):
        assert contexts(server, table, f"run_id = '{prod_run}'") == [prod_context], table
        assert contexts(server, table, f"run_id = '{staging_run}'") == [staging_context], table

    # Log records and metric points carry the Service and environment. Only
    # spans inherit the subject and Run from the active context.
    no_subject = {
        "subject_kind": None,
        "subject_name": None,
        "subject_version": None,
        "subject": None,
        "run_id": None,
    }
    for table in (LOGS, METRICS):
        prod_rows = contexts(server, table, "deployment_environment = 'prod'")
        staging_rows = contexts(server, table, "deployment_environment = 'staging'")
        assert prod_rows == [{**prod_context, **no_subject}], table
        assert staging_rows == [{**staging_context, **no_subject}], table

    # Filtering by the environment returns exactly that environment's records.
    prod_only = "deployment_environment = 'prod'"
    assert records(server, SPANS, prod_only) == ["chat", "configured.work"]
    assert records(server, LOGS, prod_only) == ["configured work done"]
    # A cumulative counter is exported at every collection: once by the
    # state's shutdown flush and once by the provider's own shutdown.
    assert records(server, METRICS, prod_only) == ["configured.work", "configured.work"]
    assert records(server, CALLS, prod_only) == ["gpt-4o"]
    assert records(server, RESULTS, prod_only) == sorted([prod_verified, prod_submitted])
    assert records(server, INPUTS, prod_only) == sorted([prod_verified, prod_submitted])
    staging_only = "deployment_environment = 'staging'"
    assert records(server, RESULTS, staging_only) == sorted([staging_verified, staging_submitted])

    # The records themselves are the ones written.
    assert (
        query(server, f"SELECT int_value FROM {METRICS} WHERE {prod_only}")
        == [{"int_value": 1}] * 2
    )
    assert query(server, f"SELECT outcome FROM {CALLS} WHERE {prod_only}") == [
        {"outcome": "succeeded"}
    ]
    assert query(server, f"SELECT DISTINCT verdict FROM {RESULTS} WHERE {prod_only}") == [
        {"verdict": "passed"}
    ]
    assert query(
        server,
        f"SELECT CAST(execution_id AS VARCHAR) AS execution, queue_execution "
        f"FROM {INPUTS} WHERE {prod_only} ORDER BY queue_execution",
    ) == [
        {"execution": prod_verified, "queue_execution": False},
        {"execution": prod_submitted, "queue_execution": True},
    ]

    # Each shared context column alone finds both environments' records.
    for where in (
        "service_name = 'configured-service'",
        "service_version = '1.0.0'",
        f"workload_card_uid = '{service}'",
    ):
        for table in (SPANS, LOGS, METRICS, CALLS, RESULTS, INPUTS):
            assert records(server, table, where) == sorted(
                records(server, table, prod_only) + records(server, table, staging_only)
            ), (table, where)

    # Subject columns find the records written inside a Run, in both environments.
    for where in (
        "subject_kind = 'Agent'",
        "subject_name = 'observed-agent'",
        "subject_version = '1.0.0'",
        f"subject_card_uid = '{agent}'",
    ):
        assert records(server, SPANS, where) == sorted(["chat", "configured.work"] * 2), where
        assert records(server, CALLS, where) == ["gpt-4o"] * 2, where
        assert records(server, RESULTS, where) == sorted(
            [prod_verified, prod_submitted, staging_verified, staging_submitted]
        ), where
        assert records(server, LOGS, where) == [], where
        assert records(server, METRICS, where) == [], where

    # The Run id finds only that Run's records.
    prod_run_only = f"run_id = '{prod_run}'"
    assert records(server, SPANS, prod_run_only) == ["chat", "configured.work"]
    assert records(server, CALLS, prod_run_only) == ["gpt-4o"]
    assert records(server, RESULTS, prod_run_only) == sorted([prod_verified, prod_submitted])
    assert records(server, INPUTS, prod_run_only) == sorted([prod_verified, prod_submitted])


def test_verification_environment_comes_from_the_request_body(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    bundle, key, verifier = configured_service(server, tmp_path)
    state = WyrdState.from_path(bundle, client_of(server, key))
    service = str(state.root_ref.uid)
    agent = str(state.card_ref("agent").uid)

    answer = httpx.post(
        f"{server.base_url}/v1/verification/execute",
        json={
            "verifier_card_uid": verifier,
            "card_uid": agent,
            "inputs": {"answer": {"kind": "json", "value": "yes"}},
            "deployment_environment": "prod",
        },
        headers={"x-wyrd-deployment-environment": "staging"},
        auth=client_of(server, key, workload_uid=service).http_auth,
        timeout=60.0,
    )
    assert answer.status_code == 200, answer.text
    server.flush_bifrost()

    execution = answer.json()["execution_id"]
    for table in (RESULTS, INPUTS):
        found = query(
            server,
            f"SELECT deployment_environment FROM {table} "
            f"WHERE CAST(execution_id AS VARCHAR) = '{execution}'",
        )
        assert found == [{"deployment_environment": "prod"}], table


def test_registered_service_name_wins_over_the_resource_name(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    bundle, key, _ = configured_service(server, tmp_path)
    service = str(WyrdState.from_path(bundle, client_of(server, key)).root_ref.uid)

    provider = TracerProvider(resource=Resource.create({"service.name": "renamed-service"}))
    exporter = span_exporter(client_of(server, key, workload_uid=service))
    provider.add_span_processor(SimpleSpanProcessor(exporter))
    provider.get_tracer("wyrd.tests.configured").start_span("renamed.work").end()
    provider.shutdown()
    server.flush_bifrost()

    found = query(
        server,
        f"SELECT service_name, resource_attributes FROM {SPANS} WHERE name = 'renamed.work'",
    )
    assert len(found) == 1
    assert found[0]["service_name"] == "configured-service"
    assert "renamed-service" in str(found[0]["resource_attributes"])
