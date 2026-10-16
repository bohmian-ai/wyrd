"""The Python observation surface beyond the shared story: dataset records,
OpenTelemetry run scopes, and a burst of drift samples.

Every test runs the ``observed-service`` with Bifrost observed_with_bifrost under its own key
and reads its rows back through the public Bifrost client.
"""

from __future__ import annotations

import asyncio
from collections.abc import Iterator

import pytest
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.otel import install_run_correlation
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration

DATASET = "vala.datasets.observed_rows"
"""A caller-owned table runs record into."""

BURST_OBSERVATIONS = 1_000
BURST_FLUSH_EVERY = 100
"""Observations admitted between flushes, far below the 16 MiB budget the burst runs through."""


class DatasetRow(BaseModel):
    """The one caller-owned column of ``DATASET``."""

    value: int


class RecordRow(BaseModel):
    """A ``DATASET`` row read back with the Card that recorded it."""

    value: int
    card_uid: str


class SpanRow(BaseModel):
    """A persisted span and the Card its run scope resolved to."""

    name: str
    card_uid: str | None


class FeatureCount(BaseModel):
    """How many Drift rows one sample produced."""

    n: int


@pytest.fixture(scope="module")
def dataset(wyrd_server: WyrdTestServer) -> str:
    """``DATASET``, registered through the SDK."""
    registrar = Bifrost(TableConfig(DatasetRow, DATASET))
    registrar.register()
    registrar.shutdown()
    return DATASET


@pytest.fixture
def otlp_tracer(otlp_environment: None, observed_key: str) -> Iterator[TracerProvider]:
    """A private stock OTel provider exporting to the deployment as the ``observed-service``.

    The provider is private, as an agent framework's often is, so Wyrd's
    correlation processor is registered through the explicit hook. Spans
    asserting a ``wyrd.card_uid`` are admitted only within the exporting key's
    Card scope, so the exporter authenticates with the Service's own key.
    """
    provider = TracerProvider()
    provider.add_span_processor(
        BatchSpanProcessor(OTLPSpanExporter(headers={"x-wyrd-api-key": observed_key}))
    )
    assert install_run_correlation(provider) is True
    yield provider
    provider.shutdown()


