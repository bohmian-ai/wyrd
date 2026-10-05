"""Python Drift verification journey through the public ``wyrd`` package.

Registers Pandas, Polars, and Arrow baseline Data Cards, each saved as Parquet,
fits a PSI Verifier from each and an SPC Verifier from the Polars baseline, and
reads every baseline's Card status until it is ready. A Service bound to the
SPC Verifier with one Operator emits Drift observations through ``WyrdState``;
direct runs of every Verifier then score the window server-side and persist
their results, SPC X-bar/S evidence, and feature rows without an Operator
dispatch. A manual run of the Service's binding and its due scheduled
occurrence each fail on SPC signals and dispatch its Operator. Once the SPC
Verifier's stored fit is retired to a pre-revision format, a new run is refused
with ``baseline_legacy`` while its earlier result stays readable. Negative flows
cover an Arrow IPC baseline refused as non-Parquet, an SPC profile carrying the
retired ``weco_rule`` field, and a caller without ``evals:run``. A second journey proves each method's edge
semantics on isolated subjects: an empty tenant, a baseline-like window, SPC
subgroups until a partial one, a sparse window, unrelated and incomplete
records, per-row Custom averaging with window bounds, and a text-valued metric.

A third journey registers one Service whose Model carries PSI, SPC, and Custom
Drift bindings and whose Agent carries a deterministic and a local LLM-judge
Eval binding. As its own Card-bound principal the Service emits typed and
mapping Drift and Eval observations through one ``state.run()`` that switches
between the Model and Agent views; Scribe acknowledges them, each Eval
observation runs both Eval bindings, and manual binding runs score each Drift
method. The failed PSI binding run delivers its HTTP Operator to a local
receiver, read back as ``delivered`` through Run GET, and a direct PSI run
persists null owner and binding identity. Mapping, dataclass, and Pydantic
payloads persist identical tall Drift rows, and registering a retired
``kind: Drift`` or ``kind: Eval`` Card is refused.
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
from datetime import UTC, datetime, timedelta
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
from wyrd.observe import Run
from wyrd.prompt import Prompt
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer
from wyrd.verification import Verification

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


SERVICE = """apiVersion: wyrd/v1
kind: Service
metadata:
  name: py-drift-service
  version: 1.0.0
  space: default
spec:
  verified_by:
    - verifier:
        kind: Verifier
        name: py-drift-spc
        version: 1.0.0
        space: default
      runs_on:
        kind: schedule
        cron: "0 0 * * *"
      on_failure:
        - kind: http
          method: post
          url: https://hooks.example.test/py-drift
