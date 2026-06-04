#### begin imports ####
from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import Any

from .header import PathLike
from .prompt import Prompt

#### end of imports ####

class Agent:
    """Declarative and runnable Wyrd Agent."""

    def __init__(
        self,
        *,
        prompt: Prompt | Mapping[str, Any],
        name: str | None = ...,
        version: str | None = ...,
        space: str | None = ...,
        id: str | None = ...,
        tools: Sequence[Any] | None = ...,
        providers: Any | None = ...,
        run_config: Any | None = ...,
        before_agent_callback: Any | None = ...,
        after_agent_callback: Any | None = ...,
        before_model_callback: Any | None = ...,
        after_model_callback: Any | None = ...,
        before_tool_callback: Any | None = ...,
        after_tool_callback: Any | None = ...,
        session: Any | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
    ) -> None: ...
    @property
    def name(self) -> str | None: ...
    @property
    def version(self) -> str | None: ...
    @property
    def space(self) -> str | None: ...
    @property
    def id(self) -> str: ...
    @property
    def prompt(self) -> Prompt: ...
    @property
    def provider(self) -> str: ...
    @property
    def model(self) -> str: ...
    @property
    def tool_names(self) -> list[str]: ...
    def save(self, path: PathLike) -> None: ...
    @staticmethod
    def from_yaml(path: PathLike) -> Agent: ...
    def to_yaml_string(self) -> str: ...
    def to_card(self) -> dict[str, Any]: ...
    def model_dump_json(self) -> str: ...
    @staticmethod
    def model_validate_json(data: str) -> Agent: ...
    def validate_registrable(self) -> None: ...
    def run(self, input: str | Mapping[str, Any], *, session_id: str | None = ...) -> Any: ...
    def as_tool(self, *, description: str | None = ...) -> Any: ...

__all__ = ["Agent"]
