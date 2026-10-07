"""An Agent reads and appends its turns through a caller-supplied session memory."""

import pytest
from wyrd import Agent, AgentError, Prompt, Role, SessionTurn

DEFAULT_HISTORY_LIMIT = 50


class RecordingMemory:
    def __init__(self, recent_turns=None) -> None:
        self.recent_turns = list(recent_turns or [])
        self.recent_calls: list[tuple[str, int]] = []
        self.append_calls: list[tuple[str, SessionTurn]] = []

    def recent(self, session_id: str, limit: int):
        self.recent_calls.append((session_id, limit))
        return self.recent_turns[-limit:]

    def append(self, session_id: str, turn: SessionTurn) -> None:
        self.append_calls.append((session_id, turn))


def test_session_turn_is_python_constructible_and_serializable() -> None:
    turn = SessionTurn(role=Role.User, content="hello")

    assert turn.role == "user"
    assert turn.content == "hello"
    assert turn.call_id is None
    assert turn.model_dump() == {
        "role": "user",
        "content": "hello",
        "call_id": None,
    }
    assert SessionTurn.model_validate_json(turn.model_dump_json()).model_dump() == turn.model_dump()


def test_python_session_receives_appends() -> None:
    memory = RecordingMemory()
    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        session=memory,
    )

    agent.run("hello", session_id="s1")

    assert memory.recent_calls == [("s1", DEFAULT_HISTORY_LIMIT)]
    assert [turn.role for _, turn in memory.append_calls] == ["user", "assistant"]
    assert [session_id for session_id, _ in memory.append_calls] == ["s1", "s1"]


def test_python_session_recent_can_return_dicts() -> None:
    memory = RecordingMemory(
        [
            {"role": "system", "content": "seed"},
            {"role": "user", "content": "prior"},
        ]
    )
    seen_contexts = []

    def before_model(ctx, request):
        seen_contexts.append(ctx)

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        session=memory,
        before_model_callback=before_model,
    )

    agent.run("hello", session_id="s1")

    turns = seen_contexts[0].conversation.turns
    assert (turns[0].role, turns[0].content) == (Role.System, "seed")
    assert (turns[1].role, turns[1].content) == (Role.User, "prior")


def test_invalid_session_object_is_rejected() -> None:
    with pytest.raises(AgentError) as exc:
        Agent(
            prompt=Prompt(["hello"], "mock-model", provider="mock"),
            session=object(),  # ty: ignore[invalid-argument-type]
        )
    assert exc.value.code == "WYRD_AGENT_422_INVALID_ARGUMENT"
