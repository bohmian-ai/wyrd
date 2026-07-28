"""Public CLI-to-offline WyrdState journeys."""

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
    def predict(self, values: list[int]) -> list[int]:
        return [value + 1 for value in values]


class _Transformer:
    def transform(self, values: list[int]) -> list[int]:
        return [value * 2 for value in values]


class JourneyModelInterface(TinyModelInterface):
    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        super().load(path, load_kwargs)
        self.model, self.preprocessor, self.processor = _Predictor(), _Transformer(), _Transformer()


class JourneyDataInterface(TinyDataInterface):
    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        super().load(path, load_kwargs)
        self.data = _Transformer()


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
        name = "model-primary.yaml" if alias == "model_primary" else "model-shadow.yaml"
        return cards.register_from_path(str(self.source / name)).root

    def register_data(self, cards: Cards, alias: str) -> CardRef:
        del alias
        return cards.register_from_path(str(self.source / "training.yaml")).root

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
            refs[(str(ref.kind), ref.name)] = ref
        return refs

    def write_service_tree(self, refs: dict[tuple[str, str], CardRef]) -> Path:
        path = self.source / "typed-service.yaml"
        document = yaml.safe_load(path.read_text())

        def rewrite(value: Any) -> Any:
            if isinstance(value, dict):
                if {"kind", "name", "version"} <= value.keys():
                    ref = refs.get((str(value["kind"]), value["name"]))
                    if ref is not None:
                        return {
                            "kind": str(ref.kind),
                            "name": ref.name,
                            "version": ref.version,
                            "space": ref.space,
                            "uid": ref.uid,
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
        return self.source

    def run_cli(
        self,
        server: WyrdTestServer,
        *arguments: str,
        api_key: str | None = None,
        check: bool = True,
    ) -> dict[str, Any]:
        previous = {key: os.environ.get(key) for key in ("WYRD_SERVER_URL", "WYRD_API_KEY")}
        os.environ.update(WYRD_SERVER_URL=server.base_url, WYRD_API_KEY=api_key or server.api_key)
        old, sys.argv = sys.argv, ["wyrd", *arguments]
        out, err = io.StringIO(), io.StringIO()
        try:
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = wyrd.run_wyrd_cli()
        finally:
            sys.argv = old
            for key, value in previous.items():
                (
                    os.environ.__setitem__(key, value)
                    if value is not None
                    else os.environ.pop(key, None)
                )
        if not check:
            return {"code": code, "stdout": out.getvalue(), "stderr": err.getvalue()}
        assert code == 0, err.getvalue() or out.getvalue()
        return json.loads(out.getvalue())


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
        cards = Cards(server_url=server.base_url, api_key=server.api_key)
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
    cards = Cards(server_url=server.base_url, api_key=server.api_key)
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
        "model_primary": JourneyModelInterface(),
        "model_shadow": JourneyModelInterface(),
        "training_data": JourneyDataInterface(),
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
    assert state.workflow("runtime_workflow").spec
    assert_all_refs_are_exact_and_uid_bearing(state)
    assert_all_artifacts_are_confined_and_match_fixture(state, fixture)


@pytest.mark.integration
def test_metadata_only_bundle_is_rejected_by_python_state(tmp_path: Path) -> None:
    """Reject a bundle produced by the public metadata-only CLI mode."""
    fixture, bundle = RuntimeServiceFixture(tmp_path), tmp_path / "metadata-only"
    with WyrdTestServer(mutate_env=False) as server:
        cards = Cards(server_url=server.base_url, api_key=server.api_key)
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
    assert payload["code"] == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert payload["details"]["required"] == {"resource": "cards", "action": "read"}
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
    assert caught.value.details["stage"] == "interface"
    assert caught.value.details["card_ref"]["name"] == "primary"
