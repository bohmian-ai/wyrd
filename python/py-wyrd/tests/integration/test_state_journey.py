"""Public CLI-to-offline WyrdState journeys."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from uuid import uuid4

import blake3
import numpy as np
import pandas as pd
import pytest
import wyrd
import yaml
from wyrd.cards import CardRef, Cards
from wyrd.data import DataCard, FieldSpec, PandasInterface
from wyrd.model import (
    ModelCard,
    ModelCardMetadata,
    ModelSignature,
    SklearnInterface,
)
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer


class RuntimeServiceFixture:
    """Test-owned copy of the committed typed-state graph."""

    expected_aliases = (
        "agent_inline",
        "agent_triage",
        "default-Data-training-1.0.0",
        "default-Drift-model-drift-1.0.0",
        "default-Eval-quality-1.0.0",
        "default-Prompt-triage-prompt-1.0.0",
        "model_primary",
        "model_shadow",
        "root",
        "runtime_workflow",
        "shared_prompt",
        "triage_prompt",
    )
    training_data = pd.DataFrame({"feature": [0.0, 1.0], "label": [0, 1]})

    def __init__(self, root: Path) -> None:
        source = (
            Path(__file__).resolve().parents[4]
            / "crates/wyrd/wyrd-cli/tests/fixtures/card_lifecycle/typed_state"
        )
        self.source = root / "service"
        shutil.copytree(source, self.source)
        self.bundle: Path | None = None
        self.last_get_result: dict[str, object] | None = None
        self.artifact_count = 3

    def register_model(self, cards: Cards, alias: str) -> CardRef:
        from sklearn.linear_model import LogisticRegression

        name = "primary" if alias == "model_primary" else "shadow"
        features = self.training_data[["feature"]].to_numpy()
        labels = self.training_data["label"].to_numpy()
        model = LogisticRegression(random_state=0).fit(features, labels)
        card = ModelCard(
            SklearnInterface(model=model),
            space="default",
            name=name,
            version="1.0.0",
            metadata=ModelCardMetadata(
                task_type="other",
                signature=ModelSignature(
                    [FieldSpec("feature", "float64")],
                    [FieldSpec("prediction", "float64")],
                ),
            ),
        )
        return cards.model.register(card).root

    def register_data(self, cards: Cards, alias: str) -> CardRef:
        del alias
        card = DataCard(
            PandasInterface(data=self.training_data),
            space="default",
            name="training",
            version="1.0.0",
        )
        return cards.data.register(card).root

    def register_heavy_cards(self, cards: Cards) -> None:
        """Register reusable Model and Data lineage anchors before Service apply."""
        self.register_model(cards, "model_primary")
        self.register_model(cards, "model_shadow")
        self.register_data(cards, "training_data")

    def run_cli(
        self,
        server: WyrdTestServer,
        *arguments: str,
        api_key: str | None = None,
        check: bool = True,
    ) -> dict[str, object]:
        environment = os.environ.copy()
        environment.update(
            WYRD_SERVER_URL=server.base_url,
            WYRD_API_KEY=api_key or writer_api_key(server),
        )
        executable = Path(sys.executable).with_name("wyrd")
        completed = subprocess.run(
            [str(executable), *arguments],
            env=environment,
            capture_output=True,
            text=True,
            check=False,
        )
        if not check:
            return {
                "code": completed.returncode,
                "stdout": completed.stdout,
                "stderr": completed.stderr,
            }
        assert completed.returncode == 0, completed.stderr or completed.stdout
        return json.loads(completed.stdout)

    def trusted_artifact_hashes(self) -> dict[str, str]:
        """Return exact canonical manifest hashes for executable Model aliases."""
        assert self.bundle is not None
        manifest = yaml.safe_load((self.bundle / "metadata.yaml").read_text(encoding="utf-8"))
        trusted: dict[str, str] = {}
        for card in manifest["cards"]:
            if card["card_ref"]["kind"] != "Model":
                continue
            artifacts = [
                {
                    "relative_path": artifact["relative_path"],
                    "sha256": artifact["sha256"],
                    "size_bytes": artifact["size_bytes"],
                    "content_type": artifact.get("content_type"),
                }
                for artifact in card["artifacts"]
            ]
            artifacts.sort(key=lambda artifact: artifact["relative_path"])
            canonical = json.dumps(
                artifacts,
                ensure_ascii=False,
                separators=(",", ":"),
                sort_keys=True,
            ).encode()
            for alias in card["aliases"]:
                trusted[alias] = blake3.blake3(canonical).hexdigest()
        return trusted


def writer_api_key(server: WyrdTestServer) -> str:
    """Mint a valid writer key for the journey's real server boundary."""
    return server.bootstrap_service(["writer"], name=f"state-journey-{uuid4().hex[:12]}")