"""


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


def settle(verification: Verification, run_id: str) -> dict:
    """Poll run ``run_id`` until it leaves the queue and return its status."""
    deadline = time.monotonic() + WAIT_SECONDS
    while (run := verification.get_run(run_id))["status"] in {"pending", "running", "retrying"}:
        assert time.monotonic() < deadline, f"run never settled: {run}"
        time.sleep(0.1)
    return run


def scheduled_run(server: WyrdTestServer, binding_id: str) -> str:
    """Make ``binding_id`` due and return the one run its occurrence schedules.

    Dueness is PostgreSQL's decision, so the test harness places the schedule
    cursor at statement time and the verification runtime schedules the
    occurrence; the daily window ``[midnight UTC, now)`` holds this journey's
    observations.
    """
    earlier = set(server.verification_runs())
    server.make_binding_due(binding_id)
    deadline = time.monotonic() + WAIT_SECONDS
    while not (runs := [run for run in server.verification_runs() if run not in earlier]):
        assert time.monotonic() < deadline, "the due binding never scheduled a run"
        time.sleep(0.1)
    (run,) = runs
    return run


def complete(
    verification: Verification,
    verifier: CardRef,
    subject: CardRef,
    window: tuple[datetime, datetime] | None = None,
) -> str:
    """Run ``verifier`` directly over ``subject`` and return its result.

    ``window`` is the ``[start, end)`` range; omitted, it spans an hour either side of now.
    """
    now = datetime.now(UTC)
    start, end = window or (now - timedelta(hours=1), now + timedelta(hours=1))
    run_id = verification.start_run(
        {
            "target": {
                "kind": "verifier",
                "verifier_uid": str(verifier.uid),
                "subject_card_uid": str(subject.uid),
            },
            "input": {
                "kind": "drift_window",
                "start": start.isoformat(),
                "end": end.isoformat(),
            },
        }
    )
    run = settle(verification, run_id)
    assert run["status"] == "completed", run
    assert run["dispatches"] == [], "a direct run never dispatches"
    return run["result_id"]


def assert_spc_evidence(features: str, subgroups: int, x_bar_signals: int) -> None:
    """Assert the persisted SPC evidence of ``latency`` in a result's
    ``drift_report['features']``, read back as JSON text.

    The baseline's twenty subgroups of five consecutive integers fix the X-bar
    center at 49.5 and the S center at ``sqrt(2.5)``; the limits must be the
    NIST X-bar/S limits around them, and SPC compares signals with zero.
    """
    feature = json.loads(features)["latency"]
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
def test_parquet_baselines_fit_and_score_drift_server_side(tmp_path: Path) -> None:
    """Pandas, Polars, and Arrow baselines fit, and each Verifier scores server-side."""
    with WyrdTestServer(verification_runtime=True) as server:
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

        (tmp_path / "service.yaml").write_text(SERVICE, encoding="utf-8")
        service = cards.register_from_path(str(tmp_path / "service.yaml")).root
        credential = server.credential_registered_service(
            f"{service.space}/Service/{service.name}@{service.version}", ["admin"]
        )
        bundle = tmp_path / "service"
        (binding_id,) = download(server, admin, "Service", str(service.uid), bundle)[
            "verification"
        ]["binding_ids"]
        state = WyrdState.from_path(bundle)
        state.start_bifrost(server_url=server.base_url, credential=credential)
        run = state.run()
        for row in range(120):
            run.observe.drift({"latency": 150.0 + row})
        state.shutdown()
        server.flush_bifrost()

        verification = Verification(server_url=server.base_url, credential=credential)
        query = Bifrost(server_url=server.base_url, credential=admin)
        results = {}
        for name, verifier in verifiers.items():
            result_id = results[name] = complete(verification, verifier, service)
            server.flush_bifrost()
            (result,) = (
                query.sql(
                    "SELECT execution_status, verdict, subject_card_uid, binding_id, "
                    "to_json(drift_report['features']) AS features "
                    f"FROM vala.verification.results WHERE result_id = '{result_id}'"
                )
                .to_arrow()
                .to_pylist()
            )
            assert (result["execution_status"], result["verdict"]) == ("completed", "failed"), name
            assert result["subject_card_uid"] == str(service.uid)
            assert result["binding_id"] is None
            features = (
                query.sql(
                    "SELECT f.feature, f.verdict FROM vala.drift.result_features f "
                    "JOIN vala.verification.results r ON f.result_id = r.result_id "
                    f"WHERE r.result_id = '{result_id}'"
                )
                .to_arrow()
                .to_pylist()
            )
            assert features == [{"feature": "latency", "verdict": "drift"}], name
            if name == "py-drift-spc":
                assert_spc_evidence(result["features"], 24, 24)

        now = datetime.now(UTC)
        binding_run = verification.start_run(
            {
                "target": {"kind": "binding", "binding_id": binding_id},
                "input": {
                    "kind": "drift_window",
                    "start": (now - timedelta(hours=1)).isoformat(),
                    "end": (now + timedelta(hours=1)).isoformat(),
                },
            }
        )
        for run_id in (binding_run, scheduled_run(server, binding_id)):
            run = settle(verification, run_id)
            assert run["status"] == "completed", run
            assert len(run["dispatches"]) == 1, "a failed binding result dispatches its Operator"
            server.flush_bifrost()
            (bound,) = (
                query.sql(
                    "SELECT verdict, binding_id, to_json(drift_report['features']) AS features "
                    "FROM vala.verification.results "
                    f"WHERE result_id = '{run['result_id']}'"
                )
                .to_arrow()
                .to_pylist()
            )
            assert (bound["verdict"], bound["binding_id"]) == ("failed", binding_id), bound
            assert_spc_evidence(bound["features"], 24, 24)

        spc = verifiers["py-drift-spc"]
        server.retire_fitted_format(str(spc.uid))
        now = datetime.now(UTC)
        refused = settle(
            verification,
            verification.start_run(
                {
                    "target": {
                        "kind": "verifier",
                        "verifier_uid": str(spc.uid),
                        "subject_card_uid": str(service.uid),
                    },
                    "input": {
                        "kind": "drift_window",
                        "start": (now - timedelta(hours=1)).isoformat(),
                        "end": (now + timedelta(hours=1)).isoformat(),
                    },
                }
            ),
        )
        assert refused["status"] == "errored", refused
        assert refused["error"]["code"] == "baseline_legacy", refused
        assert refused["result_id"] is None, "a refused legacy run is never scored"
        (historical,) = (
            query.sql(
                "SELECT verdict, to_json(drift_report['features']) AS features "
                "FROM vala.verification.results "
                f"WHERE result_id = '{results['py-drift-spc']}'"
            )
            .to_arrow()
            .to_pylist()
        )
        assert historical["verdict"] == "failed", historical
        assert_spc_evidence(historical["features"], 24, 24)

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

        reader = Verification(
            server_url=server.base_url,
            credential=server.bootstrap_service(["reader"], name="py-drift-reader"),
        )
        with pytest.raises(WyrdError) as denied:
            reader.start_run(
                {
                    "target": {
                        "kind": "verifier",
                        "verifier_uid": str(verifiers["py-drift-pandas"].uid),
                        "subject_card_uid": str(service.uid),
                    },
                    "input": {
                        "kind": "drift_window",
                        "start": "2026-09-17T00:00:00Z",
                        "end": "2026-09-17T01:00:00Z",
                    },
                }
            )
        assert denied.value.status == 403


EDGE_VERIFIERS = {
    "py-edge-psi": (
        "      method: Psi\n      signal:\n        kind: Distribution\n"
        "        baseline_ref:\n          kind: Data\n          name: py-edge-data\n"
        "          version: 1.0.0\n          space: default\n"
        "        features: [latency, tier]\n      condition:\n        kind: Statistical\n"
        "      profile:\n        kind: Psi\n"
        "        binning_strategy:\n          kind: EqualWidth\n          n_bins: 10\n"
        "        categorical_features: [tier]\n"
        "        threshold:\n          kind: Fixed\n          value: 0.25\n"
    ),
    "py-edge-spc": (
        "      method: Spc\n      signal:\n        kind: Distribution\n"
        "        baseline_ref:\n          kind: Data\n          name: py-edge-data\n"
        "          version: 1.0.0\n          space: default\n"
        "        features: [latency]\n      condition:\n        kind: Statistical\n"
        "      profile:\n        kind: Spc\n        sample_size: 5\n"
    ),
    "py-edge-custom": (
        "      method: Custom\n      signal:\n        kind: Metric\n        name: score\n"
        "      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n"
        "        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n"
    ),
}


def register_edge_verifier(cards: Cards, root: Path, name: str) -> CardRef:
    """Register one method-edge Drift Verifier from ``EDGE_VERIFIERS``."""
    path = root / f"{name}.yaml"
    path.write_text(
        f"apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n"
        "  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n"
        f"    spec:\n{EDGE_VERIFIERS[name]}",
        encoding="utf-8",
    )
    return cards.register_from_path(str(path)).root


def subject(cards: Cards, root: Path, name: str) -> CardRef:
    """Register an unbound Service named ``name`` as a Drift subject."""
    path = root / f"{name}.yaml"
    path.write_text(
        f"apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: {name}\n"
        "  version: 1.0.0\n  space: default\nspec: {}\n",
        encoding="utf-8",
    )
    return cards.register_from_path(str(path)).root


def subject_credential(server: WyrdTestServer, service: CardRef) -> str:
    """Issue the subject Service's own key, whose Card scope covers its manual runs."""
    return server.credential_registered_service(
        f"{service.space}/Service/{service.name}@{service.version}", ["admin"]
    )


