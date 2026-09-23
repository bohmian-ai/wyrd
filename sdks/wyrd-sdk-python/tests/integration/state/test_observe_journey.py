"""Python scoped-observation journey through the public ``wyrd`` package.

One invocation switches from the root Service to its Model and Agent views,
emits a Drift mapping and an Eval context, writes one row into a
caller-registered generic table, drains at graceful shutdown, and reads every
row back by the exact subject Card UID and the one invocation id. Startup is
first refused once per fixed table whose describe the server fails, and the
server's staged describe decisions prove repeated writes reuse cached schemas.
"""

from __future__ import annotations

import base64
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
from uuid import uuid4

import pytest
import wyrd
from opentelemetry.sdk.trace import TracerProvider
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, TableConfig
from wyrd.cards import CardRef, Cards
from wyrd.eval import MediaRef
from wyrd.model import ModelInterface
from wyrd.observe import Run
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


def assert_fixed_table_preflight_refusals(
    server: WyrdTestServer, state: WyrdState, credential: str
) -> None:
    """Refuse startup once per fixed table whose describe the server fails.

    Drift fails first; Eval fails after Drift already described. Each refusal
    carries the server's stable code and leaves the state startable.
    """
    for table in FIXED_TABLES:
        server.fail_table_describe(table)
        try:
            with pytest.raises(wyrd.WyrdError) as refused:
                state.start_bifrost(server_url=server.base_url, credential=credential)
        finally:
            server.restore_table_describe()
        assert refused.value.code == "WYRD_VALA_500_AUDIT_UNAVAILABLE", table


def assert_eval_refusals(agent: Run) -> None:
    """Refuse a span without its trace and malformed media before enqueue."""
    with pytest.raises(wyrd.WyrdError) as unpaired:
        agent.observe.eval({"answer": "orphan"}, span_id=EXPLICIT_SPAN)
    assert unpaired.value.code == "WYRD_SPEC_400_VALIDATION"
    malformed = MediaRef(id="screenshot", kind="hologram", uri="s3://bucket/shot.png")  # type: ignore[arg-type]
    with pytest.raises(wyrd.WyrdError) as bad_media:
        agent.observe.eval({"answer": "bad-media"}, media=[malformed])
    assert bad_media.value.code == "WYRD_SPEC_400_VALIDATION"


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
        f"{root.space}/Service/{root.name}@{root.version}", ["admin"]
    )

    state = WyrdState.from_path(bundle, interfaces={"model": NoopModelInterface()})
    assert_fixed_table_preflight_refusals(server, state, credential)
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

    state.shutdown()
    state.shutdown()
    server.flush_bifrost()

    assert_read_back(server, admin, run.run_id, model_uid, agent_uid, (dataset, dataset_b), active)
