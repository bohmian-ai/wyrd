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
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from datetime import UTC, datetime, timedelta
from pathlib import Path

import pandas as pd
import polars as pl
import pyarrow as pa
import pytest
import yaml
from wyrd import WyrdError
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.data import ArrowInterface, DataCard, PandasInterface, PolarsInterface
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
                    "SELECT execution_status, verdict, subject_card_uid, binding_id, details "
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
                assert_spc_evidence(result["details"], 24, 24)

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
                    "SELECT verdict, binding_id, details FROM vala.verification.results "
                    f"WHERE result_id = '{run['result_id']}'"
                )
                .to_arrow()
                .to_pylist()
            )
            assert (bound["verdict"], bound["binding_id"]) == ("failed", binding_id), bound
            assert_spc_evidence(bound["details"], 24, 24)

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
                "SELECT verdict, details FROM vala.verification.results "
                f"WHERE result_id = '{results['py-drift-spc']}'"
            )
            .to_arrow()
            .to_pylist()
        )
        assert historical["verdict"] == "failed", historical
        assert_spc_evidence(historical["details"], 24, 24)

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
            "SELECT execution_status, verdict, details "
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
    """Assert a result completed inconclusive before scoring: null details, no features."""
    result, features = outcome
    assert (result["execution_status"], result["verdict"]) == ("completed", "inconclusive"), result
    assert result["details"] is None, result
    assert features == [], features


@pytest.mark.integration
def test_drift_method_edges_score_through_oracle(tmp_path: Path) -> None:
    """Each Drift method's edge semantics hold through the production runtime.

    Before the tenant's first Drift write a Custom run completes inconclusive with
    no details, features, or dispatch. Separate subjects then isolate each case: a
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
        assert_spc_evidence(result["details"], 2, 0)
        emit_rows(server, admin, calm, bundles / "calm-partial", latencies([50.0, 50.0]))
        assert_unscored(run(spc, calm))

        sparse = subject(cards, tmp_path, "py-edge-sparse")
        emit_rows(server, admin, sparse, bundles / "sparse", baseline_like[:3])
        result, features = run(psi, sparse)
        assert result["verdict"] == "inconclusive", result
        assert features == [("latency", "Psi", "inconclusive"), ("tier", "Psi", "inconclusive")]
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
