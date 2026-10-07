"""Python Drift verification journey through the public ``wyrd`` package.

Registers Pandas, Polars, and Arrow baseline Data Cards, each saved as Parquet,
fits a PSI Verifier from each and an SPC Verifier from the Polars baseline, and
reads every baseline's Card status until it is ready. A Service binds all four
Verifiers, and only its SPC binding names an HTTP Operator delivered to a local
receiver. As its own principal the Service emits Drift observations through
``WyrdState`` and judges the same rows directly with each Verifier through
``run.observe.verify``. Each binding's due scheduled occurrence then fails,
persisting its result, SPC X-bar/S evidence, and feature rows, and the SPC
binding delivers its Operator. Negative flows cover an Arrow IPC baseline
refused as non-Parquet, an SPC profile carrying the retired ``weco_rule``
field, and a caller without ``evals:run``.

A second journey registers one Service whose Model carries PSI, SPC, and Custom
Drift bindings and whose Agent carries a deterministic and a local LLM-judge
Eval binding. As its own Card-bound principal the Service emits typed and
mapping Drift and Eval observations through one ``state.run()`` that switches
between the Model and Agent views; Scribe acknowledges them, each Eval
observation runs both Eval bindings, and each due Drift binding scores its
method. The failed PSI binding delivers its HTTP Operator to a local receiver,
and the Model view judges the same rows directly with the PSI Verifier.
Mapping, dataclass, and Pydantic payloads persist identical tall Drift rows,
and registering a retired ``kind: Drift`` or ``kind: Eval`` Card is refused.
"""

from __future__ import annotations

import base64
import hashlib
import json
import os
import subprocess
import sys
import threading
import time
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import pandas as pd
import polars as pl
import pyarrow as pa
import pytest
import yaml
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.data import ArrowInterface, DataCard, PandasInterface, PolarsInterface
from wyrd.model import ModelInterface
from wyrd.observe import Judgment, Run
from wyrd.prompt import Prompt
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

WAIT_SECONDS = 90
ROWS = 100
LATENCY = [float(row) for row in range(ROWS)]


def verifier_yaml(name: str, baseline: CardRef, method: str) -> str:
    """Build a Distribution Drift Verifier over ``baseline`` for ``latency``."""
    profile = {
        "Psi": (
            "        kind: Psi\n"
            "        binning_strategy:\n          kind: EqualWidth\n          n_bins: 10\n"
            "        threshold:\n          kind: Fixed\n          value: 0.25\n"
        ),
        "Spc": "        kind: Spc\n        sample_size: 5\n",
    }[method]
    return (
        f"apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n"
        "  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n"
        f"    spec:\n      method: {method}\n      signal:\n        kind: Distribution\n"
        f"        baseline_ref:\n          kind: Data\n          name: {baseline.name}\n"
        f"          version: {baseline.version}\n          space: {baseline.space}\n"
        "        features: [latency]\n      condition:\n        kind: Statistical\n"
        f"      profile:\n{profile}"
    )


def service_yaml(hook: str) -> str:
    """Build a Service whose root binds every baseline's Verifier on a daily schedule.

    Only the SPC binding names an Operator: an HTTP POST to ``hook``.
    """
    bindings = "".join(
        f"    - verifier: {{kind: Verifier, name: {name}, version: 1.0.0, space: default}}\n"
        '      runs_on: {kind: schedule, cron: "0 0 * * *"}\n'
        for name in ("py-drift-pandas", "py-drift-polars", "py-drift-arrow")
    )
    return (
        "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: py-drift-service\n"
        f"  version: 1.0.0\n  space: default\nspec:\n  verified_by:\n{bindings}"
        "    - verifier: {kind: Verifier, name: py-drift-spc, version: 1.0.0, space: default}\n"
        '      runs_on: {kind: schedule, cron: "0 0 * * *"}\n'
        f"      on_failure:\n        - {{kind: http, method: post, url: '{hook}'}}\n"
    )


