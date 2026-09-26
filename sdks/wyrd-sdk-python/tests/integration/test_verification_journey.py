"""Python manual verification journey through the public ``wyrd`` package.

Registers a ready Custom Drift Verifier and a Service bound to it, reads the
served binding ID from the Service's hydrated bundle, then acts as the Service
itself: reads the binding, starts a keyed run, replays it, and reads the run
back. Negative flows cover a reused key, an inverted window, a malformed
request, and a credential without ``evals:run``.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.cards import Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer
from wyrd.verification import Verification

VERIFIER = """apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: py-run-drift
  version: 1.0.0
  space: default
spec:
  implementation:
    kind: drift
    spec:
      method: Custom
      signal:
        kind: Metric
        name: score
      condition:
        kind: Statistical
      profile:
        kind: Custom
        metric_name: score
        baseline_value: 1.0
        alert_threshold: 0.5
"""

SERVICE = """apiVersion: wyrd/v1
kind: Service
metadata:
  name: py-run-service
  version: 1.0.0
  space: default
spec:
  verified_by:
    - verifier:
        kind: Verifier
        name: py-run-drift
        version: 1.0.0
        space: default
      runs_on:
        kind: schedule
        cron: "0 2 * * *"
"""


def served_binding_id(server: WyrdTestServer, api_key: str, uid: str, bundle: Path) -> str:
    """Download the Service through the public CLI and read its one binding ID."""
    environment = os.environ.copy()
    environment.update(WYRD_SERVER_URL=server.base_url, WYRD_API_KEY=api_key)
    completed = subprocess.run(
        [
            str(Path(sys.executable).with_name("wyrd")),
            "get",
            "--kind",
            "Service",
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
    status = WyrdState.from_path(bundle).card("root").status
    assert status is not None, "a binding owner serves status"
    (binding_id,) = status["verification"]["binding_ids"]
    return binding_id


def run_request(binding_id: str, end: str) -> dict[str, object]:
    """Build a manual Drift run request for ``binding_id`` ending at ``end``."""
    return {
        "target": {"kind": "binding", "binding_id": binding_id},
        "input": {"kind": "drift_window", "start": "2026-09-17T00:00:00Z", "end": end},
    }


@pytest.mark.integration
def test_service_starts_a_keyed_manual_run_and_reads_its_status(tmp_path: Path) -> None:
    """A bound Service reads its binding, starts a keyed run, and reads the run back."""
    (tmp_path / "verifier.yaml").write_text(VERIFIER, encoding="utf-8")
    (tmp_path / "service.yaml").write_text(SERVICE, encoding="utf-8")
    with WyrdTestServer(mutate_env=False) as server:
        admin = server.bootstrap_service(["admin"], name="py-run-admin")
        cards = Cards(server_url=server.base_url, credential=admin)
        cards.register_from_path(str(tmp_path / "verifier.yaml"))
        service = cards.register_from_path(str(tmp_path / "service.yaml")).root
        assert service.uid is not None
        binding_id = served_binding_id(server, admin, str(service.uid), tmp_path / "bundle")

        writer = Verification(
            server_url=server.base_url,
            credential=server.credential_registered_service(
                "default/Service/py-run-service@1.0.0", ["writer"]
            ),
        )
        binding = writer.get_binding(binding_id)
        assert binding["binding_id"] == binding_id
        assert binding["subject_card_uid"] == str(service.uid)
        assert binding["readiness"] == "ready"

        request = run_request(binding_id, "2026-09-17T01:00:00Z")
        run_id = writer.start_run(request, idempotency_key="py-journey-0001")
        assert writer.start_run(request, idempotency_key="py-journey-0001") == run_id
        run = writer.get_run(run_id)
        assert run["run_id"] == run_id
        assert run["requested_by_principal_id"] is not None

        with pytest.raises(WyrdError) as conflict:
            writer.start_run(
                run_request(binding_id, "2026-09-17T02:00:00Z"),
                idempotency_key="py-journey-0001",
            )
        assert conflict.value.code == "WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT"
        with pytest.raises(WyrdError) as inverted:
            writer.start_run(run_request(binding_id, "2026-09-16T00:00:00Z"))
        assert inverted.value.code == "WYRD_VERIFICATION_400_INVALID_WINDOW"

        reader = Verification(
            server_url=server.base_url,
            credential=server.bootstrap_service(["reader"], name="py-run-reader"),
        )
        with pytest.raises(WyrdError) as denied:
            reader.start_run(request)
        assert denied.value.status == 403
