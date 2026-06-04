"""Shared Python type-to-JSON-Schema helpers."""

from __future__ import annotations

import inspect
import types
import typing
from typing import Any

from ._wyrd import WyrdError

_PRIMITIVES: dict[Any, dict] = {
    str: {"type": "string"},
    int: {"type": "integer"},
    float: {"type": "number"},
    bool: {"type": "boolean"},
    bytes: {"type": "string", "contentEncoding": "base64"},
}


def _wyrd_error(code: str, detail: str) -> WyrdError:
    error = WyrdError(detail)
    error.code = code
    error.message = detail
    error.details = {}
    return error


def annotation_to_schema(annotation: Any) -> dict:
    """Convert a Python type annotation to a JSON Schema dict."""
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
            {"type": "array", "items": annotation_to_schema(args[0])}
            if args
            else {"type": "array"}
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


def output_to_json_schema(
    output: Any, *, default_name: str = "structured_output"
) -> tuple[str, dict]:
    """Coerce a Prompt output declaration to a JSON Schema object."""
    if isinstance(output, dict):
        if "type" in output and isinstance(output["type"], str):
            return default_name, output
        properties: dict[str, dict] = {}
        required: list[str] = []
        for key, annotation in output.items():
            if not isinstance(key, str):
                raise _wyrd_error(
                    "WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA",
                    f"output schema keys must be strings, got {type(key)!r}",
                )
            properties[key] = annotation_to_schema(annotation)
            required.append(key)
        return default_name, {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": False,
        }

    if isinstance(output, type):
        try:
            import pydantic
        except ImportError as exc:
            raise _wyrd_error(
                "WYRD_PROMPT_422_PYDANTIC_REQUIRED",
                "passing a class to Prompt(output=...) requires pydantic; "
                "install pydantic or use dict[str, type] instead.",
            ) from exc
        if not issubclass(output, pydantic.BaseModel):
            raise _wyrd_error(
                "WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA",
                f"output class must subclass pydantic.BaseModel, got {output!r}",
            )
        return output.__name__, output.model_json_schema()

    raise _wyrd_error(
        "WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA",
        "output must be a dict[str, type], raw JSON schema dict, "
        f"ResponseFormat, or pydantic BaseModel subclass, got {type(output)!r}",
    )


__all__ = ["annotation_to_schema", "output_to_json_schema"]
