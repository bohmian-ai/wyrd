"""Python scoped-observation journey through the public ``wyrd`` package.

One invocation switches from the root Service to its Model and Agent views,
emits a Drift mapping and an Eval context, writes one row into a
caller-registered generic table, drains at graceful shutdown, and reads every
row back by the exact subject Card UID and the one invocation id. Startup is
first refused once per fixed table whose describe the server fails, and the
server's staged describe decisions prove repeated writes reuse cached schemas.

A second, single-Card run is entered with ``with state.run(card="agent")``:
framework-style spans created inside it export through the stock OTLP/HTTP
exporter to the authenticated ``/v1/traces`` endpoint, and the persisted span,
custom, and Eval rows join on their Run and trace identity. A third run proves
the scope stamps a span already active at entry, nested root and component
scopes share its run id and restore the outer Card, and spans created after an
``await``, in concurrent tasks sharing one immutable view, and in a task
created inside a scope persist with their exact correlation. A writer holding
a stale declaration of the generic table the run wrote is fenced by the
server's schema fingerprint, and malformed trace identity fails visibly.
"""

from __future__ import annotations

import asyncio
import base64
import hashlib
import json
import os
import subprocess
import sys
import urllib.parse
import urllib.request
from pathlib import Path
from uuid import uuid4

import pytest
import wyrd
from opentelemetry.exporter.otlp.proto.http.trace_exporter import OTLPSpanExporter
from opentelemetry.proto.common.v1.common_pb2 import KeyValueList
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.cards import CardRef, Cards
from wyrd.eval import MediaRef
from wyrd.model import ModelInterface
from wyrd.observe import Run
from wyrd.otel import install_run_correlation
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

PROMPT_ARTIFACT = b"observe-prompt-artifact"
MODEL_ARTIFACT = b"observe-model-artifact"


class NoopModelInterface(ModelInterface):
    """Stand-in for the fixture Model's ``Custom`` loader.

    The journey observes the Model Card, it never runs it, and the registered
    Card names a loader module that does not exist offline. Supplying this
    instance per alias is what keeps hydration from importing it.
    """

    def __init__(self) -> None:
        """Start with the empty holder slots the Model holder expects after load."""
        super().__init__()
        self.model: object = None

    def save(self, path: Path, save_kwargs: dict[str, object] | None = None) -> None:
        """Never called: the journey registers the Card, it does not save one."""
        raise NotImplementedError

    def load(self, path: Path, load_kwargs: dict[str, object] | None = None) -> None:
        """Read nothing and publish a deterministic stand-in as the loaded model."""
        self.model = lambda value: value


class DatasetRow(BaseModel):
    """The one caller-owned column the generic table carries."""

    value: int


class StaleDatasetRow(DatasetRow):
    """A writer's outdated declaration of the generic table, with a column it never had."""

    note: str


class CorrelatedDatasetRow(DatasetRow):
    """A generic row read back with the managed identity stamped onto it."""

    run_id: str | None
    card_uid: str | None


class DriftRow(BaseModel):
    """The tall Drift projection one emit produces, one row per feature."""

    series: str
    num_value: float | None
    str_value: str | None
    card_uid: str | None
    run_id: str | None


class EvalRow(BaseModel):
    """The Eval row, its trace identity, and the managed identity stamped onto it."""

    context: str
    session_id: str | None
    trace_id: bytes | None
    span_id: bytes | None
    media: str | None
    card_uid: str | None
    run_id: str | None

    def trace_hex(self) -> tuple[str | None, str | None]:
        """Return the stored trace and span identity as lower-case hex."""
        return (
            None if self.trace_id is None else self.trace_id.hex(),
            None if self.span_id is None else self.span_id.hex(),
        )


EXPLICIT_TRACE = "0af7651916cd43dd8448eb211c80319c"
EXPLICIT_SPAN = "b7ad6b7169203331"
SESSION = "0190f5a4-8c3e-7b21-9d4f-3a6b2c1d0e9f"
MEDIA = MediaRef(id="screenshot", kind="image", uri="s3://bucket/shot.png", media_type="image/png")
MEDIA_TEXT = (
    '[{"id":"screenshot","kind":"image","uri":"s3://bucket/shot.png","media_type":"image/png"}]'
)
FIXED_TABLES = ("vala.drift.observations", "vala.eval.observations")