def emit_rows(
    server: WyrdTestServer, admin: str, service: CardRef, bundle: Path, rows: list[dict]
) -> None:
    """Emit ``rows`` as Drift observations of ``service`` through one ``WyrdState`` lifetime.

    Each lifetime is one client batch; the batch is drained and flushed before returning.
    """
    download(server, admin, "Service", str(service.uid), bundle)
    state = WyrdState.from_path(bundle)
    state.start_bifrost(server_url=server.base_url, credential=subject_credential(server, service))
    run = state.run()
    for row in rows:
        run.observe.drift(row)
    state.shutdown()
    server.flush_bifrost()


def read_result(server: WyrdTestServer, query: Bifrost, result_id: str) -> tuple[dict, list]:
    """Flush Scribe, then read one result and its ``(feature, method, verdict)`` rows.

    A tenant that has never scored a report has no feature table yet, which reads
    as no feature rows.
    """
    server.flush_bifrost()
    (result,) = (
        query.sql(
            "SELECT execution_status, verdict, to_json(drift_report['features']) AS features "
            f"FROM vala.verification.results WHERE result_id = '{result_id}'"
        )
        .to_arrow()
        .to_pylist()
    )
    try:
        rows = (
            query.sql(
                "SELECT f.feature, f.method, f.verdict FROM vala.drift.result_features f "
                "JOIN vala.verification.results r ON f.result_id = r.result_id "
                f"WHERE r.result_id = '{result_id}' ORDER BY f.feature"
            )
            .to_arrow()
            .to_pylist()
        )
    except WyrdError as error:
        assert error.code == "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND", error
        rows = []
    return result, [(row["feature"], row["method"], row["verdict"]) for row in rows]