def download(server: WyrdTestServer, credential: str, kind: str, uid: str, bundle: Path) -> dict:
    """Download one Card through the public CLI and return its served status.

    The hydrated bundle writes the root's served envelope, status included, to
    ``cards/root/card.yaml``.
    """
    environment = os.environ.copy()
    environment.update(WYRD_SERVER_URL=server.base_url, WYRD_API_KEY=credential)
    completed = subprocess.run(
        [
            str(Path(sys.executable).with_name("wyrd")),
            "get",
            "--kind",
            kind,
            "--uid",
            uid,
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
    json.loads(completed.stdout)
    card = yaml.safe_load((bundle / "cards" / "root" / "card.yaml").read_text(encoding="utf-8"))
    assert card.get("status") is not None, f"{kind} {uid} serves status"
    return card["status"]


def wait_ready(server: WyrdTestServer, credential: str, verifier: CardRef, root: Path) -> None:
    """Poll a Verifier's Card status until its fitted baseline is ready."""
    deadline = time.monotonic() + WAIT_SECONDS
    attempt = 0
    while True:
        attempt += 1
        bundle = root / f"{verifier.name}-{attempt}"
        baseline = download(server, credential, "Verifier", str(verifier.uid), bundle)[
            "verification"
        ]["baseline"]
        assert baseline["state"] in {"pending", "building", "ready"}, baseline
        if baseline["state"] == "ready":
            return
        assert time.monotonic() < deadline, f"{verifier.name} never fitted: {baseline}"
        time.sleep(0.2)


def client_environment(monkeypatch: pytest.MonkeyPatch, root: Path) -> None:
    """Clear every ambient credential that would outrank an API key, and any saved login.

    ``observe.verify`` resolves its client from the environment on first use
    when Bifrost is not started. The entered ``WyrdTestServer`` publishes
    ``WYRD_SERVER_URL`` and owns ``WYRD_API_KEY``, restoring both on exit.
    """
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(root / "config"))
    for name in ("WYRD_ACCESS_TOKEN", "WYRD_WORKLOAD_TOKEN"):
        monkeypatch.delenv(name, raising=False)


def verify_as(bundle: Path, credential: str, verifier: str, input: object) -> Judgment:
    """Judge ``input`` with bound ``verifier`` from ``bundle``'s root Run view as ``credential``.

    Sets the harness-owned ``WYRD_API_KEY`` (see ``client_environment``); a
    fresh ``WyrdState`` resolves its client from it on this first judgment.
    """
    os.environ["WYRD_API_KEY"] = credential
    return WyrdState.from_path(bundle).run().observe.verify(verifier, input)


def await_rows(server: WyrdTestServer, query: Bifrost, sql: str, count: int) -> list[dict]:
    """Flush Scribe and rerun ``sql`` until it returns ``count`` rows, and return them.

    A result table the tenant has not written yet reads as no rows.
    """
    deadline = time.monotonic() + WAIT_SECONDS
    while True:
        server.flush_bifrost()
        try:
            found = query.sql(sql).to_arrow().to_pylist()
        except WyrdError as error:
            assert error.code == "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND", error
            found = []
        if len(found) >= count:
            assert len(found) == count, found
            return found
        assert time.monotonic() < deadline, f"only {len(found)} of {count} rows: {found}"
        time.sleep(0.2)


def await_hooks(received: list[tuple[str, dict]], count: int) -> None:
    """Wait until the local receiver holds ``count`` Operator deliveries to ``/hook``."""
    deadline = time.monotonic() + WAIT_SECONDS
    while len(hooks := [path for path, _ in received if path == "/hook"]) < count:
        assert time.monotonic() < deadline, f"only {len(hooks)} of {count} Operator deliveries"
        time.sleep(0.1)
    assert len(hooks) == count, received


def assert_spc_evidence(details: str, subgroups: int, x_bar_signals: int) -> None:
    """Assert the persisted SPC evidence of ``latency`` in a result's ``details``.

    The baseline's twenty subgroups of five consecutive integers fix the X-bar
    center at 49.5 and the S center at ``sqrt(2.5)``; the limits must be the
    NIST X-bar/S limits around them, and SPC compares signals with zero.
    """
    feature = json.loads(details)["features"]["latency"]
    spc = feature["evidence"]["Spc"]
    assert (spc["subgroup_size"], spc["subgroups"]) == (5, subgroups), spc
    assert spc["x_bar"]["signals"] == x_bar_signals, spc
    s_bar = 2.5**0.5
    width = 3 * s_bar / (0.9399856 * 5**0.5)
    assert abs(spc["x_bar"]["center"] - 49.5) < 1e-9, spc
    assert abs(spc["x_bar"]["upper"] - (49.5 + width)) < 1e-5, spc
    assert abs(spc["s"]["center"] - s_bar) < 1e-9, spc
    assert spc["s"]["lower"] == 0.0, "B3 is zero for subgroups of five"
    assert feature["threshold"] == 0.0, feature


def register_baselines(cards: Cards) -> dict[str, CardRef]:
    """Register the same baseline through each Parquet-backed authoring path."""
    tables = {
        "pandas": PandasInterface(data=pd.DataFrame({"latency": LATENCY})),
        "polars": PolarsInterface(data=pl.DataFrame({"latency": LATENCY})),
        "arrow": ArrowInterface(data=pa.table({"latency": LATENCY}), format="parquet"),
    }
    baselines = {}
    for kind, interface in tables.items():
        card = DataCard(interface, space="default", name=f"py-drift-{kind}-data", version="1.0.0")
        baselines[kind] = cards.data.register(card).root
    return baselines


@pytest.mark.integration
def test_parquet_baselines_fit_and_score_drift_server_side(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Pandas, Polars, and Arrow baselines fit, and each Verifier scores server-side."""
    with (
        local_upstream() as (upstream, received),
        WyrdTestServer(verification_runtime=True) as server,
    ):
        admin = server.bootstrap_service(["admin"], name="py-drift-admin")
        cards = Cards(server_url=server.base_url, credential=admin)
        baselines = register_baselines(cards)
        verifiers = {}
        for name, kind, method in [
            ("py-drift-pandas", "pandas", "Psi"),
            ("py-drift-polars", "polars", "Psi"),
            ("py-drift-arrow", "arrow", "Psi"),
            ("py-drift-spc", "polars", "Spc"),
        ]:
            path = tmp_path / f"{name}.yaml"
            path.write_text(verifier_yaml(name, baselines[kind], method), encoding="utf-8")
            verifiers[name] = cards.register_from_path(str(path)).root
        for verifier in verifiers.values():
            wait_ready(server, admin, verifier, tmp_path / "status")

        (tmp_path / "service.yaml").write_text(service_yaml(f"{upstream}/hook"), encoding="utf-8")
        service = cards.register_from_path(str(tmp_path / "service.yaml")).root
        credential = server.credential_registered_service(
            f"{service.space}/Service/{service.name}@{service.version}", ["admin"]
        )
        bundle = tmp_path / "service"
        binding_ids = download(server, admin, "Service", str(service.uid), bundle)["verification"][
            "binding_ids"
        ]
        assert len(binding_ids) == 4, binding_ids
        state = WyrdState.from_path(bundle)
        state.start_bifrost(server_url=server.base_url, credential=credential)
        run = state.run()
        shifted = [{"latency": 150.0 + row} for row in range(120)]
        for row in shifted:
            run.observe.drift(row)
        for name, verifier in verifiers.items():
            judgment = run.observe.verify(name, shifted)
            assert judgment.verdict == "failed", (name, judgment.summary)
            assert (str(judgment.verifier.uid), str(judgment.subject.uid)) == (
                str(verifier.uid),
                str(service.uid),
            ), name
            features = judgment.detail["drift"]["features"]
            assert {feature: report["verdict"] for feature, report in features.items()} == {
                "latency": "Drift"
            }, name
            if name == "py-drift-spc":
                assert_spc_evidence(json.dumps(judgment.detail["drift"]), 24, 24)
        state.shutdown()

        for binding_id in binding_ids:
            server.make_binding_due(binding_id)
        query = Bifrost(server_url=server.base_url, credential=admin)
        scheduled = await_rows(
            server,
            query,
            "SELECT r.execution_status, r.verdict, r.binding_id, r.subject_card_uid, r.details, "
            "f.feature, f.method, f.verdict AS feature_verdict "
            "FROM vala.verification.results r JOIN vala.drift.result_features f "
            "ON r.result_id = f.result_id "
            f"WHERE r.owner_card_uid = '{service.uid}' ORDER BY f.method",
            4,
        )
        assert sorted(result["binding_id"] for result in scheduled) == sorted(binding_ids)
        assert [result["method"] for result in scheduled] == ["Psi", "Psi", "Psi", "Spc"]
        for result in scheduled:
            assert (result["execution_status"], result["verdict"]) == ("completed", "failed")
            assert result["subject_card_uid"] == str(service.uid), result
            assert (result["feature"], result["feature_verdict"]) == ("latency", "drift"), result
        assert_spc_evidence(scheduled[-1]["details"], 24, 24)
        await_hooks(received, 1)

        ipc = DataCard(
            ArrowInterface(data=pa.table({"latency": LATENCY}), format="ipc"),
            space="default",
            name="py-drift-ipc-data",
            version="1.0.0",
        )
        ipc_ref = cards.data.register(ipc).root
        (tmp_path / "ipc.yaml").write_text(
            verifier_yaml("py-drift-ipc", ipc_ref, "Psi"), encoding="utf-8"
        )
        with pytest.raises(WyrdError) as refused:
            cards.register_from_path(str(tmp_path / "ipc.yaml"))
        assert refused.value.code == "WYRD_DRIFT_400_VALIDATION"

        retired = tmp_path / "py-drift-weco.yaml"
        retired.write_text(
            verifier_yaml("py-drift-weco", baselines["polars"], "Spc").replace(
                "sample_size: 5\n",
                'sample_size: 5\n        weco_rule:\n          rule_string: "8 16 4 8 2 4 1 1"\n',
            ),
            encoding="utf-8",
        )
        with pytest.raises(WyrdError) as legacy:
            cards.register_from_path(str(retired))
        assert "weco_rule" in str(legacy.value.details), legacy.value.details

        client_environment(monkeypatch, tmp_path)
        reader = server.bootstrap_service(["reader"], name="py-drift-reader")
        with pytest.raises(WyrdError) as denied:
            verify_as(bundle, reader, "py-drift-pandas", shifted)
        assert denied.value.status == 403


# The Custom Drift method block: the per-row mean of ``score`` against 1.0, alerting past 0.5.
CUSTOM_DRIFT = (
    "      method: Custom\n      signal:\n        kind: Metric\n        name: score\n"
    "      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n"
    "        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n"
)


def subject_credential(server: WyrdTestServer, service: CardRef) -> str:
    """Issue the subject Service's own key, whose Card scope covers judging it."""
    return server.credential_registered_service(
        f"{service.space}/Service/{service.name}@{service.version}", ["admin"]
    )


MODEL_ARTIFACT = b"py-bound-model-artifact"

JUDGE_REPLY = {
    "id": "chatcmpl_py_eval",
    "object": "chat.completion",
    "created": 1_700_000_000,
    "model": "gpt-test",
    "choices": [
        {
            "index": 0,
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": '{"passed":true}'},
        }
    ],
    "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8},
}


@contextmanager
def local_upstream() -> Iterator[tuple[str, list[tuple[str, dict]]]]:
    """Serve the judge provider and the Operator hook from one loopback server.

    ``POST /v1/chat/completions`` answers every judge call with a passing
    verdict; ``POST /hook`` accepts an Operator delivery. Yields the root URL
    and the ``(path, JSON body)`` of every request received, in arrival order.
    """
    received: list[tuple[str, dict]] = []

    class Handler(BaseHTTPRequestHandler):
        """Record one JSON POST and answer the judge or the hook."""

        def do_POST(self) -> None:
            body = json.loads(self.rfile.read(int(self.headers["content-length"] or 0)) or b"{}")
            received.append((self.path, body))
            reply = json.dumps(JUDGE_REPLY if self.path.endswith("/chat/completions") else {})
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.end_headers()
            self.wfile.write(reply.encode())

        def log_message(self, *args: object) -> None:
            """Keep the test output free of per-request access lines."""

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_address[1]}", received
    finally:
        server.shutdown()
        thread.join()


class NoopModelInterface(ModelInterface):
    """Stand-in loader for the fixture Model, which is observed and never run."""

    def __init__(self) -> None:
        """Start with the empty holder slot the Model holder expects after load."""
        super().__init__()
        self.model: object = None

    def save(self, path: Path, save_kwargs: dict[str, object] | None = None) -> None:
        """Never called: the journey registers YAML, it does not save a Model."""
        raise NotImplementedError

    def load(self, path: Path, load_kwargs: dict[str, object] | None = None) -> None:
        """Read nothing and publish a deterministic stand-in model."""
        self.model = lambda value: value


@dataclass
class Features:
    """The typed Drift payload: PSI/SPC ``latency`` and the Custom ``score``."""

    latency: float
    score: float


@dataclass
class Exchange:
    """The typed Eval context both Eval Verifiers read."""

    answer: str


@dataclass
class Shape:
    """One payload of every scalar kind, authored as a dataclass."""

    latency: float
    tier: str
    count: int
    cached: bool


class ShapeModel(BaseModel):
    """The same payload as ``Shape``, authored as a Pydantic model."""

    latency: float
    tier: str
    count: int
    cached: bool


def eval_verifier(name: str, tasks: str) -> str:
    """Build an Eval Verifier whose ``all_pass`` gate covers ``tasks``."""
    return (
        f"apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n"
        "  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: eval\n"
        f"    spec:\n      pass_gate: {{kind: all_pass}}\n      tasks:\n{tasks}"
    )


def write_bound_graph(root: Path, baseline: CardRef, hook: str) -> Path:
    """Write a Service whose Model and Agent carry all five verification bindings.

    The Model is the subject of the PSI, SPC, and Custom Drift bindings; only
    PSI names an Operator, an HTTP POST to ``hook``. The Agent is the subject
    of a deterministic and an LLM-judge Eval binding, both activated by
    ``observations_ready``. The judge Prompt is native OpenAI Chat with a
    JSON-schema response built through the public ``Prompt`` builder. The
    artifact-bearing Model registers alone from ``model.yaml`` first, so the
    Service names it by exact identity.
    """
    (root / "model.bin").write_bytes(MODEL_ARTIFACT)
    judge = Prompt.openai_chat(
        "gpt-test",
        messages=["Grade the answer ${answer}."],
        variables=["answer"],
        output={"passed": bool},
    )
    card = {
        "apiVersion": "wyrd/v1",
        "kind": "Prompt",
        "metadata": {"name": "py-bound-judge", "version": "1.0.0", "space": "default"},
        "spec": json.loads(judge.model_dump_json()),
    }
    files = {
        "judge-prompt.json": json.dumps(card),
        "agent-prompt.yaml": (
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: py-bound-agent-prompt\n"
            "  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n"
            "  model: gpt-test\n  messages: [answer the question]\n"
        ),
        "agent.yaml": (
            "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: py-bound-agent\n"
            "  version: 1.0.0\n  space: default\nspec:\n  prompt: ./agent-prompt.yaml\n"
            "  run_config:\n    max_iterations: 1\n"
        ),
        "model.yaml": (
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: py-bound-model\n"
            "  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n"
            "    meta:\n      framework_version: 0.1.0\n      loader_module: fixture\n"
            "      loader_class: TinyModel\n      extra: {}\n  task_type: Other\n"
            "  signature:\n    inputs:\n      - name: latency\n        dtype: float64\n"
            "    outputs:\n      - name: score\n        dtype: float64\n  card_refs: []\n"
            "artifacts:\n  - relative_path: model.bin\n"
            f"    sha256: {base64.standard_b64encode(hashlib.sha256(MODEL_ARTIFACT).digest()).decode()}\n"
            f"    size_bytes: {len(MODEL_ARTIFACT)}\n"
            "    content_type: application/octet-stream\n"
        ),
        "psi.yaml": verifier_yaml("py-bound-psi", baseline, "Psi"),
        "spc.yaml": verifier_yaml("py-bound-spc", baseline, "Spc"),
        "custom.yaml": (
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: py-bound-custom\n"
            "  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n"
            f"    spec:\n{CUSTOM_DRIFT}"
        ),
        "eval-assert.yaml": eval_verifier(
            "py-bound-assert",
            "        answer: {kind: assertion, id: answer, context_path: $.answer, "
            'operator: equals, expected: "yes"}\n',
        ),
        "eval-judge.yaml": eval_verifier(
            "py-bound-judge-eval",
            "        judge:\n          kind: llm_judge\n          id: judge\n"
            "          judge_ref: {prompt: ./judge-prompt.json, tool_names: [], "
            "run_config: {max_iterations: 1}}\n"
            "          context_path: $.answer\n          operator: equals\n"
            "          expected: {passed: true}\n          max_retries: 0\n",
        ),
        "service.yaml": (
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: py-bound-service\n"
            "  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n"
            "  components:\n"
            "    - alias: model\n      ref:\n        kind: Model\n        name: py-bound-model\n"
            "        version: 1.0.0\n        space: default\n      verified_by:\n"
            "        - verifier: ./psi.yaml\n"
            '          runs_on: {kind: schedule, cron: "0 0 * * *"}\n'
            f"          on_failure:\n            - {{kind: http, method: post, url: '{hook}'}}\n"
            "        - verifier: ./spc.yaml\n"
            '          runs_on: {kind: schedule, cron: "0 0 * * *"}\n'
            "        - verifier: ./custom.yaml\n"
            '          runs_on: {kind: schedule, cron: "0 0 * * *"}\n'
            "    - alias: agent\n      ref: ./agent.yaml\n      verified_by:\n"
            "        - verifier: ./eval-assert.yaml\n          runs_on: {kind: observations_ready}\n"
            "        - verifier: ./eval-judge.yaml\n          runs_on: {kind: observations_ready}\n"
        ),
    }
    for name, body in files.items():
        (root / name).write_text(body, encoding="utf-8")
    return root / "service.yaml"


def rows_of(query: Bifrost, sql: str) -> list[dict]:
    """Run one Bifrost query and return its rows as dictionaries."""
    return query.sql(sql).to_arrow().to_pylist()


def assert_psi_bins(details: str, sample: int) -> None:
    """Assert the persisted PSI bin evidence of ``latency`` in a result's ``details``.

    The baseline's integers 0..99 fill ten equal-width bins with a tenth each;
    every target value lies above 99, so the whole sample lands in the last bin.
    """
    psi = json.loads(details)["features"]["latency"]["evidence"]["Psi"]
    assert psi["sample"] == sample, psi
    bins = psi["bins"]
    assert len(bins) == 10, bins
    assert all(abs(item["bin"]["proportion"] - 0.1) < 1e-9 for item in bins), bins
    assert [item["target_count"] for item in bins] == [0] * 9 + [sample], bins
    assert bins[-1]["target_proportion"] == 1.0, bins


def emit_invocation(state: WyrdState) -> tuple[Run, Run, Run]:
    """Emit one invocation that switches from the Model view to the Agent view.

    The Model view alternates typed and mapping Drift payloads far above the
    baseline, with a Custom ``score`` above its threshold; the Agent view emits
    one mapping and one typed Eval context that pass both Eval Verifiers.
    Returns the root run and its two views.
    """
    with state.run() as run:
        with run.for_card("model") as model:
            for row in range(DRIFT_ROWS):
                if row % 2 == 0:
                    model.observe.drift(Features(latency=150.0 + row, score=5.0))
                else:
                    model.observe.drift({"latency": 150.0 + row, "score": 5.0})
        with run.for_card("agent") as agent:
            agent.observe.eval({"answer": "yes"})
            agent.observe.eval(Exchange(answer="yes"))
    return run, model, agent


def emit_payload_forms(state: WyrdState) -> list[str]:
    """Emit one equal payload as a mapping, a dataclass, and a Pydantic model.

    Each form is its own root-Service run, so its rows are addressable by run
    and never enter the Model's Drift windows. Returns the three run ids.
    """
    forms: list[object] = [
        {"latency": 1.5, "tier": "gold", "count": 3, "cached": True},
        Shape(latency=1.5, tier="gold", count=3, cached=True),
        ShapeModel(latency=1.5, tier="gold", count=3, cached=True),
    ]
    run_ids = []
    for form in forms:
        run = state.run()
        run.observe.drift(form)
        run_ids.append(run.run_id)
    return run_ids


def assert_payload_forms_agree(query: Bifrost, run_ids: list[str]) -> None:
    """Assert all three payload forms persisted the same canonical tall rows."""
    persisted = [
        rows_of(
            query,
            "SELECT series, num_value, str_value FROM vala.drift.observations "
            f"WHERE run_id = '{run_id}' ORDER BY series",
        )
        for run_id in run_ids
    ]
    canonical = [
        {"series": "cached", "num_value": None, "str_value": "true"},
        {"series": "count", "num_value": 3.0, "str_value": "3"},
        {"series": "latency", "num_value": 1.5, "str_value": "1.5"},
        {"series": "tier", "num_value": None, "str_value": "gold"},
    ]
    assert persisted == [canonical] * 3, persisted


def assert_eval_runs(server: WyrdTestServer, query: Bifrost, agent_uid: str) -> set[str]:
    """Read the four observation-created Eval results and return their binding ids.

    Each acknowledged record activates both Eval bindings; every run passes
    its gate and persists one item row per task joined to its summary on
    ``result_id`` (Oracle scopes both sides to the caller's data tenant, which
    is not a queryable column). Each binding runs its own one task.
    """
    joined = await_rows(
        server,
        query,
        "SELECT r.verdict, r.binding_id, r.subject_card_uid, i.task_id "
        "FROM vala.verification.results r JOIN vala.eval.result_items i "
        "ON r.result_id = i.result_id "
        f"WHERE r.subject_card_uid = '{agent_uid}'",
        4,
    )
    assert {(row["verdict"], row["subject_card_uid"]) for row in joined} == {("passed", agent_uid)}
    tasks = {(row["binding_id"], row["task_id"]) for row in joined}
    assert sorted(task for _, task in tasks) == ["answer", "judge"], joined
    assert sorted(row["task_id"] for row in joined) == ["answer", "answer", "judge", "judge"], (
        "each record runs both Eval bindings"
    )
    return {binding for binding, _ in tasks}


def assert_retired_kinds_refused(cards: Cards, root: Path) -> None:
    """Registering a ``kind: Drift`` or ``kind: Eval`` Card is refused visibly."""
    for kind in ("Drift", "Eval"):
        retired = root / f"retired-{kind.lower()}.yaml"
        retired.write_text(
            f"apiVersion: wyrd/v1\nkind: {kind}\nmetadata:\n  name: py-retired-{kind.lower()}\n"
            "  version: 1.0.0\n  space: default\nspec: {}\n",
            encoding="utf-8",
        )
        with pytest.raises(WyrdError) as refused:
            cards.register_from_path(str(retired))
        details = refused.value.details or {}
        assert [d["code"] for d in details.get("diagnostics", [])] == [
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        ], refused.value
        assert details["diagnostics"][0]["path"] == str(retired), details


DRIFT_ROWS = ROWS + 20


@pytest.mark.integration
def test_service_bindings_verify_drift_and_eval_through_an_http_operator(tmp_path: Path) -> None:
    """One Service's five bindings verify typed and mapping observations end to end."""
    with (
        local_upstream() as (upstream, received),
        WyrdTestServer(verification_runtime=True, provider_base_url=upstream) as server,
    ):
        admin = server.bootstrap_service(["admin"], name="py-bound-admin")
        cards = Cards(server_url=server.base_url, credential=admin)
        baseline = cards.data.register(
            DataCard(
                PolarsInterface(data=pl.DataFrame({"latency": LATENCY})),
                space="default",
                name="py-bound-data",
                version="1.0.0",
            )
        ).root
        graph = write_bound_graph(tmp_path, baseline, f"{upstream}/hook")
        cards.register_from_path(str(tmp_path / "model.yaml"))
        receipt = cards.register_from_path(str(graph))
        service = receipt.root
        refs = {outcome.card_ref.name: outcome.card_ref for outcome in receipt.outcomes}
        for name in ("py-bound-psi", "py-bound-spc"):
            wait_ready(server, admin, refs[name], tmp_path / "status")
        credential = server.credential_registered_service(
            f"{service.space}/Service/{service.name}@{service.version}", ["admin"]
        )
        bundle = tmp_path / "bundle"
        binding_ids = download(server, admin, "Service", str(service.uid), bundle)["verification"][
            "binding_ids"
        ]
        assert len(binding_ids) == 5, binding_ids

        state = WyrdState.from_path(bundle, interfaces={"model": NoopModelInterface()})
        state.start_bifrost(server_url=server.base_url, credential=credential)
        model_uid, agent_uid = str(state.card_ref("model").uid), str(state.card_ref("agent").uid)
        run, model, agent = emit_invocation(state)
        assert model.run_id == agent.run_id == run.run_id
        assert (model.alias, agent.alias) == ("model", "agent")
        direct = model.observe.verify(
            "py-bound-psi", [{"latency": 150.0 + row} for row in range(DRIFT_ROWS)]
        )
        assert direct.verdict == "failed", direct.summary
        assert (str(direct.verifier.uid), str(direct.subject.uid)) == (
            str(refs["py-bound-psi"].uid),
            model_uid,
        )
        form_runs = emit_payload_forms(state)
        state.shutdown()
        server.flush_bifrost()

        query = Bifrost(server_url=server.base_url, credential=admin)
        assert_payload_forms_agree(query, form_runs)
        drift = rows_of(
            query,
            "SELECT COUNT(*) AS n, MIN(card_uid) AS low, MAX(card_uid) AS high "
            f"FROM vala.drift.observations WHERE run_id = '{run.run_id}'",
        )
        assert drift == [{"n": 2 * DRIFT_ROWS, "low": model_uid, "high": model_uid}], drift

        eval_bindings = assert_eval_runs(server, query, agent_uid)
        assert len(eval_bindings) == 2 and eval_bindings < set(binding_ids), eval_bindings
        judged = [path for path, _ in received if path == "/v1/chat/completions"]
        assert len(judged) == 2, "the judge binding called the local provider once per record"

        drift_bindings = sorted(set(binding_ids) - eval_bindings)
        for binding_id in drift_bindings:
            server.make_binding_due(binding_id)
        listed = ", ".join(f"'{binding_id}'" for binding_id in drift_bindings)
        results = await_rows(
            server,
            query,
            "SELECT r.verdict, r.owner_card_uid, r.binding_id, r.subject_card_uid, r.details, "
            "f.method FROM vala.verification.results r JOIN vala.drift.result_features f "
            f"ON r.result_id = f.result_id WHERE r.binding_id IN ({listed})",
            3,
        )
        by_method = {result["method"]: result for result in results}
        assert sorted(by_method) == ["Custom", "Psi", "Spc"], results
        assert sorted(result["binding_id"] for result in results) == drift_bindings
        for result in results:
            assert result["verdict"] == "failed", result
            assert result["owner_card_uid"] == str(service.uid), result
            assert result["subject_card_uid"] == model_uid, result
        assert_psi_bins(by_method["Psi"]["details"], DRIFT_ROWS)
        assert_spc_evidence(by_method["Spc"]["details"], DRIFT_ROWS // 5, DRIFT_ROWS // 5)
        await_hooks(received, 1)

        assert_retired_kinds_refused(cards, tmp_path)
