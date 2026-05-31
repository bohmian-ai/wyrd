import importlib

import pytest
import wyrd
import wyrd.prompt


def test_wyrd_prompt_exposes_prompt_and_promptcard() -> None:
    assert wyrd.prompt.Prompt is wyrd.Prompt
    assert wyrd.prompt.PromptCard is wyrd.PromptCard


def test_importing_wyrd_runtime_fails() -> None:
    with pytest.raises(ModuleNotFoundError):
        importlib.import_module("wyrd" + ".runtime")


def test_top_level_runtime_agent_workflow_task_and_embedder_are_absent() -> None:
    for name in ("SkaldRuntime", "Agent", "Workflow", "Task", "Embedder"):
        assert not hasattr(wyrd, name), name