def write_service_graph(root: Path) -> Path:
    """Write a Service graph of one Model, one Agent, and its artifact-bearing Prompt.

    The Model and Agent give the journey two sibling views to switch between.
    Both the Model and the Prompt carry an artifact, so each registers alone and
    the Service references it by exact identity.
    """
    digest = base64.standard_b64encode(hashlib.sha256(PROMPT_ARTIFACT).digest()).decode()
    model_digest = base64.standard_b64encode(hashlib.sha256(MODEL_ARTIFACT).digest()).decode()
    (root / "observe-prompt.txt").write_bytes(PROMPT_ARTIFACT)
    (root / "observe-model.bin").write_bytes(MODEL_ARTIFACT)
    (root / "observe-prompt.yaml").write_text(
        "apiVersion: wyrd/v1\n"
        "kind: Prompt\n"
        "metadata:\n"
        "  name: observe-prompt\n"
        "  version: 1.0.0\n"
        "  space: default\n"
        "spec:\n"
        "  provider: openai\n"
        "  model: gpt-4o\n"
        "  messages: [hello]\n"
        "artifacts:\n"
        "  - relative_path: observe-prompt.txt\n"
        f"    sha256: {digest}\n"
        f"    size_bytes: {len(PROMPT_ARTIFACT)}\n"
        "    content_type: text/plain\n",
        encoding="utf-8",
    )
    (root / "observe-model.yaml").write_text(
        "apiVersion: wyrd/v1\n"
        "kind: Model\n"
        "metadata:\n"
        "  name: observe-model\n"
        "  version: 1.0.0\n"
        "  space: default\n"
        "spec:\n"
        "  interface:\n"
        "    kind: Custom\n"
        "    meta:\n"
        "      framework_version: 0.1.0\n"
        "      loader_module: fixture\n"
        "      loader_class: TinyModel\n"
        "      extra: {}\n"
        "  task_type: Other\n"
        "  signature:\n"
        "    inputs:\n"
        "      - name: input\n"
        "        dtype: float64\n"
        "    outputs:\n"
        "      - name: output\n"
        "        dtype: float64\n"
        "  card_refs: []\n"
        "artifacts:\n"
        "  - relative_path: observe-model.bin\n"
        f"    sha256: {model_digest}\n"
        f"    size_bytes: {len(MODEL_ARTIFACT)}\n"
        "    content_type: application/octet-stream\n",
        encoding="utf-8",
    )
    (root / "observe-agent.yaml").write_text(
        "apiVersion: wyrd/v1\n"
        "kind: Agent\n"
        "metadata:\n"
        "  name: observe-agent\n"
        "  version: 1.0.0\n"
        "  space: default\n"
        "spec:\n"
        "  prompt:\n"
        "    kind: Prompt\n"
        "    name: observe-prompt\n"
        "    version: 1.0.0\n"
        "    space: default\n"
        "  run_config:\n"
        "    max_iterations: 2\n"
        "    timeout_ms: 1000\n",
        encoding="utf-8",
    )
    service = root / "observe-service.yaml"
    service.write_text(
        "apiVersion: wyrd/v1\n"
        "kind: Service\n"
        "metadata:\n"
        "  name: observe-service\n"
        "  version: 1.0.0\n"
        "  space: default\n"
        "spec:\n"
        "  service_type: agent\n"
        "  components:\n"
        "    - alias: model\n"
        "      ref:\n"
        "        kind: Model\n"
        "        name: observe-model\n"
        "        version: 1.0.0\n"
        "        space: default\n"
        "    - alias: agent\n"
        "      ref: ./observe-agent.yaml\n"
        "    - alias: prompt\n"
        "      ref:\n"
        "        kind: Prompt\n"
        "        name: observe-prompt\n"
        "        version: 1.0.0\n"
        "        space: default\n",
        encoding="utf-8",
    )
    return service