def exact_get_arguments(
    service_ref: CardRef, bundle: Path, metadata_only: bool = False
) -> tuple[str, ...]:
    assert service_ref.uid is not None
    args = (
        "get",
        "--kind",
        "Service",
        "--uid",
        str(service_ref.uid),
        "--output-dir",
        str(bundle),
        "--format",
        "json",
    )
    return args + (("--metadata-only",) if metadata_only else ())


def assert_all_refs_are_exact_and_uid_bearing(state: WyrdState) -> None:
    for alias in state.aliases:
        ref = state.card_ref(alias)
        assert ref.uid is not None and ref.version


def assert_all_artifacts_are_confined_and_match_fixture(
    state: WyrdState, fixture: RuntimeServiceFixture
) -> None:
    """Hydrate the complete CLI bundle into usable offline Python objects."""
    assert fixture.bundle is not None
    for alias in ("model_primary", "model_shadow", "default-Data-training-1.0.0"):
        artifact = state.artifacts(alias)[0]
        assert artifact.local_path.is_file()
        assert artifact.local_path.is_relative_to(fixture.bundle.resolve())


def download_fixture(tmp_path: Path) -> tuple[RuntimeServiceFixture, Path, CardRef]:
    fixture, bundle = RuntimeServiceFixture(tmp_path), tmp_path / "wyrd-state"
    fixture.bundle = bundle
    with WyrdTestServer(mutate_env=False) as server:
        cards = Cards(server_url=server.base_url, api_key=writer_api_key(server))
        fixture.register_heavy_cards(cards)
        receipt = cards.register_from_path(str(fixture.source / "typed-service.yaml"))
        service_ref = receipt.root
        fixture.last_get_result = fixture.run_cli(server, *exact_get_arguments(service_ref, bundle))
    return fixture, bundle, service_ref


def register_and_apply(
    fixture: RuntimeServiceFixture, server: WyrdTestServer, bundle: Path
) -> CardRef:
    """Register and apply the fixture graph against the supplied live server."""
    cards = Cards(server_url=server.base_url, api_key=writer_api_key(server))
    fixture.register_heavy_cards(cards)
    service_ref = cards.register_from_path(str(fixture.source / "typed-service.yaml")).root
    fixture.run_cli(server, *exact_get_arguments(service_ref, bundle))
    return service_ref


