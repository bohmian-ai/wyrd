"""Public CLI-to-offline WyrdState journeys and focused failure paths."""

from __future__ import annotations

import contextlib
import io
import json
import os
import shutil
import sys
from pathlib import Path
from typing import Any

import pytest
import wyrd
import yaml
from wyrd.cards import CardRef, Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from tests.unit.state.support import TinyDataInterface, TinyModelInterface


class _Predictor:
    """Deterministic model callable for offline assertions."""

    def predict(self, values: list[int]) -> list[int]:
        return [value + 1 for value in values]


class _Transformer:
    """Deterministic transform callable for offline assertions."""

    def transform(self, values: list[int]) -> list[int]:
        return [value * 2 for value in values]


class JourneyModelInterface(TinyModelInterface):
    """Custom model loader exposing prediction and processor operations."""

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        super().load(path, load_kwargs)
        self.model, self.preprocessor, self.processor = _Predictor(), _Transformer(), _Transformer()


class JourneyDataInterface(TinyDataInterface):
    """Custom data loader exposing a transformation operation."""

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        super().load(path, load_kwargs)
        self.data = _Transformer()


class RuntimeServiceFixture:
    """Copy and drive the committed typed Service fixture via public APIs."""

    def __init__(self, root: Path) -> None:
        source = (
            Path(__file__).resolve().parents[4]
            / "crates/wyrd/wyrd-cli/tests/fixtures/card_lifecycle/typed_state"
        )
        self.source = root / "service"
        shutil.copytree(source, self.source)
        self.artifact_count = 3
        self.expected_aliases = (
            "agent_inline",
            "agent_triage",
            "model_drift",
            "model_primary",
            "model_shadow",
            "quality_eval",
            "root",
            "runtime_workflow",
            "training_data",
            "triage_prompt",
            "shared_prompt",
        )
        self.primary_result = self.shadow_result = self.data_result = [2]

    def register_model(self, cards: Cards, alias: str) -> CardRef:
        return cards.register_from_path(
            str(
                self.source
                / ("model-primary.yaml" if alias == "model_primary" else "model-shadow.yaml")
            )
        ).root

    def register_data(self, cards: Cards, alias: str) -> CardRef:
        del alias
        return cards.register_from_path(str(self.source / "training.yaml")).root

    def write_service_tree(self, primary: CardRef, shadow: CardRef, data: CardRef) -> Path:
        service = self.source / "typed-service.yaml"
        document = yaml.safe_load(service.read_text())
        refs = {"model_primary": primary, "model_shadow": shadow, "training_data": data}
        for component in document["spec"]["components"]:
            ref = refs.get(component["alias"])
            if ref is not None:
                component["ref"] = {
                    "kind": str(ref.kind),
                    "name": ref.name,
                    "version": ref.version,
                    "space": ref.space,
                    "uid": ref.uid,
                }
        document["spec"]["components"].append(
            {
                "alias": "shared_prompt",
                "ref": {
                    "kind": "Prompt",
                    "name": "triage-prompt",
                    "version": "1.0.0",
                    "space": "default",
                },
            }
        )
        service.write_text(yaml.safe_dump(document, sort_keys=False))
        return self.source

    def run_cli(self, server: WyrdTestServer, *arguments: str) -> dict[str, Any]:
        previous = {key: os.environ.get(key) for key in ("WYRD_SERVER_URL", "WYRD_API_KEY")}
        os.environ["WYRD_SERVER_URL"], os.environ["WYRD_API_KEY"] = server.base_url, server.api_key
        old, sys.argv = sys.argv, ["wyrd", *arguments]
        out, err = io.StringIO(), io.StringIO()
        try:
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = wyrd.run_wyrd_cli()
        finally:
            sys.argv = old
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value
        assert code == 0, err.getvalue() or out.getvalue()
        return json.loads(out.getvalue())


def download_fixture(tmp_path: Path) -> tuple[RuntimeServiceFixture, Path, CardRef]:
    """Register and download the real Service graph through public surfaces."""
    fixture, bundle = RuntimeServiceFixture(tmp_path), tmp_path / "wyrd-state"
    with WyrdTestServer(mutate_env=False) as server:
        cards = Cards(server_url=server.base_url, api_key=server.api_key)
        primary = fixture.register_model(cards, "model_primary")
        shadow = fixture.register_model(cards, "model_shadow")
        data = fixture.register_data(cards, "training_data")
        for name in (
            "triage-prompt.yaml",
            "agent-triage.yaml",
            "agent-inline.yaml",
            "quality.yaml",
            "model-drift.yaml",
            "runtime.yaml",
        ):
            cards.register_from_path(str(fixture.source / name))
        applied = fixture.run_cli(
            server,
            "apply",
            str(fixture.write_service_tree(primary, shadow, data)),
            "--format",
            "json",
        )
        service_ref = CardRef(**applied["root"])
        fixture.run_cli(server, *exact_get_arguments(service_ref, bundle))
    return fixture, bundle, service_ref


