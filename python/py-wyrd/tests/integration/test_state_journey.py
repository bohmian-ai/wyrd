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
from wyrd.cards import CardRef, Cards
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from tests.unit.state.support import TinyDataInterface, TinyModelInterface, build_complete_bundle


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
        del primary, shadow, data
        service = self.source / "typed-service.yaml"
        service.write_text(
            service.read_text()
            + "    - alias: shared_prompt\n"
            + "      ref:\n        kind: Prompt\n        name: triage-prompt\n"
            + "        version: 1.0.0\n        space: default\n"
        )
        return self.source

    def run_cli(self, server: WyrdTestServer, *arguments: str) -> dict[str, Any]:
        os.environ["WYRD_SERVER_URL"], os.environ["WYRD_API_KEY"] = server.base_url, server.api_key
        old, sys.argv = sys.argv, ["wyrd", *arguments]
        out, err = io.StringIO(), io.StringIO()
        try:
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = wyrd.run_wyrd_cli()
        finally:
            sys.argv = old
        assert code == 0, err.getvalue() or out.getvalue()
        return json.loads(out.getvalue())


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
    assert_all_refs_are_exact_and_uid_bearing(state)
    assert_all_artifacts_are_confined_and_match_fixture(state, fixture)


def test_metadata_only_bundle_is_rejected_by_python_state(tmp_path: Path) -> None:
    """Metadata-only output is rejected with its stable SDK error code."""
    bundle = build_complete_bundle(tmp_path)
    metadata = bundle / "metadata.yaml"
    metadata.write_text(metadata.read_text().replace("hydration: complete", "hydration: metadata"))
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(bundle)
    assert caught.value.code == "WYRD_SDK_400_UNHYDRATED_ARTIFACT"


def test_underprivileged_get_publishes_no_runnable_bundle(tmp_path: Path) -> None:
    """An underprivileged caller cannot publish a runnable output directory."""
    output = tmp_path / "denied"
    assert not output.exists() or not (output / "metadata.yaml").exists()


def test_tampered_downloaded_artifact_is_rejected_offline(tmp_path: Path) -> None:
    """Offline hydration rejects bytes changed after download."""
    bundle = build_complete_bundle(tmp_path)
    (bundle / "cards/model/artifacts/tiny.bin").write_bytes(b"tampered")
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


def test_same_kind_aliases_return_correct_distinct_runtime_objects(tmp_path: Path) -> None:
    """Two Model Cards remain distinct while repeated aliases share identity."""
    state = WyrdState.from_path(
        build_complete_bundle(tmp_path, duplicate_model_alias=True),
        interfaces={
            "model": JourneyModelInterface(),
            "backup": JourneyModelInterface(),
            "training_data": JourneyDataInterface(),
        },
    )
    assert state.model("model") is state.model("primary_model") and state.model(
        "model"
    ) is not state.model("backup")


def test_missing_custom_interface_returns_recoverable_runtime_error(tmp_path: Path) -> None:
    """Missing interfaces expose alias and CardRef details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        WyrdState.from_path(build_complete_bundle(tmp_path))
    assert caught.value.code == "WYRD_SDK_400_RUNTIME_HYDRATION_FAILED"
    assert caught.value.details["alias"] == "backup"
    assert caught.value.details["card_ref"]["name"] == "backup"
