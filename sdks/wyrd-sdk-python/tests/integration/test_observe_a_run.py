"""One Run of the ``observed-service`` emits Drift, Eval, generic, and span rows that read back by run id."""

from __future__ import annotations

import asyncio
import json
import os
from collections.abc import Iterator
from pathlib import Path

import pytest
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from opentelemetry.proto.common.v1.common_pb2 import KeyValueList
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from pydantic import BaseModel
from wyrd import WyrdError, cli
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.cards import CardRef, Cards
from wyrd.eval import MediaRef
from wyrd.otel import install_run_correlation
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .conftest import StandInModel, download, register

DATASET = "vala.datasets.observed_rows"
EXPLICIT_TRACE = "0af7651916cd43dd8448eb211c80319c"
EXPLICIT_SPAN = "b7ad6b7169203331"
SESSION = "0190f5a4-8c3e-7b21-9d4f-3a6b2c1d0e9f"
MEDIA = MediaRef(id="screenshot", kind="image", uri="s3://bucket/shot.png", media_type="image/png")
MEDIA_TEXT = (
    '[{"id":"screenshot","kind":"image","uri":"s3://bucket/shot.png","media_type":"image/png"}]'
)


class DatasetRow(BaseModel):
    """The one caller-owned column of the generic table."""

    value: int


class CorrelatedRow(BaseModel):
    """A generic row read back with the managed identity stamped onto it."""

    value: int
    run_id: str | None
    card_uid: str | None


class DriftRow(BaseModel):
    """The tall Drift projection: one row per emitted feature."""

    series: str
    num_value: float | None
    str_value: str | None
    card_uid: str | None


class EvalRow(BaseModel):
    """An Eval row, its trace identity, and its subject."""

    context: str
    session_id: str | None
    trace_id: bytes | None
    span_id: bytes | None
    media: str | None
    card_uid: str | None

    def trace_hex(self) -> tuple[str | None, str | None]:
        """The stored trace and span identity as lower-case hex."""
        return (
            None if self.trace_id is None else self.trace_id.hex(),
            None if self.span_id is None else self.span_id.hex(),
        )


class SpanRow(BaseModel):
    """A persisted span with its asserted attributes and resolved subject."""

    name: str
    attributes: bytes
    card_uid: str | None

    def asserted_card_ref(self) -> str:
        """The ``wyrd.card_ref`` the span asserted, from its lossless attribute payload."""
        values = KeyValueList.FromString(self.attributes).values
        return {item.key: item.value.string_value for item in values}["wyrd.card_ref"]


