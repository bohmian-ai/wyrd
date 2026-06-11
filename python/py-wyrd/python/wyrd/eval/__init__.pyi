# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
"""Source stub for wyrd.eval."""

#### begin imports ####
from collections.abc import Callable
from typing import Any, Literal, TypedDict
#### end of imports ####


class ConversationTurn(TypedDict):
    """One turn of conversation history."""

    role: Literal["user", "agent"]
    content: str


class AgentTurnResponse(TypedDict, total=False):
    """Return shape accepted from agent_fn."""

    response: str
    records: list[dict[str, Any]]


class RunSummary(TypedDict):
    """Summary returned when the server emits RunComplete."""

    run_id: str
    server_url: str


def run_eval(
    server_url: str,
    eval_ref: str,
    agent_fn: Callable[[str, list[ConversationTurn]], AgentTurnResponse | str],
    *,
    simulated_user_fn: Callable[[list[ConversationTurn]], str] | None = None,
    simulated_user: Literal["server", "client"] = "server",
    request_timeout_secs: int = 60,
) -> RunSummary:
    """Drive a Wyrd eval run through the server pull protocol.

    Args:
        server_url (str): Base URL for the Wyrd server.
        eval_ref (str): Eval Card reference as name@version or space/name@version.
        agent_fn (Callable[[str, list[ConversationTurn]], AgentTurnResponse | str]):
            Callback that returns the agent response for each requested turn.
        simulated_user_fn (Callable[[list[ConversationTurn]], str] | None):
            Optional callback for client-delegated user turns.
        simulated_user (Literal["server", "client"]): Source for non-scripted user turns.
        request_timeout_secs (int): Per-request HTTP timeout in seconds.

    Returns:
        RunSummary: Minimal run summary containing run_id and server_url.

    Raises:
        WyrdError: On invalid inputs, transport failures, malformed protocol
            responses, or callback exceptions.
    """
    ...


__all__ = [
    "AgentTurnResponse",
    "ConversationTurn",
    "RunSummary",
    "run_eval",
]