def register_dataset_table(server: WyrdTestServer, credential: str, fqn: str) -> None:
    """Register one caller-owned generic table through the public SDK."""
    writer = Bifrost(
        TableConfig(DatasetRow, fqn),
        server_url=server.base_url,
        credential=credential,
    )
    writer.register()
    writer.shutdown()


def pull_bundle(server: WyrdTestServer, credential: str, root: CardRef, bundle: Path) -> None:
    """Download the registered Service as a complete bundle through the public CLI."""
    assert root.uid is not None
    environment = os.environ.copy()
    environment.update(WYRD_SERVER_URL=server.base_url, WYRD_API_KEY=credential)
    completed = subprocess.run(
        [
            str(Path(sys.executable).with_name("wyrd")),
            "get",
            "--kind",
            "Service",
            "--uid",
            str(root.uid),
            "--output-dir",
            str(bundle),
            "--format",
            "json",
        ],
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert completed.returncode == 0, completed.stderr or completed.stdout


def assert_read_back(
    server: WyrdTestServer,
    credential: str,
    run_id: str,
    model_uid: str,
    agent_uid: str,
    datasets: tuple[str, str],
    active: tuple[str, str],
) -> None:
    """Read every emitted row back and assert its projection and correlation."""
    query = Bifrost(server_url=server.base_url, credential=credential)
    drift = query.sql(
        "SELECT series, num_value, str_value, card_uid, run_id "
        f"FROM vala.drift.observations WHERE run_id = '{run_id}' ORDER BY series",
        DriftRow,
    )
    assert [row.series for row in drift] == ["latency_ms", "tier"]
    assert drift[0].num_value == 12.5
    assert drift[0].str_value == "12.5"
    assert drift[1].num_value is None
    assert drift[1].str_value == "gold"
    assert {row.run_id for row in drift} == {run_id}
    assert {row.card_uid for row in drift} == {model_uid}

    evals = query.sql(
        "SELECT context, session_id, trace_id, span_id, media, card_uid, run_id "
        f"FROM vala.eval.observations WHERE run_id = '{run_id}'",
        EvalRow,
    )
    by_answer = {json.loads(row.context)["answer"]: row for row in evals}
    assert set(by_answer) == {"yes", "traced", "explicit"}
    assert {row.run_id for row in evals} == {run_id}
    assert {row.card_uid for row in evals} == {agent_uid}
    # No span: neither id. Active span: exactly its ids. Explicit ids win over
    # the active span.
    assert by_answer["yes"].trace_hex() == (None, None)
    assert by_answer["traced"].trace_hex() == active
    assert by_answer["explicit"].trace_hex() == (EXPLICIT_TRACE, EXPLICIT_SPAN)
    # Session and media persist exactly as authored, only where supplied.
    assert (by_answer["explicit"].session_id, by_answer["explicit"].media) == (SESSION, MEDIA_TEXT)
    assert (by_answer["yes"].session_id, by_answer["yes"].media) == (None, None)

    # Two tables, two scopes, one invocation: each row keeps the subject of the
    # view that wrote it, so a second table never inherits the first's scope.
    for table, values, subject in (
        (datasets[0], [41, 43], agent_uid),
        (datasets[1], [42], model_uid),
    ):
        rows = query.sql(
            f"SELECT value, run_id, card_uid FROM {table} WHERE run_id = '{run_id}' ORDER BY value",
            CorrelatedDatasetRow,
        )
        assert [row.value for row in rows] == values
        assert {row.run_id for row in rows} == {run_id}
        assert {row.card_uid for row in rows} == {subject}


def assert_eval_refusals(agent: Run) -> None:
    """Refuse a span without its trace and malformed media before enqueue."""
    with pytest.raises(wyrd.WyrdError) as unpaired:
        agent.observe.eval({"answer": "orphan"}, span_id=EXPLICIT_SPAN)
    assert unpaired.value.code == "WYRD_SPEC_400_VALIDATION"
    malformed = MediaRef(id="screenshot", kind="hologram", uri="s3://bucket/shot.png")  # type: ignore[arg-type]
    with pytest.raises(wyrd.WyrdError) as bad_media:
        agent.observe.eval({"answer": "bad-media"}, media=[malformed])
    assert bad_media.value.code == "WYRD_SPEC_400_VALIDATION"
    # Malformed trace identity fails visibly rather than dropping correlation.
    for trace_id, span_id in (
        ("zz" * 16, EXPLICIT_SPAN),
        (EXPLICIT_TRACE[:30], EXPLICIT_SPAN),
        (EXPLICIT_TRACE, "zz" * 8),
        (EXPLICIT_TRACE, EXPLICIT_SPAN + "00"),
    ):
        with pytest.raises(wyrd.WyrdError) as bad_trace:
            agent.observe.eval({"answer": "bad-trace"}, trace_id=trace_id, span_id=span_id)
        assert bad_trace.value.code == "WYRD_SPEC_400_VALIDATION", (trace_id, span_id)


def assert_stale_writer_fenced(server: WyrdTestServer, credential: str, dataset: str) -> None:
    """Refuse a batch built from a stale declaration of a table the run wrote.

    The writer admits the row locally; the server's schema fingerprint fence
    refuses it at flush with the stable public code, and nothing lands.
    """
    stale = Bifrost(
        TableConfig(StaleDatasetRow, dataset),
        server_url=server.base_url,
        credential=credential,
    )
    stale.insert({"value": 99, "note": "stale"})
    with pytest.raises(wyrd.WyrdError) as fenced:
        stale.flush()
    assert fenced.value.code == "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH"
    stale.shutdown()


def access_token(server: WyrdTestServer, credential: str) -> str:
    """Exchange an API key for a Wyrd access token through the public token route."""
    request = urllib.request.Request(
        f"{server.base_url}/auth/token",
        data=urllib.parse.urlencode(
            {
                "grant_type": "urn:ietf:params:oauth:grant-type:token-exchange",
                "subject_token": credential,
                "subject_token_type": "urn:wyrd:oauth:token-type:api_key",
            }
        ).encode(),
        headers={"content-type": "application/x-www-form-urlencoded"},
        method="POST",
    )
    with urllib.request.urlopen(request) as response:
        return json.loads(response.read())["access_token"]


def otlp_provider(server: WyrdTestServer, credential: str) -> TracerProvider:
    """A stock OpenTelemetry SDK provider exporting OTLP/HTTP to ``/v1/traces``.

    The provider is private, as an agent framework's often is, so the journey
    registers Wyrd's correlation processor through the explicit hook.
    """
    provider = TracerProvider()
    provider.add_span_processor(
        BatchSpanProcessor(
            OTLPSpanExporter(
                endpoint=f"{server.base_url}/v1/traces",
                headers={"x-wyrd-access-token": f"Bearer {access_token(server, credential)}"},
            )
        )
    )
    assert install_run_correlation(provider) is True
    return provider


def emit_framework_scope(
    state: WyrdState, provider: TracerProvider, dataset: str
) -> tuple[Run, tuple[str, str]]:
    """Run framework-style code inside ``with state.run(card="agent")``.

    The spans carry no Wyrd attributes of their own; the scope supplies them.
    The custom row and the Eval observation pass no trace identity: the Eval
    row takes the active tool span's ids. Returns the run and those ids.
    """
    tracer = provider.get_tracer("framework")
    with state.run(card="agent") as agent_run:
        with (
            tracer.start_as_current_span("agent.invoke"),
            tracer.start_as_current_span("agent.tool") as tool,
        ):
            ids = tool.get_span_context()
            agent_run.observe.record(dataset, {"value": 44})
            agent_run.observe.eval({"answer": "scoped"})
    return agent_run, (f"{ids.trace_id:032x}", f"{ids.span_id:016x}")


def emit_nested_scopes(state: WyrdState, provider: TracerProvider) -> tuple[Run, dict[str, Run]]:
    """Create framework spans across nested, async, and concurrent Run scopes.

    ``pre.active`` is current before the root scope is entered, so entry stamps
    it. The Model scope nests inside the root and restores it on exit. Inside
    ``asyncio.run``, one span follows an ``await``, two concurrent tasks share
    one immutable Agent view, and a task created inside the Model scope runs
    after that scope exits. ``nested.outside`` starts after every scope.
    Returns the root run and the view each named span must carry.
    """
    tracer = provider.get_tracer("framework")
    run = state.run()
    model = run.for_card("model")
    agent = run.for_card("agent")

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
    tracer.start_span("nested.outside").end()
    views = {"pre.active": run, "root.before": run, "root.restored": run, "async.await": run}
    views |= {"model.inner": model, "async.spawned": model}
    views |= {"async.concurrent.a": agent, "async.concurrent.b": agent}
    return run, views


def assert_nested_scopes(
    server: WyrdTestServer, credential: str, run: Run, views: dict[str, Run]
) -> None:
    """Prove every nested and async span persisted with its view's exact correlation."""
    query = Bifrost(server_url=server.base_url, credential=credential)
    spans = (
        query.sql(
            "SELECT name, attributes, run_id, card_uid "
            f"FROM vala.traces.spans WHERE run_id = '{run.run_id}' ORDER BY name"
        )
        .to_arrow()
        .to_pylist()
    )
    assert {
        row["name"]: (asserted_card_ref(row["attributes"]), row["card_uid"]) for row in spans
    } == {name: (view.card_ref, view.card_ref.split("#", 1)[1]) for name, view in views.items()}, (
        "one run id across every scope, each span under its own view's Card"
    )
    outside = (
        query.sql("SELECT run_id, card_uid FROM vala.traces.spans WHERE name = 'nested.outside'")
        .to_arrow()
        .to_pylist()
    )
    assert outside == [{"run_id": None, "card_uid": None}], "no scope outlives its block"


def asserted_card_ref(attributes: bytes) -> str:
    """Read the ``wyrd.card_ref`` a span asserted from its persisted attribute payload."""
    values = {
        item.key: item.value.string_value for item in KeyValueList.FromString(attributes).values
    }
    return values["wyrd.card_ref"]


def assert_scope_joins(
    server: WyrdTestServer,
    credential: str,
    agent_run: Run,
    agent_uid: str,
    dataset: str,
    tool: tuple[str, str],
) -> None:
    """Prove the persisted span, custom, and Eval rows join on the Run scope."""
    query = Bifrost(server_url=server.base_url, credential=credential)
    run_id = agent_run.run_id
    spans = (
        query.sql(
            "SELECT name, attributes, run_id, card_uid, principal_id "
            f"FROM vala.traces.spans WHERE run_id = '{run_id}' ORDER BY name"
        )
        .to_arrow()
        .to_pylist()
    )
    assert [row["name"] for row in spans] == ["agent.invoke", "agent.tool"]
    # The asserted CardRef is not a column: Scribe resolves it to `card_uid`
    # and the lossless attribute payload keeps exactly what the client sent.
    assert {
        (asserted_card_ref(row["attributes"]), row["run_id"], row["card_uid"]) for row in spans
    } == {(agent_run.card_ref, run_id, agent_uid)}
    publishers = {row["principal_id"] for row in spans}
    assert len(publishers) == 1 and None not in publishers, "one authenticated publisher"
    (publisher,) = publishers

    custom = (
        query.sql(
            "SELECT d.value, d.card_uid, d.principal_id, s.name "
            f"FROM {dataset} d JOIN vala.traces.spans s ON d.run_id = s.run_id "
            f"WHERE d.run_id = '{run_id}' ORDER BY s.name"
        )
        .to_arrow()
        .to_pylist()
    )
    assert custom == [
        {"value": 44, "card_uid": agent_uid, "principal_id": publisher, "name": name}
        for name in ("agent.invoke", "agent.tool")
    ]

    evals = (
        query.sql(
            "SELECT e.context, e.trace_id, e.span_id, e.run_id, e.card_uid, s.name "
            "FROM vala.eval.observations e JOIN vala.traces.spans s "
            "ON e.trace_id = s.trace_id AND e.span_id = s.span_id "
            f"WHERE e.run_id = '{run_id}'"
        )
        .to_arrow()
        .to_pylist()
    )
    assert len(evals) == 1
    (joined,) = evals
    assert json.loads(joined["context"]) == {"answer": "scoped"}
    assert (joined["trace_id"].hex(), joined["span_id"].hex()) == tool
    assert (joined["run_id"], joined["card_uid"], joined["name"]) == (
        run_id,
        agent_uid,
        "agent.tool",
    )


@pytest.mark.integration
def test_scoped_run_emits_drift_eval_and_generic_rows(tmp_path: Path) -> None:
    """One invocation emits Drift, Eval, and generic rows correlated to their subjects."""
    # A dedicated server: publication would retire the staged describe
    # decisions this journey counts.
    with WyrdTestServer(audit_publication=False) as server:
        run_journey(tmp_path, server)


def run_journey(tmp_path: Path, server: WyrdTestServer) -> None:
    """Drive the whole scoped-observation journey against one server."""
    service = write_service_graph(tmp_path)
    bundle = tmp_path / "bundle"
    admin = server.bootstrap_service(["admin"], name=f"py-observe-{uuid4().hex[:12]}")
    dataset = f"vala.datasets.observe_py_{uuid4().hex}"
    dataset_b = f"vala.datasets.observe_py_{uuid4().hex}"
    register_dataset_table(server, admin, dataset)
    register_dataset_table(server, admin, dataset_b)

    cards = Cards(server_url=server.base_url, credential=admin)
    cards.register_from_path(str(tmp_path / "observe-prompt.yaml"))
    cards.register_from_path(str(tmp_path / "observe-model.yaml"))
    root = cards.register_from_path(str(service)).root
    pull_bundle(server, admin, root, bundle)
    credential = server.credential_registered_service(
        f"{root.space}/Service/{root.name}@{root.version}", []
    )

    state = WyrdState.from_path(bundle, interfaces={"model": NoopModelInterface()})
    state.start_bifrost(server_url=server.base_url, credential=credential)
    fixed_describes = [server.table_describe_count(table) for table in FIXED_TABLES]

    run = state.run()
    model = run.for_card("model")
    agent = run.for_card("agent")
    assert model.run_id == run.run_id == agent.run_id
    model_uid = model.card_ref.split("#", 1)[1]
    agent_uid = agent.card_ref.split("#", 1)[1]

    model.observe.drift({"latency_ms": 12.5, "tier": "gold"})
    agent.observe.eval({"answer": "yes"})
    tracer = TracerProvider().get_tracer("wyrd.tests.observe")
    with tracer.start_as_current_span("observe-journey") as span:
        span_context = span.get_span_context()
        active = (f"{span_context.trace_id:032x}", f"{span_context.span_id:016x}")
        agent.observe.eval({"answer": "traced"})
        agent.observe.eval(
            {"answer": "explicit"},
            session_id=SESSION,
            media=[MEDIA],
            trace_id=EXPLICIT_TRACE,
            span_id=EXPLICIT_SPAN,
        )
    agent.observe.record(dataset, {"value": 41})
    agent.observe.record(dataset, {"value": 43})
    assert server.table_describe_count(dataset) == 1, "the repeated write reused its describe"
    model.observe.record(dataset_b, {"value": 42})

    assert_eval_refusals(agent)
    with pytest.raises(wyrd.WyrdError) as reserved:
        agent.observe.record("vala.drift.observations", {"x": 1})
    assert reserved.value.code == "WYRD_SDK_400_INVALID_OBSERVATION"
    with pytest.raises(wyrd.WyrdError) as absent:
        agent.observe.record(f"vala.datasets.absent_{uuid4().hex}", {"value": 1})
    assert absent.value.code == "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND"
    with pytest.raises(wyrd.WyrdError) as unknown:
        run.for_card("missing")
    assert unknown.value.code == "WYRD_SDK_404_UNKNOWN_ALIAS"
    assert [server.table_describe_count(table) for table in FIXED_TABLES] == fixed_describes, (
        "Drift and Eval emits perform no per-observation schema IO"
    )

    provider = otlp_provider(server, credential)
    agent_run, tool = emit_framework_scope(state, provider, dataset)
    assert agent_run.run_id != run.run_id
    nested_run, views = emit_nested_scopes(state, provider)
    # Leaving the scope is not a durability barrier: flush spans, drain the
    # writer, then wait for publication before reading anything back.
    assert provider.force_flush()

    state.shutdown()
    state.shutdown()
    server.flush_bifrost()

    assert_read_back(server, admin, run.run_id, model_uid, agent_uid, (dataset, dataset_b), active)
    assert_scope_joins(server, admin, agent_run, agent_uid, dataset, tool)
    assert_nested_scopes(server, admin, nested_run, views)
    assert_stale_writer_fenced(server, admin, dataset)
    server.flush_bifrost()
    stale_rows = query_values(server, admin, dataset)
    assert 99 not in stale_rows, "a fenced stale batch never lands"
    provider.shutdown()


def query_values(server: WyrdTestServer, credential: str, table: str) -> list[int]:
    """Read every caller-owned ``value`` in ``table``."""
    query = Bifrost(server_url=server.base_url, credential=credential)
    return [row["value"] for row in query.sql(f"SELECT value FROM {table}").to_arrow().to_pylist()]


BURST_OBSERVATIONS = 1_000
BURST_FEATURES = 9
UNSEALABLE_BUDGET = 1024
BURST_BUDGET = 16 * 1024 * 1024


def emit_with_resubmit(state: WyrdState, view: Run, features: dict[str, float]) -> None:
    """Emit one Drift observation, flushing and resubmitting it on ``QUEUE_FULL``.

    Admission is all-or-none per observation, so a refused observation admitted
    no row and resubmitting it after the flush duplicates nothing.
    """
    while True:
        try:
            view.observe.drift(features)
            return
        except wyrd.WyrdError as error:
            if error.code != "WYRD_CLIENT_429_QUEUE_FULL":
                raise
            state.flush()


@pytest.mark.integration
def test_drift_burst_survives_a_byte_budget_override(tmp_path: Path) -> None:
    """A 1,000 x 9 Drift burst through a budget override lands exactly once per row."""
    with WyrdTestServer() as server:
        service = write_service_graph(tmp_path)
        bundle = tmp_path / "bundle"
        admin = server.bootstrap_service(["admin"], name=f"py-burst-{uuid4().hex[:12]}")
        cards = Cards(server_url=server.base_url, credential=admin)
        cards.register_from_path(str(tmp_path / "observe-prompt.yaml"))
        cards.register_from_path(str(tmp_path / "observe-model.yaml"))
        root = cards.register_from_path(str(service)).root
        pull_bundle(server, admin, root, bundle)
        credential = server.credential_registered_service(
            f"{root.space}/Service/{root.name}@{root.version}", []
        )
        state = WyrdState.from_path(bundle, interfaces={"model": NoopModelInterface()})

        with pytest.raises(wyrd.WyrdError) as unsealable:
            state.start_bifrost(
                server_url=server.base_url,
                credential=credential,
                client_byte_limit_bytes=UNSEALABLE_BUDGET,
            )
        assert unsealable.value.code == "WYRD_CLIENT_400_CONFIG_INVALID"
        state.start_bifrost(
            server_url=server.base_url,
            credential=credential,
            client_byte_limit_bytes=BURST_BUDGET,
        )

        run = state.run()
        model = run.for_card("model")
        for observation in range(BURST_OBSERVATIONS):
            emit_with_resubmit(
                state,
                model,
                {
                    f"feature_{feature}": observation + feature / 10
                    for feature in range(BURST_FEATURES)
                },
            )
        state.shutdown()
        server.flush_bifrost()

        counts = (
            Bifrost(server_url=server.base_url, credential=admin)
            .sql(
                "SELECT COUNT(*) AS n FROM vala.drift.observations "
                f"WHERE run_id = '{run.run_id}' GROUP BY record_id"
            )
            .to_arrow()
            .column("n")
            .to_pylist()
        )
        assert len(counts) == BURST_OBSERVATIONS, "one record_id per observation"
        assert set(counts) == {BURST_FEATURES}, "every observation landed every feature once"
        assert sum(counts) == BURST_OBSERVATIONS * BURST_FEATURES
