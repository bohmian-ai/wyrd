"""Wyrd eval thin protocol client."""

from typing import Any, Literal, TypedDict

from .._wyrd.eval import run_eval


class ConversationTurn(TypedDict):
    """One turn of conversation history."""

    role: Literal["User", "Agent"]
    content: str


class AgentTurnResponse(TypedDict, total=False):
    """Return shape accepted from agent_fn."""

    response: str
    records: list[dict[str, Any]]


class RunSummary(TypedDict):
    """Summary returned when the server emits RunComplete."""

    run_id: str
    server_url: str


__all__ = [
    "AgentTurnResponse",
    "ConversationTurn",
    "RunSummary",
    "run_eval",
]
