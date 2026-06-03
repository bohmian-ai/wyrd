"""Public Agent wrapper."""

from __future__ import annotations

from collections.abc import Iterable
from os import PathLike
from typing import Any, Callable, TYPE_CHECKING

from ._wyrd.cards.agent import _AgentBuilderInner, _AgentWithMetaInner
from .prompt import Prompt
from .tool import _ToolCallable

if TYPE_CHECKING:
    from .providers import ProviderRegistry


_BANNED_KWARGS = frozenset({"provider", "model", "observer"})


class Agent:
    """Declarative and runnable Wyrd Agent."""

    def __init__(
        self,
        *,
        prompt: Prompt,
        name: str | None = None,
        version: str | None = None,
        space: str | None = None,
        id: str | None = None,
        tools: Iterable[_ToolCallable] | None = None,
        providers: "ProviderRegistry | None" = None,
        run_config: Any | None = None,
        before_agent_callback: Any | None = None,
        after_agent_callback: Any | None = None,
        before_model_callback: Any | None = None,
        after_model_callback: Any | None = None,
        before_tool_callback: Any | None = None,
        after_tool_callback: Any | None = None,
        session: Any | None = None,
        **kwargs: Any,
    ) -> None:
        bad = set(kwargs) & _BANNED_KWARGS
        if bad:
            raise TypeError(f"Agent.__init__() got banned keyword argument(s): {sorted(bad)}")
        if kwargs:
            raise TypeError(f"Agent.__init__() got unexpected keyword argument(s): {sorted(kwargs)}")

        builder = _AgentBuilderInner()
        builder.prompt(prompt)
        if name is not None:
            builder.name(name)
        if version is not None:
            builder.version(version)
        if space is not None:
            builder.space(space)
        if id is not None:
            builder.id(id)
        if providers is not None:
            builder.providers(providers._inner if hasattr(providers, "_inner") else providers)
        if run_config is not None:
            builder.run_config(run_config)
        if session is not None:
            builder.session(session)
        for tool in tools or []:
            builder.tool(tool)
        self._add_callbacks(
            builder,
            before_agent_callback,
            after_agent_callback,
            before_model_callback,
            after_model_callback,
            before_tool_callback,
            after_tool_callback,
        )
        self._inner: _AgentWithMetaInner = builder.build()

    @property
    def name(self) -> str | None:
        return self._inner.name

    @property
    def version(self) -> str | None:
        return self._inner.version

    @property
    def space(self) -> str | None:
        return self._inner.space

    @property
    def id(self) -> str:
        return self._inner.id

    @property
    def prompt(self) -> Prompt:
        return self._inner.prompt

    @property
    def provider(self) -> str:
        return self.prompt.provider

    @property
    def model(self) -> str:
        return self.prompt.model

    @property
    def tool_names(self) -> list[str]:
        return list(self._inner.tool_names)

    def save(self, path: PathLike[str] | str) -> None:
        self._inner.save(str(path))

    @staticmethod
    def from_yaml(path: PathLike[str] | str) -> "Agent":
        wrapper = Agent.__new__(Agent)
        wrapper._inner = _AgentWithMetaInner.from_yaml(str(path))
        return wrapper

    def to_yaml_string(self) -> str:
        return self._inner.to_yaml_string()

    def model_dump_json(self) -> str:
        return self._inner.model_dump_json()

    @staticmethod
    def model_validate_json(data: str) -> "Agent":
        wrapper = Agent.__new__(Agent)
        wrapper._inner = _AgentWithMetaInner.model_validate_json(data)
        return wrapper

    def validate_registrable(self) -> None:
        self._inner.validate_registrable()

    def run(self, input: str | dict, *, session_id: str | None = None):
        return self._inner.run(input, session_id=session_id)

    def add_tool(self, tool: _ToolCallable) -> "Agent":
        self._inner.add_tool(tool)
        return self

    def set_tools(self, tools: Iterable[_ToolCallable]) -> "Agent":
        self._inner.set_tools(list(tools))
        return self

    def with_prompt(self, prompt: Prompt) -> "Agent":
        self._inner.with_prompt(prompt)
        return self

    def with_session(self, session: Any) -> "Agent":
        self._inner.with_session(session)
        return self

    def with_run_config(self, run_config: Any) -> "Agent":
        self._inner.with_run_config(run_config)
        return self

    def add_before_agent_callback(self, cb: Callable) -> "Agent":
        self._inner.add_before_agent(cb)
        return self

    def add_after_agent_callback(self, cb: Callable) -> "Agent":
        self._inner.add_after_agent(cb)
        return self

    def add_before_model_callback(self, cb: Callable) -> "Agent":
        self._inner.add_before_model(cb)
        return self

    def add_after_model_callback(self, cb: Callable) -> "Agent":
        self._inner.add_after_model(cb)
        return self

    def add_before_tool_callback(self, cb: Callable) -> "Agent":
        self._inner.add_before_tool(cb)
        return self

    def add_after_tool_callback(self, cb: Callable) -> "Agent":
        self._inner.add_after_tool(cb)
        return self

    def as_tool(self, *, description: str | None = None) -> _ToolCallable:
        return self._inner.as_tool(description)

    def __repr__(self) -> str:
        return repr(self._inner)

    @staticmethod
    def _add_callbacks(builder: Any, *callbacks: Any) -> None:
        setters = (
            builder.before_agent,
            builder.after_agent,
            builder.before_model,
            builder.after_model,
            builder.before_tool,
            builder.after_tool,
        )
        for callback_value, setter in zip(callbacks, setters, strict=True):
            if callback_value is None:
                continue
            if callable(callback_value):
                setter(callback_value)
            else:
                for fn in callback_value:
                    setter(fn)


__all__ = ["Agent"]
