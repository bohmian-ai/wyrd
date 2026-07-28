"""Public CLI-to-offline WyrdState journeys."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any
from uuid import uuid4

import pytest
import wyrd
import yaml
from wyrd.cards import CardRef, Cards
from wyrd.data import DataCard, DataStats, FieldSpec
from wyrd.model import ModelCard, ModelCardMetadata, ModelSignature
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from tests.unit.state.support import TinyDataInterface, TinyModelInterface


class _Predictor:
    def predict(self, values: list[int]) -> list[int]:
        return [value + 1 for value in values]


class _Transformer:
    def transform(self, values: list[int]) -> list[int]:
        return [value * 2 for value in values]


class JourneyModelInterface(TinyModelInterface):
    def __init__(self, payload: bytes) -> None:
        super().__init__()
        self.payload = payload

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> DataStats:
        del save_kwargs
        output = path / "model" / "model.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(self.payload)
        return DataStats(byte_count=len(self.payload), sha256=_sha256(self.payload))

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        super().load(path, load_kwargs)
        self.model, self.preprocessor, self.processor = _Predictor(), _Transformer(), _Transformer()


class JourneyDataInterface(TinyDataInterface):
    def __init__(self, payload: bytes) -> None:
        super().__init__()
        self.payload = payload

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> DataStats:
        del save_kwargs
        output = path / "data" / "data.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(self.payload)
        return DataStats(byte_count=len(self.payload), sha256=_sha256(self.payload))

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        super().load(path, load_kwargs)
        self.data = _Transformer()


def _sha256(payload: bytes) -> str:
    """Return the hexadecimal digest used by deterministic holder artifacts."""
    return hashlib.sha256(payload).hexdigest()


class RuntimeServiceFixture:
    """Test-owned copy of the committed typed-state graph."""

    expected_aliases = (
        "agent_inline",
        "agent_triage",
        "default-Data-training-1.0.0",
        "default-Prompt-triage-prompt-1.0.0",
        "model_drift",
        "model_primary",
        "model_shadow",
        "quality_eval",
        "root",
        "runtime_workflow",
        "shared_prompt",
        "training_data",
        "triage_prompt",
    )
    artifact_bytes = {
        "model_primary": b"primary-model\n",
        "model_shadow": b"shadow-model\n",
        "training_data": b"training-data\n",
    }

    def __init__(self, root: Path) -> None:
        source = (
            Path(__file__).resolve().parents[4]
            / "crates/wyrd/wyrd-cli/tests/fixtures/card_lifecycle/typed_state"
        )
        self.source = root / "service"
        shutil.copytree(source, self.source)
        self.bundle: Path | None = None
        self.last_get_result: dict[str, Any] | None = None
        self.artifact_count = len(self.artifact_bytes)

    def register_model(self, cards: Cards, alias: str) -> CardRef:
        name = "primary" if alias == "model_primary" else "shadow"
        payload = self.artifact_bytes[alias]
        card = ModelCard(
            JourneyModelInterface(payload),
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
        card = DataCard(
            JourneyDataInterface(self.artifact_bytes[alias]),
            space="default",
            name="training",
            version="1.0.0",
        )
        return cards.data.register(card).root

    def register_graph(self, cards: Cards) -> dict[tuple[str, str], CardRef]:
        refs: dict[tuple[str, str], CardRef] = {}
        for name in (
            "triage-prompt.yaml",
            "agent-triage.yaml",
            "agent-inline.yaml",
            "quality.yaml",
            "model-drift.yaml",
            "runtime.yaml",
        ):
            ref = cards.register_from_path(str(self.source / name)).root
            refs[(ref.kind.name, ref.name)] = ref
        return refs

    def write_service_tree(self, refs: dict[tuple[str, str], CardRef]) -> Path:
        path = self.source / "typed-service.yaml"
        document = yaml.safe_load(path.read_text())

        def rewrite(value: Any) -> Any:
            if isinstance(value, dict):
                if {"kind", "name", "version"} <= value.keys():
                    ref = refs.get((value["kind"], value["name"]))
                    if ref is not None:
                        return {
                            "kind": ref.kind.name,
                            "name": ref.name,
                            "version": ref.version,
                            "space": ref.space,
                        }
                return {key: rewrite(item) for key, item in value.items()}
            if isinstance(value, list):
                return [rewrite(item) for item in value]
            return value

        document = rewrite(document)
        document["spec"]["components"].append(
            {
                "alias": "shared_prompt",
                "ref": rewrite(
                    {
                        "kind": "Prompt",
                        "name": "triage-prompt",
                        "version": "1.0.0",
                        "space": "default",
                    }
                ),
            }
        )
        path.write_text(yaml.safe_dump(document, sort_keys=False))
        return path

    def run_cli(
        self,
        server: WyrdTestServer,
        *arguments: str,
        api_key: str | None = None,
        check: bool = True,
    ) -> dict[str, Any]:
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
    for alias, expected in fixture.artifact_bytes.items():
        artifact = state.artifacts(alias)[0]
        assert artifact.local_path.is_file()
        assert artifact.local_path.read_bytes() == expected
        assert artifact.local_path.is_relative_to(fixture.bundle.resolve())


def download_fixture(tmp_path: Path) -> tuple[RuntimeServiceFixture, Path, CardRef]:
    fixture, bundle = RuntimeServiceFixture(tmp_path), tmp_path / "wyrd-state"
    fixture.bundle = bundle
    with WyrdTestServer(mutate_env=False) as server:
        cards = Cards(server_url=server.base_url, api_key=writer_api_key(server))
        refs = {
            ("Model", "primary"): fixture.register_model(cards, "model_primary"),
            ("Model", "shadow"): fixture.register_model(cards, "model_shadow"),
            ("Data", "training"): fixture.register_data(cards, "training_data"),
        }
        refs.update(fixture.register_graph(cards))
        applied = fixture.run_cli(
            server, "apply", str(fixture.write_service_tree(refs)), "--format", "json"
        )
        service_ref = CardRef(**applied["root"])
        fixture.last_get_result = fixture.run_cli(server, *exact_get_arguments(service_ref, bundle))
    return fixture, bundle, service_ref


def register_and_apply(
    fixture: RuntimeServiceFixture, server: WyrdTestServer, bundle: Path
) -> CardRef:
    """Register and apply the fixture graph against the supplied live server."""
    cards = Cards(server_url=server.base_url, api_key=writer_api_key(server))
    refs = {
        ("Model", "primary"): fixture.register_model(cards, "model_primary"),
        ("Model", "shadow"): fixture.register_model(cards, "model_shadow"),
        ("Data", "training"): fixture.register_data(cards, "training_data"),
    }
    refs.update(fixture.register_graph(cards))
    receipt = fixture.run_cli(
        server, "apply", str(fixture.write_service_tree(refs)), "--format", "json"
    )
    service_ref = CardRef(**receipt["root"])
    fixture.run_cli(server, *exact_get_arguments(service_ref, bundle))
    return service_ref


def _interfaces() -> dict[str, Any]:
    return {
        "model_primary": JourneyModelInterface(
            RuntimeServiceFixture.artifact_bytes["model_primary"]
        ),
        "model_shadow": JourneyModelInterface(RuntimeServiceFixture.artifact_bytes["model_shadow"]),
        "training_data": JourneyDataInterface(
            RuntimeServiceFixture.artifact_bytes["training_data"]
        ),
    }


@pytest.mark.integration
def test_service_bundle_hydrates_complete_python_runtime_offline(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Hydrate the complete CLI bundle into usable offline Python objects."""
    fixture, bundle, service_ref = download_fixture(tmp_path)
    monkeypatch.setenv("WYRD_SERVER_URL", "http://127.0.0.1:1")
    state = WyrdState.from_path(bundle, interfaces=_interfaces())
    assert fixture.last_get_result is not None
    assert fixture.last_get_result["mode"] == "complete"
    assert fixture.last_get_result["card_count"] == 10
    assert fixture.last_get_result["downloaded_artifact_count"] == fixture.artifact_count
    assert state.service.card_ref == service_ref
    assert state.aliases == fixture.expected_aliases
    assert state.model("model_primary").model.predict([1]) == [2]
    assert state.model("model_shadow").model.predict([1]) == [2]
    assert state.model("model_primary").preprocessor.transform([1]) == [2]
    assert state.model("model_shadow").processor.transform([1]) == [2]
    assert state.data("training_data").data.transform([1]) == [2]
    assert (
        state.agent("agent_triage").prompt is not None
        and state.agent("agent_inline").prompt is not None
    )
    assert state.prompt("triage_prompt").prompt is not None
    assert state.prompt("triage_prompt") is state.prompt("default-Prompt-triage-prompt-1.0.0")
    assert state.prompt("triage_prompt") is state.prompt("shared_prompt")
    assert state.model("model_primary") is not state.model("model_shadow")
    assert state.eval("quality_eval").kind is wyrd.CardKind.Eval
    assert state.eval("quality_eval").spec
    assert state.drift("model_drift").kind is wyrd.CardKind.Drift
    assert state.drift("model_drift").spec
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
        refs = {
            ("Model", "primary"): fixture.register_model(cards, "model_primary"),
            ("Model", "shadow"): fixture.register_model(cards, "model_shadow"),
            ("Data", "training"): fixture.register_data(cards, "training_data"),
        }
        refs.update(fixture.register_graph(cards))
        receipt = fixture.run_cli(
            server, "apply", str(fixture.write_service_tree(refs)), "--format", "json"
        )
        fixture.run_cli(server, *exact_get_arguments(CardRef(**receipt["root"]), bundle, True))
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
    _, bundle, _ = download_fixture(tmp_path)
    artifact = next(bundle.rglob("*.bin"))
    artifact.write_bytes(b"tampered")
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle, interfaces=_interfaces())
    assert caught.value.code == "WYRD_SDK_400_INVALID_STATE_BUNDLE"
    assert caught.value.details["path"]


@pytest.mark.integration
def test_same_kind_aliases_return_correct_distinct_runtime_objects(tmp_path: Path) -> None:
    """Keep distinct same-kind Cards distinct at runtime."""
    state = WyrdState.from_path(download_fixture(tmp_path)[1], interfaces=_interfaces())
    assert state.model("model_primary") is not state.model("model_shadow")


@pytest.mark.integration
def test_missing_custom_interface_returns_recoverable_runtime_error(tmp_path: Path) -> None:
    """Report a stable recoverable error when a custom loader is absent."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(download_fixture(tmp_path)[1])
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "model_primary"
    assert caught.value.details["stage"] == "artifact_load"
    assert caught.value.details["card_ref"]["name"] == "primary"