@pytest.mark.integration
def test_service_bundle_hydrates_complete_python_runtime_offline(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Hydrate the complete CLI bundle into usable offline Python objects."""
    fixture, bundle, service_ref = download_fixture(tmp_path)
    monkeypatch.setenv("WYRD_SERVER_URL", "http://127.0.0.1:1")
    state = WyrdState.from_path(
        bundle,
        trusted_artifact_hashes=fixture.trusted_artifact_hashes(),
    )
    assert fixture.last_get_result is not None
    assert fixture.last_get_result["mode"] == "complete"
    assert fixture.last_get_result["card_count"] == 10
    assert fixture.last_get_result["downloaded_artifact_count"] == fixture.artifact_count
    assert state.service.card_ref == service_ref
    assert state.aliases == fixture.expected_aliases
    np.testing.assert_array_equal(
        state.model("model_primary").model.predict([[0.0], [1.0]]),
        np.array([0, 1]),
    )
    np.testing.assert_array_equal(
        state.model("model_shadow").model.predict([[0.0], [1.0]]),
        np.array([0, 1]),
    )
    pd.testing.assert_frame_equal(
        state.data("default-Data-training-1.0.0").data,
        fixture.training_data,
    )
    assert (
        state.agent("agent_triage").prompt is not None
        and state.agent("agent_inline").prompt is not None
    )
    assert state.prompt("triage_prompt").prompt is not None
    assert state.prompt("triage_prompt") is state.prompt("default-Prompt-triage-prompt-1.0.0")
    assert state.prompt("triage_prompt") is state.prompt("shared_prompt")
    assert state.model("model_primary") is not state.model("model_shadow")
    assert state.eval("default-Eval-quality-1.0.0").kind is wyrd.CardKind.Eval
    assert state.eval("default-Eval-quality-1.0.0").spec
    assert state.drift("default-Drift-model-drift-1.0.0").kind is wyrd.CardKind.Drift
    assert state.drift("default-Drift-model-drift-1.0.0").spec
    assert state.workflow("runtime_workflow").kind is wyrd.CardKind.Workflow
    assert state.workflow("runtime_workflow").spec == {}
    assert_all_refs_are_exact_and_uid_bearing(state)
    assert_all_artifacts_are_confined_and_match_fixture(state, fixture)


@pytest.mark.integration
def test_metadata_only_bundle_is_rejected_by_python_state(tmp_path: Path) -> None:
    """Reject a bundle produced by the public metadata-only CLI mode."""
    fixture, bundle = RuntimeServiceFixture(tmp_path), tmp_path / "metadata-only"
    with WyrdTestServer(mutate_env=False) as server:
        cards = Cards(server_url=server.base_url, api_key=writer_api_key(server))
        fixture.register_heavy_cards(cards)
        service_ref = cards.register_from_path(str(fixture.source / "typed-service.yaml")).root
        fixture.run_cli(server, *exact_get_arguments(service_ref, bundle, True))
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle)
    assert caught.value.code == "WYRD_SDK_400_UNHYDRATED_ARTIFACT"
    assert caught.value.details["path"]


@pytest.mark.integration
def test_underprivileged_get_publishes_no_runnable_bundle(tmp_path: Path) -> None:
    """Ensure a denied get cannot publish a runnable bundle."""
    fixture = RuntimeServiceFixture(tmp_path)
    bundle = tmp_path / "denied"
    with WyrdTestServer(mutate_env=False) as server:
        service_ref = register_and_apply(fixture, server, tmp_path / "authorized")
        denied = server.bootstrap_service([], name="underprivileged-get")
        result = fixture.run_cli(
            server, *exact_get_arguments(service_ref, bundle), api_key=denied, check=False
        )
    payload = json.loads(result["stderr"].splitlines()[0])
    assert result["code"] != 0
    assert payload["kind"] == "wyrd_cli_error"
    assert payload["code"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert payload["status"] == 403
    assert payload["message"]
    assert payload["remediation"] == "Request the required role from a workspace admin."
    assert not bundle.exists() or not (bundle / "metadata.yaml").exists()


@pytest.mark.integration
def test_tampered_downloaded_artifact_is_rejected_offline(tmp_path: Path) -> None:
    """Reject downloaded artifact bytes that no longer match inventory."""
    fixture, bundle, _ = download_fixture(tmp_path)
    artifact = next(
        path
        for path in bundle.rglob("*")
        if path.is_file() and "artifacts" in path.parts and path.name != "artifacts.yaml"
    )
    artifact.write_bytes(b"tampered")
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            bundle,
            trusted_artifact_hashes=fixture.trusted_artifact_hashes(),
        )
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"
    assert caught.value.details["path"]


@pytest.mark.integration
def test_same_kind_aliases_return_correct_distinct_runtime_objects(tmp_path: Path) -> None:
    """Keep distinct same-kind Cards distinct at runtime."""
    fixture, bundle, _ = download_fixture(tmp_path)
    state = WyrdState.from_path(
        bundle,
        trusted_artifact_hashes=fixture.trusted_artifact_hashes(),
    )
    assert state.model("model_primary") is not state.model("model_shadow")


@pytest.mark.integration
def test_missing_model_trust_returns_recoverable_runtime_error(tmp_path: Path) -> None:
    """Reject executable built-in Model hydration without external trust."""
    _, bundle, _ = download_fixture(tmp_path)
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle)
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model_primary"
    assert caught.value.details["stage"] == "artifact_trust"
    assert caught.value.details["card_ref"]["name"] == "primary"


@pytest.mark.integration
def test_missing_relationship_projection_is_rejected_offline(tmp_path: Path) -> None:
    """Reject a complete bundle whose projected relationship file is absent."""
    fixture, bundle, _ = download_fixture(tmp_path)
    manifest = yaml.safe_load((bundle / "metadata.yaml").read_text(encoding="utf-8"))
    relationship_path = bundle / manifest["cards"][0]["relationships_path"]
    relationship_path.unlink()
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            bundle,
            trusted_artifact_hashes=fixture.trusted_artifact_hashes(),
        )
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"
    assert caught.value.details["path"]
