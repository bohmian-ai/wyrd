"""Public modules export what they list, and the package root re-exports them."""

import importlib
import subprocess
import sys

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
    "wyrd.prompt.decisions",
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


def test_verify_exports_definitions_and_the_original_prompt() -> None:
    verify = importlib.import_module("wyrd.verify")

    assert verify.Prompt is wyrd.prompt.Prompt
    assert verify.ResponseFormat is wyrd.prompt.ResponseFormat
    assert not hasattr(verify, "CardDefinition")
    assert not hasattr(importlib.import_module("wyrd._wyrd.verify"), "CardDefinition")
    with pytest.raises(ModuleNotFoundError):
        importlib.import_module("wyrd.authoring")
    assert not hasattr(importlib.import_module("wyrd._wyrd"), "authoring")


DICTIONARY_ONLY_DECISIONS = """
import sys

sys.modules["pydantic"] = None
from wyrd.prompt import Prompt
from wyrd.verify import Action, Agent, Check

prompt = Prompt.jev(
    "jev-latest",
    state="${context}",
    questions={"helpful": {"type": "noul", "instructions": "Does the reply help?"}},
)
judge = Agent("judge", "1.0.0", prompt=prompt)
Action.agent(judge)
Check.agent(
    judge,
    verdict_transform={
        "kind": "probability", "question": "helpful", "expected": True,
        "pass_at": 0.9, "fail_at": 0.1,
    },
)
try:
    from wyrd.verify import ProbabilityTransform
except ImportError:
    print("models need pydantic")
"""
"""Jev authoring from dictionaries alone, in an interpreter where Pydantic cannot import."""


def test_decision_authoring_from_dictionaries_needs_no_pydantic() -> None:
    ran = subprocess.run(
        [sys.executable, "-c", DICTIONARY_ONLY_DECISIONS], capture_output=True, text=True
    )

    assert (ran.returncode, ran.stdout, ran.stderr) == (0, "models need pydantic\n", "")
