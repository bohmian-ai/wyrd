import wyrd
import wyrd.prompt
import wyrd.runtime


def test_wyrd_prompt_exposes_prompt_and_promptcard() -> None:
    assert wyrd.prompt.Prompt is wyrd.Prompt
    assert wyrd.prompt.PromptCard is wyrd.PromptCard


def test_wyrd_runtime_exposes_local_state() -> None:
    assert wyrd.runtime.WyrdState is wyrd.WyrdState


def test_top_level_runtime_task_and_embedder_are_absent() -> None:
    for name in ("SkaldRuntime", "Task", "Embedder"):
        assert not hasattr(wyrd, name), name


def test_top_level_workflow_is_exposed() -> None:
    assert wyrd.Workflow is wyrd.agent.Workflow