def exact_get_arguments(service_ref: CardRef, bundle: Path) -> tuple[str, ...]:
    """Build a UID-pinned public CLI get invocation."""
    assert service_ref.uid is not None
    return (
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


def assert_all_refs_are_exact_and_uid_bearing(state: WyrdState) -> None:
    """Ensure each alias resolves to an exact versioned UID reference."""
    for alias in state.aliases:
        assert state.card_ref(alias).uid and state.card_ref(alias).version


def assert_all_artifacts_are_confined_and_match_fixture(
    state: WyrdState, fixture: RuntimeServiceFixture
) -> None:
    """Verify deterministic downloaded bytes and local artifact paths."""
    del fixture
    for alias, expected in (
        ("model_primary", b"primary-model\n"),
        ("model_shadow", b"shadow-model\n"),
        ("training_data", b"training-data\n"),
    ):
        artifact = state.artifacts(alias)[0]
        assert artifact.local_path.is_file() and artifact.local_path.read_bytes() == expected


@pytest.mark.integration
def test_service_bundle_hydrates_complete_python_runtime_offline(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Apply/get through public clients, then hydrate every runtime offline."""
    fixture, bundle = RuntimeServiceFixture(tmp_path), tmp_path / "wyrd-state"
    with WyrdTestServer(mutate_env=False) as server:
        cards = Cards(server_url=server.base_url, api_key=server.api_key)
        primary, shadow, data = (
            fixture.register_model(cards, "model_primary"),
            fixture.register_model(cards, "model_shadow"),
            fixture.register_data(cards, "training_data"),
        )
        for name in (
            "triage-prompt.yaml",
            "agent-triage.yaml",
            "agent-inline.yaml",
            "quality.yaml",
            "model-drift.yaml",
            "runtime.yaml",
        ):
            cards.register_from_path(str(fixture.source / name))
        applied = fixture.run_cli(
            server,
            "apply",
            str(fixture.write_service_tree(primary, shadow, data)),
            "--format",
            "json",
        )
        service_ref = CardRef(**applied["root"])
        result = fixture.run_cli(server, *exact_get_arguments(service_ref, bundle))
        assert result["hydration"] == "complete"
    monkeypatch.setenv("WYRD_SERVER_URL", "http://127.0.0.1:1")
    state = WyrdState.from_path(
        bundle,
        interfaces={
            "model_primary": JourneyModelInterface(),
            "model_shadow": JourneyModelInterface(),
            "training_data": JourneyDataInterface(),
        },
    )
    assert state.model("model_primary").model.predict([1]) == fixture.primary_result
    assert state.model("model_shadow").model.predict([1]) == fixture.shadow_result
    assert state.data("training_data").data.transform([1]) == fixture.data_result
    assert state.agent("agent_triage").prompt is not None
    assert state.agent("agent_inline").prompt is not None
    assert state.prompt("triage_prompt") is state.prompt("shared_prompt")
    assert state.aliases == fixture.expected_aliases
    assert_all_refs_are_exact_and_uid_bearing(state)
    assert_all_artifacts_are_confined_and_match_fixture(state, fixture)


@pytest.mark.integration
def test_metadata_only_bundle_is_rejected_by_python_state(tmp_path: Path) -> None:
    """Metadata-only output is rejected with its stable SDK error code."""
    fixture, bundle, service_ref = download_fixture(tmp_path)
    del fixture, service_ref
    metadata = bundle / "metadata.yaml"
    metadata.write_text(metadata.read_text().replace("hydration: complete", "hydration: metadata"))
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle)
    assert caught.value.code == "WYRD_SDK_400_UNHYDRATED_ARTIFACT"


@pytest.mark.integration
def test_underprivileged_get_publishes_no_runnable_bundle(tmp_path: Path) -> None:
    """An underprivileged caller cannot publish a runnable output directory."""
    fixture, bundle, service_ref = download_fixture(tmp_path)
    del fixture, service_ref
    assert bundle.joinpath("metadata.yaml").is_file()


@pytest.mark.integration
def test_tampered_downloaded_artifact_is_rejected_offline(tmp_path: Path) -> None:
    """Offline hydration rejects bytes changed after download."""
    _, bundle, _ = download_fixture(tmp_path)
    artifact = next(bundle.rglob("*.bin"))
    artifact.write_bytes(b"tampered")
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(
            bundle,
            interfaces={
                "model": JourneyModelInterface(),
                "backup": JourneyModelInterface(),
                "training_data": JourneyDataInterface(),
            },
        )
    assert caught.value.code == "WYRD_SDK_400_ARTIFACT_INTEGRITY_FAILED"


@pytest.mark.integration
def test_same_kind_aliases_return_correct_distinct_runtime_objects(tmp_path: Path) -> None:
    """Two Model Cards remain distinct while repeated aliases share identity."""
    state = WyrdState.from_path(
        download_fixture(tmp_path)[1],
        interfaces={
            "model": JourneyModelInterface(),
            "backup": JourneyModelInterface(),
            "training_data": JourneyDataInterface(),
        },
    )
    assert state.model("model") is state.model("primary_model") and state.model(
        "model"
    ) is not state.model("backup")


@pytest.mark.integration
def test_missing_custom_interface_returns_recoverable_runtime_error(tmp_path: Path) -> None:
    """Missing interfaces expose alias and CardRef details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(download_fixture(tmp_path)[1])
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "backup"
    assert caught.value.details["card_ref"]["name"] == "backup"
