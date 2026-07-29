import wyrd
import wyrd.prompt
import wyrd.state


def test_wyrd_prompt_exposes_prompt_and_promptcard() -> None:
    """Prompt authoring types remain available from their public module."""
    assert wyrd.prompt.Prompt is wyrd.Prompt
    assert wyrd.prompt.PromptCard is wyrd.PromptCard


def test_wyrd_state_exposes_local_projections() -> None:
    """The state module exposes the offline runtime projections."""
    assert wyrd.state.WyrdState is wyrd.WyrdState
    assert wyrd.state.CardEnvelope is wyrd.CardEnvelope
    assert wyrd.state.HydratedArtifact is wyrd.HydratedArtifact


def test_top_level_runtime_task_and_embedder_are_absent() -> None:
    """Legacy runtime task and embedder exports remain absent."""
    for name in ("SkaldRuntime", "Task", "Embedder"):
        assert not hasattr(wyrd, name), name


def test_top_level_workflow_is_exposed() -> None:
    """Workflow remains available from the agent package and top level."""
    assert wyrd.Workflow is wyrd.agent.Workflow
