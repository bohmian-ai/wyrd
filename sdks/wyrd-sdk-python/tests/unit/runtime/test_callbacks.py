"""Agent callbacks see a typed context and can replace or abort a model call."""

from wyrd import Agent, FinishReason, Prompt, ProviderResponse, WyrdError
from wyrd.agent import CallbackContext, Role


def test_callback_context_is_typed() -> None:
    seen: list[CallbackContext] = []

    def before_model(ctx, request):
        seen.append(ctx)
        return None

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        before_model_callback=before_model,
    )
    agent.run("hello")

    ctx = seen[0]
    assert isinstance(ctx, CallbackContext)
    assert ctx.agent_id == agent.id
    assert ctx.session_id is None
    assert ctx.iteration == 0
    assert len(ctx.conversation) == len(ctx.conversation.turns) >= 1
    user_turns = [turn for turn in ctx.conversation.turns if turn.role == Role.User]
    assert user_turns
    assert user_turns[-1].content == "hello"
    assert user_turns[-1].to_dict()["type"] == "user"


def test_after_model_replace_with_changes_output() -> None:
    def after_model(ctx, response):
        return ProviderResponse.text("synthetic")

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        after_model_callback=after_model,
    )

    run = agent.run("hello")

    assert run.output == "synthetic"


def test_before_model_raise_aborts_run() -> None:
    def before_model(ctx, request):
        raise WyrdError("WYRD_AGENT_499_CALLBACK_ABORTED", "no")

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        before_model_callback=before_model,
    )

    run = agent.run("hello")

    assert run.finish_reason == FinishReason.CallbackAborted
    assert run.error is not None
    assert run.error.code == "WYRD_AGENT_499_CALLBACK_ABORTED"