def test_records_land_under_the_view_that_wrote_them(
    observed_with_bifrost: WyrdState, dataset: str, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    run = observed_with_bifrost.run()

    run.for_card("agent").observe.record(dataset, {"value": 41})
    run.for_card("model").observe.record(dataset, {"value": 42})
    observed_with_bifrost.flush()
    wyrd_server.flush_bifrost()

    rows = bifrost.sql(
        f"SELECT value, card_uid FROM {dataset} WHERE run_id = $1 ORDER BY value",
        [run.run_id],
        model=RecordRow,
    )
    assert rows == [
        RecordRow(value=41, card_uid=str(observed_with_bifrost.card_ref("agent").uid)),
        RecordRow(value=42, card_uid=str(observed_with_bifrost.card_ref("model").uid)),
    ]


@pytest.mark.parametrize(
    ("table", "code"),
    [
        ("vala.drift.observations", "WYRD_SDK_400_INVALID_OBSERVATION"),
        ("vala.datasets.never_registered", "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND"),
    ],
)
def test_record_into_a_managed_or_unregistered_table_is_refused(
    observed_with_bifrost: WyrdState, table: str, code: str
) -> None:
    with pytest.raises(WyrdError) as refused:
        observed_with_bifrost.run("agent").observe.record(table, {"value": 1})
    assert refused.value.code == code


def test_framework_spans_join_their_run_scope(
    observed_with_bifrost: WyrdState,
    otlp_tracer: TracerProvider,
    wyrd_server: WyrdTestServer,
    bifrost: Bifrost,
) -> None:
    """Spans created inside ``with state.run("agent")`` carry its scope."""
    tracer = otlp_tracer.get_tracer("framework")
    with (
        observed_with_bifrost.run("agent") as run,
        tracer.start_as_current_span("agent.invoke"),
        tracer.start_as_current_span("agent.tool"),
    ):
        pass
    assert otlp_tracer.force_flush()
    observed_with_bifrost.flush()
    wyrd_server.flush_bifrost()

    agent = str(observed_with_bifrost.card_ref("agent").uid)
    spans = bifrost.sql(
        "SELECT name, card_uid FROM vala.traces.spans WHERE run_id = $1 ORDER BY name",
        [run.run_id],
        model=SpanRow,
    )
    assert spans == [
        SpanRow(name="agent.invoke", card_uid=agent),
        SpanRow(name="agent.tool", card_uid=agent),
    ]


def test_nested_scope_stamps_its_view_and_restores_the_outer_one(
    observed_with_bifrost: WyrdState,
    otlp_tracer: TracerProvider,
    wyrd_server: WyrdTestServer,
    bifrost: Bifrost,
) -> None:
    """Entry stamps an already-active span, and leaving a nested view restores the outer one."""
    tracer = otlp_tracer.get_tracer("framework")
    run = observed_with_bifrost.run()
    with tracer.start_as_current_span("pre.active"), run:
        tracer.start_span("root.before").end()
        with run.for_card("model"):
            tracer.start_span("model.inner").end()
        tracer.start_span("root.restored").end()
    assert otlp_tracer.force_flush()
    wyrd_server.flush_bifrost()

    spans = bifrost.sql(
        "SELECT name, card_uid FROM vala.traces.spans WHERE run_id = $1 ORDER BY name",
        [run.run_id],
        model=SpanRow,
    )
    root, model = (
        str(observed_with_bifrost.card_ref("root").uid),
        str(observed_with_bifrost.card_ref("model").uid),
    )
    assert spans == [
        SpanRow(name="model.inner", card_uid=model),
        SpanRow(name="pre.active", card_uid=root),
        SpanRow(name="root.before", card_uid=root),
        SpanRow(name="root.restored", card_uid=root),
    ]


def test_async_tasks_keep_the_view_they_started_in(
    observed_with_bifrost: WyrdState,
    otlp_tracer: TracerProvider,
    wyrd_server: WyrdTestServer,
    bifrost: Bifrost,
) -> None:
    """An awaited coroutine, concurrent tasks, and a spawned task each keep their own view."""
    tracer = otlp_tracer.get_tracer("framework")
    run = observed_with_bifrost.run()
    model, agent = run.for_card("model"), run.for_card("agent")

    async def concurrent(name: str) -> None:
        with agent:
            await asyncio.sleep(0)
            tracer.start_span(name).end()

    async def spawned() -> None:
        await asyncio.sleep(0)
        tracer.start_span("async.spawned").end()

    async def main() -> None:
        await asyncio.sleep(0)
        tracer.start_span("async.await").end()
        await asyncio.gather(concurrent("async.concurrent.a"), concurrent("async.concurrent.b"))
        with model:
            task = asyncio.create_task(spawned())
        await task

    with run:
        asyncio.run(main())
    assert otlp_tracer.force_flush()
    wyrd_server.flush_bifrost()

    spans = bifrost.sql(
        "SELECT name, card_uid FROM vala.traces.spans WHERE run_id = $1 ORDER BY name",
        [run.run_id],
        model=SpanRow,
    )
    root = str(observed_with_bifrost.card_ref("root").uid)
    assert spans == [
        SpanRow(name="async.await", card_uid=root),
        SpanRow(
            name="async.concurrent.a", card_uid=str(observed_with_bifrost.card_ref("agent").uid)
        ),
        SpanRow(
            name="async.concurrent.b", card_uid=str(observed_with_bifrost.card_ref("agent").uid)
        ),
        SpanRow(name="async.spawned", card_uid=str(observed_with_bifrost.card_ref("model").uid)),
    ]


def test_drift_burst_through_a_byte_budget_override_lands_once_per_sample(
    observed: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    """A burst of 1,000 ``observed-drift`` samples through a 16 MiB budget lands exactly once each."""
    observed.start_bifrost(client_byte_limit_bytes=16 * 1024 * 1024)
    model = observed.run("model")
    for observation in range(BURST_OBSERVATIONS):
        model.observe.submit("observed-drift", {"latency": observation / 10})
        if (observation + 1) % BURST_FLUSH_EVERY == 0:
            observed.flush()
    observed.shutdown()
    wyrd_server.flush_bifrost()

    counts = bifrost.sql(
        "SELECT COUNT(*) AS n FROM vala.drift.observations WHERE run_id = $1 GROUP BY record_id",
        [model.run_id],
        model=FeatureCount,
    )
    assert len(counts) == BURST_OBSERVATIONS
    assert {count.n for count in counts} == {1}
