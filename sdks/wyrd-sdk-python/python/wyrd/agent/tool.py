"""Python tool decorator for runtime-local tools."""

from __future__ import annotations

import functools
import inspect
import typing
from collections.abc import Callable
from contextlib import contextmanager
from typing import Any

from .._schema import annotation_to_schema as _annotation_to_schema

# The native ``_wyrd.tool`` submodule is private and has no stub.
from .._wyrd.tool import (  # ty: ignore[unresolved-import]
    _pop_tool_registry_scope,
    _push_tool_registry_scope,
    _register_tool,
)


class _ToolCallable:
    """Callable returned by ``tool()`` and ``Agent.as_tool()``.

    Calling it calls ``fn``. ``name``, ``description``, ``input_schema``, and
    ``output_schema`` are what the model sees when the tool is attached.
    """

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
    """Decorate a function as a runtime-local tool and register it.

    The input schema comes from the parameters' annotations (parameters without
    defaults are required) and the output schema from the return annotation.
    The model's arguments are passed as keyword arguments, and the return value
    must be JSON-compatible. The returned callable still calls ``fn`` directly.
    Registration goes to the innermost ``local_registry()`` on this thread, or
    the process-wide registry outside one.

    Args:
        fn: the function to decorate. Omitted, returns a decorator, so both
            ``@tool`` and ``@tool(name=...)`` work.
        name: the tool name the model sees. Omitted, ``fn.__name__``.
        description: the description the model sees. Omitted, ``fn``'s
            docstring, or ``""`` without one.

    Raises:
        WyrdError: ``WYRD_TOOL_409_NAME_TAKEN`` when the active registry
            already holds a tool with that name.
    """

    def wrap(f: Callable) -> _ToolCallable:
        tool_name = name or getattr(f, "__name__", None)
        if not tool_name:
            raise TypeError(f"tool() needs name= for a callable without __name__: {f!r}")
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
    """Scope tool registration to a temporary, thread-local registry.

    Tools decorated inside the ``with`` block register into a fresh registry
    that is discarded on exit, so names may repeat across blocks (useful in
    tests). Nested blocks stack. ``Agent.from_yaml()`` and
    ``Agent.model_validate_json()`` still resolve tool names against the
    process-wide registry.
    """
    _push_tool_registry_scope()
    try:
        yield
    finally:
        _pop_tool_registry_scope()


def _schema_from_signature(fn: Callable) -> dict:
    sig = inspect.signature(fn)
    hints = typing.get_type_hints(fn) if fn.__annotations__ else {}
    properties: dict[str, Any] = {}
    required: list[str] = []
    for param_name, parameter in sig.parameters.items():
        if param_name in {"self", "cls"}:
            continue
        schema = _annotation_to_schema(hints.get(param_name, Any))
        if parameter.default is inspect.Parameter.empty:
            required.append(param_name)
        else:
            schema = {**schema, "default": parameter.default}
        properties[param_name] = schema
    out: dict[str, Any] = {"type": "object", "properties": properties}
    if required:
        out["required"] = required
    return out


def _schema_from_return(fn: Callable) -> dict:
    hints = typing.get_type_hints(fn) if fn.__annotations__ else {}
    return _annotation_to_schema(hints.get("return", Any))


__all__ = ["_ToolCallable", "local_registry", "tool"]
