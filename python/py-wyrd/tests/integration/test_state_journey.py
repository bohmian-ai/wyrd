"""Real server-to-offline Python WyrdState journey."""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

import pytest
from wyrd.cards import CardKind, Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from tests.unit.state.support import TinyDataInterface, TinyModelInterface


def _write_source_tree(root: Path) -> Path:
    """Copy committed authored Cards consumed by the real workflow."""
    fixture = (
        Path(__file__).resolve().parents[4]
        / "crates/wyrd/wyrd-cli/tests/fixtures/card_lifecycle/typed_state"
    )
    source = root / "source"
    shutil.copytree(fixture, source)
    service_path = source / "typed-service.yaml"
    service_path.write_text(
        service_path.read_text().replace("name: shadow", "name: primary"),
    )
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
        for relative in (
            "triage-prompt.yaml",
            "model-primary.yaml",
            "model-shadow.yaml",
            "training.yaml",
            "agent-triage.yaml",
            "agent-inline.yaml",
            "quality.yaml",
            "model-drift.yaml",
            "runtime.yaml",
        ):
            cards.register_from_path(source / relative)
        receipt = cards.register_from_path(source / "typed-service.yaml")
        assert receipt.root.kind == CardKind.Service

        bundle = tmp_path / "hydrated"
        env = os.environ.copy()
        env.update({"WYRD_SERVER_URL": wyrd_server.base_url, "WYRD_API_KEY": api_key})
        repo_root = Path(__file__).resolve().parents[4]
        command = [
            "cargo",
            "run",
            "--quiet",
            "-p",
            "wyrd-cli",
            "--",
            "get",
            "--space",
            "default",
            "--kind",
            "Service",
            "--name",
            "typed-service",
            "--version",
            "1.0.0",
            "--output-dir",
            str(bundle),
            "--format",
            "json",
        ]
        completed = subprocess.run(
            command, cwd=repo_root, env=env, capture_output=True, text=True, check=False
        )
        assert completed.returncode == 0, completed.stderr or completed.stdout
    finally:
        server.__exit__(None, None, None)

    state = WyrdState.from_path(
        bundle,
        interfaces={
            "model_primary": TinyModelInterface(),
            "training_data": TinyDataInterface(),
        },
    )
    assert state.service.kind == CardKind.Service
    assert state.model("model_primary") is state.model("model_shadow")
    artifact = state.artifacts("model_primary")[0]
    assert artifact.local_path.is_file()
    assert artifact.local_path.read_bytes() == b"primary-model\n"
    assert state.model("model_primary").interface.loaded_path == artifact.local_path.parent
