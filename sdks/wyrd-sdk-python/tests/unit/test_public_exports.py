"""Public modules export what they list, and the package root re-exports them."""

import importlib

import pytest
import wyrd

PUBLIC_MODULES = [
    "wyrd",
    "wyrd.bifrost",
    "wyrd.cards",
    "wyrd.cli",
    "wyrd.client",
    "wyrd.config",
    "wyrd.data",
    "wyrd.gateway_admin",
    "wyrd.model",
    "wyrd.observe",
    "wyrd.operators",
    "wyrd.auth",
    "wyrd.prompt",
    "wyrd.state",
    "wyrd.testing",
    "wyrd.testing.cli",
    "wyrd.verify",
]


@pytest.mark.parametrize("module_name", PUBLIC_MODULES)
def test_every_listed_export_resolves(module_name: str) -> None:
    module = importlib.import_module(module_name)

    assert [name for name in module.__all__ if getattr(module, name, None) is None] == []


@pytest.mark.parametrize(
    ("module_name", "name"),
    [
        ("wyrd.cards", "AgentCard"),
        ("wyrd.cards", "Cards"),
        ("wyrd.config", "WyrdConfig"),
        ("wyrd.data", "DataCard"),
        ("wyrd.model", "ModelCard"),
        ("wyrd.prompt", "AnthropicSettings"),
        ("wyrd.prompt", "GeminiSettings"),
        ("wyrd.prompt", "OpenAISettings"),
        ("wyrd.prompt", "Prompt"),
        ("wyrd.prompt", "PromptCard"),
        ("wyrd.state", "CardEnvelope"),
        ("wyrd.state", "WyrdState"),
    ],
)
def test_root_reexport_is_the_module_export(module_name: str, name: str) -> None:
    assert getattr(wyrd, name) is getattr(importlib.import_module(module_name), name)


def test_state_declares_and_lists_no_artifact_files() -> None:
    assert callable(wyrd.WyrdState.declare)
    assert not hasattr(wyrd.WyrdState, "artifacts")
    assert not hasattr(importlib.import_module("wyrd.state"), "HydratedArtifact")
    assert not hasattr(wyrd, "HydratedArtifact")