def assert_unscored(outcome: tuple[dict, list]) -> None:
    """Assert a result completed inconclusive before scoring: no report, no features."""
    result, features = outcome
    assert (result["execution_status"], result["verdict"]) == ("completed", "inconclusive"), result
    assert result["features"] is None, result
    assert features == [], features


@pytest.mark.integration
def test_drift_method_edges_score_through_oracle(tmp_path: Path) -> None:
    """Each Drift method's edge semantics hold through the production runtime.

    Before the tenant's first Drift write a Custom run completes inconclusive with
    no report, features, or dispatch. Separate subjects then isolate each case: a
    baseline-like window passes PSI and Custom (a mean at the threshold is no
    drift); two in-control subgroups pass SPC with zero-signal evidence until a
    trailing partial subgroup leaves it unscored; three rows are too few for
    PSI and leave SPC a partial, unscored subgroup; records without a PSI feature are ignored while
    one omitting a feature leaves PSI unscored; Custom averages per row, not per
    batch, and the window bounds exclude a batch; a text-valued metric is
    inconclusive.
    """
    with WyrdTestServer(verification_runtime=True) as server:
        admin = server.bootstrap_service(["admin"], name="py-drift-edges-admin")
        cards = Cards(server_url=server.base_url, credential=admin)
        query = Bifrost(server_url=server.base_url, credential=admin)
        now = datetime.now(UTC)
        start, end = now - timedelta(hours=1), now + timedelta(hours=1)

        def run(
            verifier: CardRef, service: CardRef, window: tuple[datetime, datetime] = (start, end)
        ) -> tuple[dict, list]:
            verification = Verification(
                server_url=server.base_url, credential=subject_credential(server, service)
            )
            return read_result(server, query, complete(verification, verifier, service, window))

        custom = register_edge_verifier(cards, tmp_path, "py-edge-custom")
        steady = subject(cards, tmp_path, "py-edge-steady")
        assert_unscored(run(custom, steady))

        tier = ["gold" if row % 2 == 0 else "silver" for row in range(ROWS)]
        baseline = DataCard(
            PolarsInterface(data=pl.DataFrame({"latency": LATENCY, "tier": tier})),
            space="default",
            name="py-edge-data",
            version="1.0.0",
        )
        cards.data.register(baseline)
        psi = register_edge_verifier(cards, tmp_path, "py-edge-psi")
        spc = register_edge_verifier(cards, tmp_path, "py-edge-spc")
        for verifier in (psi, spc):
            wait_ready(server, admin, verifier, tmp_path / "status")

        bundles = tmp_path / "bundles"
        baseline_like = [
            {
                "latency": float((row * 37) % ROWS),
                "tier": tier[row],
                "score": 1.0 if row % 2 == 0 else 2.0,
            }
            for row in range(ROWS)
        ]
        emit_rows(server, admin, steady, bundles / "steady", baseline_like)
        result, features = run(psi, steady)
        assert result["verdict"] == "passed", result
        assert features == [("latency", "Psi", "no_drift"), ("tier", "Psi", "no_drift")]
        result, features = run(custom, steady)
        assert result["verdict"] == "passed", "a mean at the threshold is no drift"
        assert features == [("score", "Custom", "no_drift")]

        def latencies(values: list[float]) -> list[dict]:
            return [{"latency": value, "tier": "gold", "score": 1.0} for value in values]

        calm = subject(cards, tmp_path, "py-edge-calm")
        emit_rows(
            server, admin, calm, bundles / "calm", latencies([48.0, 49.0, 50.0, 51.0, 52.0] * 2)
        )
        result, features = run(spc, calm)
        assert result["verdict"] == "passed", result
        assert features == [("latency", "Spc", "no_drift")]
        assert_spc_evidence(result["features"], 2, 0)
        emit_rows(server, admin, calm, bundles / "calm-partial", latencies([50.0, 50.0]))
        assert_unscored(run(spc, calm))

        sparse = subject(cards, tmp_path, "py-edge-sparse")
        emit_rows(server, admin, sparse, bundles / "sparse", baseline_like[:3])
        assert_unscored(run(psi, sparse))
        assert_unscored(run(spc, sparse))

        gappy = subject(cards, tmp_path, "py-edge-gappy")
        emit_rows(server, admin, gappy, bundles / "gappy", baseline_like)
        emit_rows(server, admin, gappy, bundles / "gappy-unrelated", [{"score": 9.0}] * 5)
        result, _ = run(psi, gappy)
        assert result["verdict"] == "passed", "records without a PSI feature do not enter PSI"
        emit_rows(server, admin, gappy, bundles / "gappy-omitted", [{"latency": 50.0}])
        assert_unscored(run(psi, gappy))

        weighted = subject(cards, tmp_path, "py-edge-weighted")

        def scores(values: list[float]) -> list[dict]:
            return [{"latency": 50.0, "tier": "gold", "score": value} for value in values]

        emit_rows(server, admin, weighted, bundles / "weighted-a", scores([1.0]))
        split = datetime.now(UTC)
        emit_rows(server, admin, weighted, bundles / "weighted-b", scores([2.0, 2.0, 2.0]))
        result, _ = run(custom, weighted)
        assert result["verdict"] == "failed", "rows average 1.75; batches would average 1.5"
        result, _ = run(custom, weighted, (start, split))
        assert result["verdict"] == "passed", "the window end excludes the second batch"
        result, _ = run(custom, weighted, (split, end))
        assert result["verdict"] == "failed", "the window start excludes the first batch"

        text = subject(cards, tmp_path, "py-edge-text")
        emit_rows(server, admin, text, bundles / "text", [{"score": "high"}] * 3)
        assert_unscored(run(custom, text))


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
            f"    spec:\n{EDGE_VERIFIERS['py-edge-custom']}"
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


