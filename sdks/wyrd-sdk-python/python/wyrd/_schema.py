"""Shared Python type-to-JSON-Schema helpers."""

from __future__ import annotations

import inspect
import types
import typing
from typing import Any

_PRIMITIVES: dict[Any, dict] = {
    str: {"type": "string"},
    int: {"type": "integer"},
    float: {"type": "number"},
    bool: {"type": "boolean"},
    bytes: {"type": "string", "contentEncoding": "base64"},
}


def annotation_to_schema(annotation: Any) -> dict:
    """Convert a Python type annotation to a JSON Schema dict.

    ``Optional[T]`` maps to ``T``'s schema; other unions become ``anyOf``.
    Types without a primitive or container mapping go through pydantic's
    ``TypeAdapter``; an unknown or unsupported annotation yields ``{}``.
    """
    if annotation is None or annotation is type(None):
        return {"type": "null"}

    if annotation is Any or annotation is inspect.Parameter.empty:
        return {}

    if annotation in _PRIMITIVES:
        return _PRIMITIVES[annotation]

    if isinstance(annotation, types.UnionType):
        args = annotation.__args__
        non_none = [arg for arg in args if arg is not type(None)]
        if len(non_none) == 1 and len(args) == 2:
            return annotation_to_schema(non_none[0])
        return {"anyOf": [annotation_to_schema(arg) for arg in args]}

    origin = typing.get_origin(annotation)
    args = typing.get_args(annotation)

    if origin is typing.Union:
        non_none = [arg for arg in args if arg is not type(None)]
        if len(non_none) == 1:
            return annotation_to_schema(non_none[0])
        return {"anyOf": [annotation_to_schema(arg) for arg in args]}

    if origin is typing.Literal:
        return {"enum": list(args)}

    if origin is list:
        return (
            {"type": "array", "items": annotation_to_schema(args[0])} if args else {"type": "array"}
        )

    if origin is dict:
        schema: dict = {"type": "object"}
        if len(args) == 2:
            schema["additionalProperties"] = annotation_to_schema(args[1])
        return schema

    if origin is tuple:
        return (
            {"type": "array", "prefixItems": [annotation_to_schema(arg) for arg in args]}
            if args
            else {"type": "array"}
        )

    try:
        from pydantic import TypeAdapter

        return TypeAdapter(annotation).json_schema()
    except Exception:
        return {}


__all__ = ["annotation_to_schema"]
