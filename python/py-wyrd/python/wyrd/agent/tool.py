"""Python tool decorator for runtime-local tools."""

from __future__ import annotations

import functools
import inspect
from collections.abc import Callable
from contextlib import contextmanager
from typing import Any

from .._wyrd.tool import (
    _pop_tool_registry_scope,
    _push_tool_registry_scope,
    _register_tool,
)


class _ToolCallable:
    """User-visible decorated callable."""

    def __init__(
        self,
        fn: Callable,
        name: str,
        description: str,
        input_schema: dict,
        output_schema: dict,
    ) -> None:
        self.fn = fn
        self.name = name
        self.description = description
        self.input_schema = input_schema
        self.output_schema = output_schema

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        return self.fn(*args, **kwargs)

    def __repr__(self) -> str:
        return f"<wyrd tool {self.name!r}>"


def tool(
    fn: Callable | None = None,
    *,
    name: str | None = None,
    description: str | None = None,
):
    """Decorate and register a runtime-local tool."""

    def wrap(f: Callable) -> _ToolCallable:
        tool_name = name or f.__name__
        tool_description = description or (inspect.getdoc(f) or "").strip()
        input_schema = _schema_from_signature(f)
        output_schema = _schema_from_return(f)
        wrapped = _ToolCallable(f, tool_name, tool_description, input_schema, output_schema)
        _register_tool(tool_name, tool_description, input_schema, output_schema, f)
        functools.update_wrapper(wrapped, f, updated=())
        return wrapped

    return wrap(fn) if fn is not None else wrap


@contextmanager
def local_registry():
    _push_tool_registry_scope()
    try:
        yield
    finally:
        _pop_tool_registry_scope()


def _schema_from_signature(fn: Callable) -> dict:
    from pydantic import TypeAdapter

    signature = inspect.signature(fn)
    hints = getattr(fn, "__annotations__", {}) or {}
    properties: dict[str, Any] = {}
    required: list[str] = []
    for name, parameter in signature.parameters.items():
        if name in {"self", "cls"}:
            continue
        annotation = hints.get(name, Any)
        schema = TypeAdapter(annotation).json_schema()
        if parameter.default is inspect._empty:
            required.append(name)
        else:
            schema["default"] = parameter.default
        properties[name] = schema
    out: dict[str, Any] = {"type": "object", "properties": properties}
    if required:
        out["required"] = required
    return out


def _schema_from_return(fn: Callable) -> dict:
    from pydantic import TypeAdapter

    hints = getattr(fn, "__annotations__", {}) or {}
    return TypeAdapter(hints.get("return", Any)).json_schema()


__all__ = ["_ToolCallable", "local_registry", "tool"]