def await_binding_ready(verification: Verification, binding_id: str) -> dict:
    """Poll one binding until its Verifier is ready to run, and return it."""
    deadline = time.monotonic() + WAIT_SECONDS
    while (binding := verification.get_binding(binding_id))["readiness"] != "ready":
        assert time.monotonic() < deadline, f"binding never became ready: {binding}"
        time.sleep(0.2)
    return binding


def await_new_runs(server: WyrdTestServer, earlier: set[str], count: int) -> list[str]:
    """Poll until ``count`` verification runs exist beyond ``earlier``, and return them."""
    deadline = time.monotonic() + WAIT_SECONDS
    while len(runs := [run for run in server.verification_runs() if run not in earlier]) < count:
        assert time.monotonic() < deadline, f"only {len(runs)} of {count} runs were created"
        time.sleep(0.1)
    assert len(runs) == count, runs
    return runs


def await_dispatches(verification: Verification, run_id: str) -> dict:
    """Poll a settled run until every Operator dispatch leaves the queue."""
    deadline = time.monotonic() + WAIT_SECONDS
    while True:
        run = verification.get_run(run_id)
        statuses = {dispatch["status"] for dispatch in run["dispatches"]}
        if not statuses & {"pending", "running", "retrying"}:
            return run
        assert time.monotonic() < deadline, f"a dispatch never settled: {run}"
        time.sleep(0.1)