@pytest.fixture(scope="module")
def observed_bundle(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """The ``observed-service`` graph registered and downloaded, with the generic table registered."""
    register(cards, "cards/observe_a_run/observed-model.yaml")
    register(cards, "cards/observe_a_run/observed-service.yaml")
    writer = Bifrost(TableConfig(DatasetRow, DATASET))
    writer.register()
    writer.shutdown()
    return download("observed-service", tmp_path_factory.mktemp("observed"))


@pytest.fixture(scope="module")
def observed_key(wyrd_server: WyrdTestServer, observed_bundle: Path) -> str:
    """The ``observed-service`` Service's own workload key."""
    return cli.issue_key(kind="Service", name="observed-service", version="1.0.0", space="default")[
        "key"
    ]


@pytest.fixture
def observed(
    observed_bundle: Path, observed_key: str, monkeypatch: pytest.MonkeyPatch
) -> Iterator[WyrdState]:
    """The hydrated ``observed-service`` with Bifrost started under its own key."""
    monkeypatch.setenv("WYRD_API_KEY", observed_key)
    state = WyrdState.from_path(observed_bundle, interfaces={"model": StandInModel()})
    state.start_bifrost()
    yield state
    state.shutdown()


def publish(state: WyrdState, server: WyrdTestServer) -> None:
    """Drain the state's writer and publish everything it sent."""
    state.flush()
    server.flush_bifrost()


@pytest.mark.integration
def test_run_observations_read_back_by_run_id(
    observed: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    run = observed.run()
    model, agent = run.for_card("model"), run.for_card("agent")
    model.observe.drift({"latency_ms": 12.5, "tier": "gold"})
    agent.observe.eval({"answer": "yes"})
    agent.observe.record(DATASET, {"value": 41})
    model.observe.record(DATASET, {"value": 42})
    publish(observed, wyrd_server)

    model_uid = str(observed.card_ref("model").uid)
    agent_uid = str(observed.card_ref("agent").uid)
    drift = bifrost.sql(
        "SELECT series, num_value, str_value, card_uid FROM vala.drift.observations "
        "WHERE run_id = $1 ORDER BY series",
        [run.run_id],
        model=DriftRow,
    )
    evals = bifrost.sql(
        "SELECT context, session_id, trace_id, span_id, media, card_uid "
        "FROM vala.eval.observations WHERE run_id = $1",
        [run.run_id],
        model=EvalRow,
    )
    rows = bifrost.sql(
        f"SELECT value, run_id, card_uid FROM {DATASET} WHERE run_id = $1 ORDER BY value",
        [run.run_id],
        model=CorrelatedRow,
    )

    assert drift == [
        DriftRow(series="latency_ms", num_value=12.5, str_value="12.5", card_uid=model_uid),
        DriftRow(series="tier", num_value=None, str_value="gold", card_uid=model_uid),
    ]
    assert [(json.loads(row.context), row.card_uid) for row in evals] == [
        ({"answer": "yes"}, agent_uid)
    ]
    assert rows == [
        CorrelatedRow(value=41, run_id=run.run_id, card_uid=agent_uid),
        CorrelatedRow(value=42, run_id=run.run_id, card_uid=model_uid),
    ]


@pytest.mark.integration
def test_run_view_exposes_its_alias(observed: WyrdState) -> None:
    run = observed.run()
    model = run.for_card("model")

    assert (model.alias, model.run_id) == ("model", run.run_id)
    with pytest.raises(WyrdError) as unknown:
        run.for_card("missing")
    assert unknown.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"


@pytest.mark.integration
def test_card_scoped_key_cannot_write_another_cards_observations(
    observed_bundle: Path, wyrd_server: WyrdTestServer, monkeypatch: pytest.MonkeyPatch
) -> None:
    """A key bound to the Agent Card cannot write observations for the Model Card."""
    agent_key = cli.issue_key(kind="Agent", name="observed-agent", version="1.0.0", space="default")
    monkeypatch.setenv("WYRD_API_KEY", agent_key["key"])
    state = WyrdState.from_path(observed_bundle, interfaces={"model": StandInModel()})
    state.start_bifrost()
    state.run().for_card("model").observe.drift({"latency_ms": 1.0})

    with pytest.raises(WyrdError) as refused:
        state.flush()
    state.shutdown()
    assert refused.value.code == "WYRD_VALA_403_BIFROST_CARD_SCOPE"


@pytest.mark.integration
def test_eval_carries_trace_session_and_media(
    observed: WyrdState, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    """No span stores no ids, an active span stores its ids, and explicit ids win."""
    agent = observed.run().for_card("agent")
    tracer = TracerProvider().get_tracer("wyrd.tests.observe")
    agent.observe.eval({"answer": "untraced"})
    with tracer.start_as_current_span("observe") as span:
        context = span.get_span_context()
        agent.observe.eval({"answer": "traced"})
        agent.observe.eval(
            {"answer": "explicit"},
            session_id=SESSION,
            media=[MEDIA],
            trace_id=EXPLICIT_TRACE,
            span_id=EXPLICIT_SPAN,
        )
    publish(observed, wyrd_server)

    evals = bifrost.sql(
        "SELECT context, session_id, trace_id, span_id, media, card_uid "
        "FROM vala.eval.observations WHERE run_id = $1",
        [agent.run_id],
        model=EvalRow,
    )
    by_answer = {json.loads(row.context)["answer"]: row for row in evals}
    assert by_answer["untraced"].trace_hex() == (None, None)
    assert by_answer["traced"].trace_hex() == (
        f"{context.trace_id:032x}",
        f"{context.span_id:016x}",
    )
    assert by_answer["explicit"].trace_hex() == (EXPLICIT_TRACE, EXPLICIT_SPAN)
    assert (by_answer["explicit"].session_id, by_answer["explicit"].media) == (SESSION, MEDIA_TEXT)
    assert (by_answer["untraced"].session_id, by_answer["untraced"].media) == (None, None)


@pytest.mark.integration
@pytest.mark.parametrize(
    ("trace_id", "span_id", "media"),
    [
        (None, EXPLICIT_SPAN, None),
        ("zz" * 16, EXPLICIT_SPAN, None),
        (EXPLICIT_TRACE[:30], EXPLICIT_SPAN, None),
        (EXPLICIT_TRACE, "zz" * 8, None),
        (EXPLICIT_TRACE, EXPLICIT_SPAN + "00", None),
        (None, None, MediaRef(id="shot", kind="hologram", uri="s3://bucket/shot.png")),  # ty: ignore[invalid-argument-type]
    ],
)
def test_malformed_eval_identity_is_refused(
    observed: WyrdState, trace_id: str | None, span_id: str | None, media: MediaRef | None
) -> None:
    agent = observed.run().for_card("agent")
    with pytest.raises(WyrdError) as refused:
        agent.observe.eval(
            {"answer": "refused"},
            trace_id=trace_id,
            span_id=span_id,
            media=None if media is None else [media],
        )
    assert refused.value.code == "WYRD_SPEC_400_VALIDATION"


@pytest.mark.integration
@pytest.mark.parametrize(
    ("table", "code"),
    [
        ("vala.drift.observations", "WYRD_SDK_400_INVALID_OBSERVATION"),
        ("vala.datasets.never_registered", "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND"),
    ],
)
def test_record_into_a_managed_or_unregistered_table_is_refused(
    observed: WyrdState, table: str, code: str
) -> None:
    with pytest.raises(WyrdError) as refused:
        observed.run().for_card("agent").observe.record(table, {"value": 1})
    assert refused.value.code == code


@pytest.fixture
def otlp_tracer(observed_key: str) -> Iterator[TracerProvider]:
    """A private stock OTel provider exporting OTLP/HTTP with the Service's API key.

    The provider is private, as an agent framework's often is, so Wyrd's
    correlation processor is registered through the explicit hook.
    """
    provider = TracerProvider()
    provider.add_span_processor(
        BatchSpanProcessor(
            OTLPSpanExporter(
                endpoint=f"{os.environ['WYRD_SERVER_URL']}/v1/traces",
                headers={"x-wyrd-api-key": observed_key},
            )
        )
    )
    assert install_run_correlation(provider) is True
    yield provider
    provider.shutdown()


@pytest.mark.integration
def test_framework_spans_join_their_run_scope(
    observed: WyrdState,
    otlp_tracer: TracerProvider,
    wyrd_server: WyrdTestServer,
    bifrost: Bifrost,
) -> None:
    """Spans created inside ``with state.run("agent")`` carry its scope and join its rows."""
    tracer = otlp_tracer.get_tracer("framework")
    with observed.run("agent") as run:
        with (
            tracer.start_as_current_span("agent.invoke"),
            tracer.start_as_current_span("agent.tool"),
        ):
            run.observe.eval({"answer": "scoped"})
    assert otlp_tracer.force_flush()
    publish(observed, wyrd_server)

    agent = observed.card_ref("agent")
    spans = bifrost.sql(
        "SELECT name, attributes, card_uid FROM vala.traces.spans WHERE run_id = $1 ORDER BY name",
        [run.run_id],
        model=SpanRow,
    )
    joined = (
        bifrost.sql(
            "SELECT s.name FROM vala.eval.observations e JOIN vala.traces.spans s "
            "ON e.trace_id = s.trace_id AND e.span_id = s.span_id WHERE e.run_id = $1",
            [run.run_id],
        )
        .to_arrow()
        .column("name")
        .to_pylist()
    )
    assert [(span.name, span.asserted_card_ref(), span.card_uid) for span in spans] == [
        ("agent.invoke", str(agent), str(agent.uid)),
        ("agent.tool", str(agent), str(agent.uid)),
    ]
    assert joined == ["agent.tool"]


@pytest.mark.integration
def test_nested_and_async_scopes_stamp_their_view(
    observed: WyrdState,
    otlp_tracer: TracerProvider,
    wyrd_server: WyrdTestServer,
    bifrost: Bifrost,
) -> None:
    """Entry stamps an active span, nesting restores the outer view, and tasks keep their view."""
    tracer = otlp_tracer.get_tracer("framework")
    run = observed.run()
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

    with tracer.start_as_current_span("pre.active"), run:
        tracer.start_span("root.before").end()
        with model:
            tracer.start_span("model.inner").end()
        tracer.start_span("root.restored").end()
        asyncio.run(main())
    assert otlp_tracer.force_flush()
    publish(observed, wyrd_server)

    spans = bifrost.sql(
        "SELECT name, attributes, card_uid FROM vala.traces.spans WHERE run_id = $1",
        [run.run_id],
        model=SpanRow,
    )
    view_of = {
        "pre.active": "root",
        "root.before": "root",
        "root.restored": "root",
        "async.await": "root",
        "model.inner": "model",
        "async.spawned": "model",
        "async.concurrent.a": "agent",
        "async.concurrent.b": "agent",
    }
    refs: dict[str, CardRef] = {alias: observed.card_ref(alias) for alias in set(view_of.values())}
    assert {span.name: (span.asserted_card_ref(), span.card_uid) for span in spans} == {
        name: (str(refs[alias]), str(refs[alias].uid)) for name, alias in view_of.items()
    }


BURST_OBSERVATIONS = 1_000
BURST_FEATURES = 9


@pytest.mark.integration
def test_unsealable_byte_budget_is_refused(observed_bundle: Path, observed_key: str) -> None:
    state = WyrdState.from_path(observed_bundle, interfaces={"model": StandInModel()})
    with pytest.raises(WyrdError) as refused:
        state.start_bifrost(credential=observed_key, client_byte_limit_bytes=1024)
    assert refused.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"


@pytest.mark.integration
def test_drift_burst_survives_a_byte_budget_override(
    observed_bundle: Path, observed_key: str, wyrd_server: WyrdTestServer, bifrost: Bifrost
) -> None:
    """A 1,000 x 9 Drift burst through a 16 MiB budget lands exactly once per feature."""
    state = WyrdState.from_path(observed_bundle, interfaces={"model": StandInModel()})
    state.start_bifrost(credential=observed_key, client_byte_limit_bytes=16 * 1024 * 1024)
    run = state.run()
    model = run.for_card("model")
    for observation in range(BURST_OBSERVATIONS):
        features = {f"feature_{n}": observation + n / 10 for n in range(BURST_FEATURES)}
        # A full queue refuses the whole observation at once, so it is resubmitted after a flush.
        while True:
            try:
                model.observe.drift(features)
                break
            except WyrdError as error:
                if error.code != "WYRD_CLIENT_429_QUEUE_FULL":
                    raise
                state.flush()
    state.shutdown()
    wyrd_server.flush_bifrost()

    counts = (
        bifrost.sql(
            "SELECT COUNT(*) AS n FROM vala.drift.observations WHERE run_id = $1 GROUP BY record_id",
            [run.run_id],
        )
        .to_arrow()
        .column("n")
        .to_pylist()
    )
    assert len(counts) == BURST_OBSERVATIONS
    assert set(counts) == {BURST_FEATURES}
