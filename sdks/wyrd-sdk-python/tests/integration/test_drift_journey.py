"""Python Drift verification journey through the public ``wyrd`` package.

Registers Pandas, Polars, and Arrow baseline Data Cards, each saved as Parquet,
fits a PSI Verifier from each and an SPC Verifier from the Polars baseline, and
reads every baseline's Card status until it is ready. A Service emits Drift
observations through ``WyrdState``; direct runs of every Verifier then score
the window server-side and persist their results and feature rows without an
Operator dispatch. Negative flows cover an Arrow IPC baseline refused as
non-Parquet and a caller without ``evals:run``.
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
        "Spc": (
            "        kind: Spc\n        sample_size: 5\n"
            '        weco_rule:\n          rule_string: "8 16 4 8 2 4 1 1"\n'
            "        alert_threshold: Zone1\n"
        ),
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
        name: py-drift-pandas
        version: 1.0.0
        space: default
      runs_on:
        kind: schedule
        cron: "0 0 * * *"
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


def complete(verification: Verification, verifier: CardRef, subject: CardRef) -> str:
    """Run ``verifier`` directly over ``subject`` for the last hour and return its result."""
    now = datetime.now(UTC)
    run_id = verification.start_run(
        {
            "target": {
                "kind": "verifier",
                "verifier_uid": str(verifier.uid),
                "subject_card_uid": str(subject.uid),
            },
            "input": {
                "kind": "drift_window",
                "start": (now - timedelta(hours=1)).isoformat(),
                "end": (now + timedelta(hours=1)).isoformat(),
            },
        }
    )
    deadline = time.monotonic() + WAIT_SECONDS
    while (run := verification.get_run(run_id))["status"] in {"pending", "running", "retrying"}:
        assert time.monotonic() < deadline, f"run never settled: {run}"
        time.sleep(0.1)
    assert run["status"] == "completed", run
    assert run["dispatches"] == [], "a direct run never dispatches"
    return run["result_id"]


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
        download(server, admin, "Service", str(service.uid), bundle)
        state = WyrdState.from_path(bundle)
        state.start_bifrost(server_url=server.base_url, credential=credential)
        run = state.run()
        for row in range(120):
            run.observe.drift({"latency": 150.0 + row})
        state.shutdown()
        server.flush_bifrost()

        verification = Verification(server_url=server.base_url, credential=credential)
        query = Bifrost(server_url=server.base_url, credential=admin)
        for name, verifier in verifiers.items():
            result_id = complete(verification, verifier, service)
            server.flush_bifrost()
            (result,) = (
                query.sql(
                    "SELECT execution_status, verdict, subject_card_uid, binding_id "
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