def manual_window() -> dict[str, str]:
    """A Drift window spanning an hour either side of now."""
    now = datetime.now(UTC)
    return {
        "kind": "drift_window",
        "start": (now - timedelta(hours=1)).isoformat(),
        "end": (now + timedelta(hours=1)).isoformat(),
    }


def rows_of(query: Bifrost, sql: str) -> list[dict]:
    """Run one Bifrost query and return its rows as dictionaries."""
    return query.sql(sql).to_arrow().to_pylist()


def assert_psi_bins(features: str, sample: int) -> None:
    """Assert the persisted PSI bin evidence of ``latency`` in a result's
    ``drift_report['features']``, read back as JSON text.

    The baseline's integers 0..99 fill ten equal-width bins with a tenth each;
    every target value lies above 99, so the whole sample lands in the last bin.
    """
    psi = json.loads(features)["latency"]["evidence"]["Psi"]
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


def assert_eval_runs(
    server: WyrdTestServer,
    verification: Verification,
    query: Bifrost,
    earlier: set[str],
    tasks: dict[str, str],
    agent_uid: str,
) -> None:
    """Settle the four observation-created Eval runs and read their joined results.

    Each acknowledged record activates both Eval bindings; every run passes
    its gate, dispatches nothing, and persists one item row per task joined to
    its summary on ``result_id`` (Oracle scopes both sides to the caller's
    data tenant, which is not a queryable column). ``tasks`` maps each Eval
    binding id to its one task id.
    """
    runs = await_new_runs(server, earlier, 4)
    observed = []
    for run_id in runs:
        settled = settle(verification, run_id)
        assert settled["status"] == "completed", settled
        assert settled["dispatches"] == [], "a passing gate dispatches nothing"
        assert settled["requested_by_principal_id"] is None, settled
        server.flush_bifrost()
        (joined,) = rows_of(
            query,
            "SELECT r.verdict, r.binding_id, r.subject_card_uid, i.task_id, i.outcome_kind "
            "FROM vala.verification.results r JOIN vala.eval.result_items i "
            "ON r.result_id = i.result_id "
            f"WHERE r.result_id = '{settled['result_id']}'",
        )
        assert (joined["verdict"], joined["subject_card_uid"]) == ("passed", agent_uid), joined
        assert joined["task_id"] == tasks[joined["binding_id"]], joined
        observed.append(joined["binding_id"])
    assert sorted(observed) == sorted(list(tasks) * 2), "each record runs both Eval bindings"


