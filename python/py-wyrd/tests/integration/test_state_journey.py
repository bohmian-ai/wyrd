"""Real server-to-offline Python WyrdState journey."""

from __future__ import annotations

import base64
import hashlib
import os
import subprocess
import sys
from pathlib import Path

import pytest
import yaml
from wyrd.cards import Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from tests.unit.state.support import TinyModelInterface


def _write_source_tree(root: Path) -> Path:
    """Write authored Model and Service cards consumed by the real workflow."""
    payload = b"server-produced-model-artifact"
    digest = base64.b64encode(hashlib.sha256(payload).digest()).decode("ascii")
    model = {
        "apiVersion": "wyrd/v1",
        "kind": "Model",
        "metadata": {"name": "journey-model", "version": "1.0.0", "space": "python-e2e"},
        "spec": {
            "interface": {
                "kind": "Custom",
                "meta": {"loader_module": "journey", "loader_class": "TinyModel"},
            },
            "task_type": "Other",
            "signature": {"inputs": [], "outputs": []},
        },
        "artifacts": [
            {
                "relative_path": "model.bin",
                "sha256": digest,
                "size_bytes": len(payload),
                "content_type": "application/octet-stream",
            }
        ],
    }
    service = {
        "apiVersion": "wyrd/v1",
        "kind": "Service",
        "metadata": {"name": "python-state-journey", "version": "1.0.0", "space": "python-e2e"},
        "spec": {
            "service_type": "agent",
            "components": [
                {
                    "alias": "model_primary",
                    "ref": {
                        "kind": "Model",
                        "name": "journey-model",
                        "version": "1.0.0",
                        "space": "python-e2e",
                    },
                },
                {
                    "alias": "model_shadow",
                    "ref": {
                        "kind": "Model",
                        "name": "journey-model",
                        "version": "1.0.0",
                        "space": "python-e2e",
                    },
                },
            ],
        },
    }
    source = root / "source"
    source.mkdir()
    (source / "model.yaml").write_text(yaml.safe_dump(model, sort_keys=False), encoding="utf-8")
    (source / "service.yaml").write_text(yaml.safe_dump(service, sort_keys=False), encoding="utf-8")
    (source / "model.bin").write_bytes(payload)
    return source


@pytest.mark.integration
def test_server_bundle_hydrates_public_python_state_after_shutdown(tmp_path: Path) -> None:
    """Produce a complete server bundle, then hydrate it offline in Python."""
    server = WyrdTestServer(mutate_env=False)
    wyrd_server = server.__enter__()
    try:
        api_key = wyrd_server.bootstrap_service(["writer"], name="python-state-journey-client")
        cards = Cards(server_url=wyrd_server.base_url, api_key=api_key)
        source = _write_source_tree(tmp_path)
        receipt = cards.register_from_path(source)
        assert receipt.root.kind.value == "Service"

        bundle = tmp_path / "hydrated"
        env = os.environ.copy()
        env.update({"WYRD_SERVER_URL": wyrd_server.base_url, "WYRD_API_KEY": api_key})
        command = [
            sys.executable,
            "-c",
            "from wyrd.cli import run_wyrd_cli; raise SystemExit(run_wyrd_cli())",
            "get",
            "--space",
            "python-e2e",
            "--kind",
            "Service",
            "--name",
            "python-state-journey",
            "--version",
            "1.0.0",
            "--output-dir",
            str(bundle),
            "--format",
            "json",
        ]
        completed = subprocess.run(command, env=env, capture_output=True, text=True, check=False)
        assert completed.returncode == 0, completed.stderr or completed.stdout
    finally:
        server.__exit__(None, None, None)

    state = WyrdState.from_path(bundle, interfaces={"model_primary": TinyModelInterface()})
    assert state.service.kind.value == "Service"
    assert state.model("model_primary") is state.model("model_shadow")
    artifact = state.artifacts("model_primary")[0]
    assert artifact.local_path.is_file()
    assert artifact.local_path.read_bytes() == b"server-produced-model-artifact"
    assert state.model("model_primary").interface.loaded_path == artifact.local_path.parent
