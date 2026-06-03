"""Agent run value objects."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


class FinishReason:
    """Agent finish reason constants."""

    ModelStopped = "model_stopped"
    MaxIterations = "max_iterations"
    CallbackSkipped = "callback_skipped"
    ProviderError = "provider_error"
    ToolError = "tool_error"
    Timeout = "timeout"


@dataclass
class RunConfig:
    max_iterations: int = 10
    tool_concurrency_cap: int | None = 8
    session_recent_limit: int | None = None
    timeout_ms: int | None = None


@dataclass
class AgentRun:
    finish_reason: str
    output: str
    iterations: int
    tokens_in: int = 0
    tokens_out: int = 0
    conversation: Any = field(default_factory=dict)


__all__ = ["AgentRun", "FinishReason", "RunConfig"]