def assert_direct_run_unbound(
    verification: Verification, query: Bifrost, server: WyrdTestServer, psi: str, model: str
) -> None:
    """A direct PSI run persists null owner and binding on its summary and features."""
    result_id = complete(
        verification,
        CardRef(kind="Verifier", name="py-bound-psi", version="1.0.0", space="default", uid=psi),
        CardRef(kind="Model", name="py-bound-model", version="1.0.0", space="default", uid=model),
    )
    server.flush_bifrost()
    (unbound,) = rows_of(
        query,
        "SELECT r.owner_card_uid, r.binding_id, f.owner_card_uid AS feature_owner, "
        "f.binding_id AS feature_binding, f.verdict FROM vala.verification.results r "
        "JOIN vala.drift.result_features f "
        "ON r.result_id = f.result_id "
        f"WHERE r.result_id = '{result_id}'",
    )
    assert unbound == {
        "owner_card_uid": None,
        "binding_id": None,
        "feature_owner": None,
        "feature_binding": None,
        "verdict": "drift",
    }, unbound


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
        model_ref = cards.register_from_path(str(tmp_path / "model.yaml")).root
        receipt = cards.register_from_path(str(graph))
        service = receipt.root
        uids = {outcome.card_ref.name: str(outcome.card_ref.uid) for outcome in receipt.outcomes}
        uids["py-bound-model"] = str(model_ref.uid)
        credential = server.credential_registered_service(
            f"{service.space}/Service/{service.name}@{service.version}", ["admin"]
        )
        verification = Verification(server_url=server.base_url, credential=credential)
        bundle = tmp_path / "bundle"
        binding_ids = download(server, admin, "Service", str(service.uid), bundle)["verification"][
            "binding_ids"
        ]
        by_verifier = {uid: name for name, uid in uids.items()}
        bindings = {
            by_verifier[binding["verifier_uid"]]: binding
            for binding in (await_binding_ready(verification, b) for b in binding_ids)
        }
        assert sorted(bindings) == [
            "py-bound-assert",
            "py-bound-custom",
            "py-bound-judge-eval",
            "py-bound-psi",
            "py-bound-spc",
        ], bindings
        model_uid, agent_uid = uids["py-bound-model"], uids["py-bound-agent"]
        assert {
            binding["subject_card_uid"]
            for name, binding in bindings.items()
            if name in {"py-bound-psi", "py-bound-spc", "py-bound-custom"}
        } == {model_uid}

        state = WyrdState.from_path(bundle, interfaces={"model": NoopModelInterface()})
        state.start_bifrost(server_url=server.base_url, credential=credential)
        run, model, agent = emit_invocation(state)
        assert model.run_id == agent.run_id == run.run_id
        assert (model.card_ref.split("#", 1)[1], agent.card_ref.split("#", 1)[1]) == (
            model_uid,
            agent_uid,
        )
        form_runs = emit_payload_forms(state)
        earlier = set(server.verification_runs())
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

        tasks = {
            bindings["py-bound-assert"]["binding_id"]: "answer",
            bindings["py-bound-judge-eval"]["binding_id"]: "judge",
        }
        assert_eval_runs(server, verification, query, earlier, tasks, agent_uid)
        judged = [path for path, _ in received if path == "/v1/chat/completions"]
        assert len(judged) == 2, "the judge binding called the local provider once per record"

        for name in ("py-bound-psi", "py-bound-spc", "py-bound-custom"):
            binding_id = bindings[name]["binding_id"]
            run_id = verification.start_run(
                {"target": {"kind": "binding", "binding_id": binding_id}, "input": manual_window()}
            )
            settled = settle(verification, run_id)
            assert settled["status"] == "completed", settled
            assert settled["requested_by_principal_id"] is not None, settled
            server.flush_bifrost()
            (result,) = rows_of(
                query,
                "SELECT verdict, owner_card_uid, binding_id, subject_card_uid, "
                "to_json(drift_report['features']) AS features "
                f"FROM vala.verification.results WHERE result_id = '{settled['result_id']}'",
            )
            assert result["verdict"] == "failed", (name, result)
            assert (result["owner_card_uid"], result["binding_id"]) == (
                str(service.uid),
                binding_id,
            ), result
            assert result["subject_card_uid"] == model_uid, result
            if name == "py-bound-psi":
                delivered = await_dispatches(verification, run_id)
                assert [d["status"] for d in delivered["dispatches"]] == ["delivered"], delivered
                assert [path for path, _ in received].count("/hook") == 1, received
                assert_psi_bins(result["features"], DRIFT_ROWS)
            else:
                assert settled["dispatches"] == [], "a binding without an Operator dispatches none"
            if name == "py-bound-spc":
                assert_spc_evidence(result["features"], DRIFT_ROWS // 5, DRIFT_ROWS // 5)

        assert_direct_run_unbound(verification, query, server, uids["py-bound-psi"], model_uid)
        assert_retired_kinds_refused(cards, tmp_path)
