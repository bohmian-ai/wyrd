import json

import pytest
from wyrd import (
    Agent,
    FinishReason,
    Prompt,
    Role,
    RunConfig,
    SessionTurn,
    WyrdError,
    local_registry,
    tool,
)


def _prompt() -> Prompt:
    return Prompt(["hello"], "mock-model", provider="mock")


def test_role_declares_system() -> None:
    assert str(Role.System) == "system"
    assert str(Role.User) == "user"


def test_session_turn_is_keyword_only_with_declared_members() -> None:
    turn = SessionTurn(role=Role.Tool, content="out", call_id="c1")

    assert turn.role == "tool"
    assert turn.content == "out"
    assert turn.call_id == "c1"
    dumped = turn.model_dump()
    assert dumped["call_id"] == "c1"
    assert SessionTurn.model_validate(dumped).content == "out"
    assert SessionTurn.model_validate_json(turn.model_dump_json()).call_id == "c1"
    assert SessionTurn(role="user", content="hi").call_id is None

    with pytest.raises(TypeError):
        SessionTurn("user", "hi")  # ty: ignore[missing-argument, too-many-positional-arguments]


def test_wyrd_error_direct_construction_is_plain_exception() -> None:
    error = WyrdError("WYRD_AGENT_499_CALLBACK_ABORTED", "no")

    assert error.args == ("WYRD_AGENT_499_CALLBACK_ABORTED", "no")
    assert not hasattr(error, "code")
    with pytest.raises(TypeError):
        WyrdError(code="WYRD_AGENT_499_CALLBACK_ABORTED")  # ty: ignore[unknown-argument]


def test_agent_mutators_replace_in_place() -> None:
    agent = Agent(prompt=_prompt())

    agent.with_prompt(_prompt())
    agent.with_run_config(RunConfig(max_iterations=2))
    agent.set_tools([])

    assert "hello" in agent.run("hello").output


def test_agent_with_prompt_rejects_a_mapping() -> None:
    agent = Agent(prompt=_prompt())

    with pytest.raises(WyrdError) as raised:
        agent.with_prompt({"name": "p"})  # ty: ignore[invalid-argument-type]

    assert raised.value.code == "WYRD_AGENT_422_INVALID_ARGUMENT"


def test_added_callbacks_chain_in_registration_order() -> None:
    seen: list[str] = []
    agent = Agent(prompt=_prompt(), before_model_callback=lambda ctx, req: seen.append("first"))
    agent.add_before_model(lambda ctx, req: seen.append("second"))
    agent.add_before_agent(lambda ctx, text: seen.append("agent"))
    agent.add_after_model(lambda ctx, resp: seen.append("after_model"))
    agent.add_after_agent(lambda ctx, run: seen.append("after_agent"))
    agent.add_before_tool(lambda ctx, tool, args: None)
    agent.add_after_tool(lambda ctx, tool, result: None)

    agent.run("hello")

    assert seen == ["agent", "first", "second", "after_model", "after_agent"]


def test_after_model_raise_ends_run_callback_aborted_with_raised_code() -> None:
    def after_model(ctx, response):
        raise WyrdError("WYRD_AGENT_499_CALLBACK_ABORTED", "no")

    run = Agent(prompt=_prompt(), after_model_callback=after_model).run("hello")

    assert run.finish_reason == FinishReason.CallbackAborted
    assert run.output == ""
    assert run.error is not None
    assert run.error.code == "WYRD_AGENT_499_CALLBACK_ABORTED"


def test_after_agent_raise_ends_run_callback_aborted() -> None:
    def after_agent(ctx, run):
        raise RuntimeError("boom")

    run = Agent(prompt=_prompt(), after_agent_callback=after_agent).run("hello")

    assert run.finish_reason == FinishReason.CallbackAborted
    assert run.output == ""
    assert run.iterations == 1
    assert run.error is not None
    assert run.error.code == "WYRD_AGENT_422_VALIDATION"


def test_after_tool_raise_reports_failed_tool_call_and_continues() -> None:
    invoked: list[str] = []

    def record(input: str) -> str:
        invoked.append(input)
        return input

    def after_model(ctx, response):
        if ctx.iteration > 0:
            return None
        return {
            "id": "tool-call",
            "object": "chat.completion",
            "created": 0,
            "model": "mock-model",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": None,
                        "tool_calls": [
                            {
                                "id": "c1",
                                "type": "function",
                                "function": {"name": "t", "arguments": '{"input": "x"}'},
                            }
                        ],
                    },
                    "finish_reason": "tool_calls",
                }
            ],
        }

    def after_tool(ctx, tool_name, result):
        raise WyrdError("WYRD_AGENT_499_CALLBACK_ABORTED", "no")

    with local_registry():
        run = Agent(
            prompt=_prompt(),
            tools=[tool(record, name="t", description="records its input")],
            run_config=RunConfig(max_iterations=3),
            after_model_callback=after_model,
            after_tool_callback=after_tool,
        ).run("hello")

    assert invoked == ["x"]
    assert run.finish_reason == FinishReason.ModelStopped
    assert run.iterations == 2
    assert run.error is None
    assert "WYRD_AGENT_499_CALLBACK_ABORTED" in json.dumps(run.conversation)


def test_prompt_card_dumps_its_envelope_and_has_no_is_card() -> None:
    from wyrd.prompt import PromptCard

    card = PromptCard(_prompt(), space="default", name="parity-prompt", version="1.0.0")

    assert card.model_dump()["kind"] == "Prompt"
    assert not hasattr(card, "is_card")
