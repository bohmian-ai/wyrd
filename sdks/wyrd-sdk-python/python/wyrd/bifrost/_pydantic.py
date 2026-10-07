"""Convert a Pydantic model's time fields to Wyrd timestamp types.

Imported only when Pydantic is already loaded, so the SDK never requires it.
Pydantic gives every ``datetime`` the ``date-time`` format, which is
``TIMESTAMP_LTZ``; a datetime constrained to be naive is a wall-clock reading,
so it is declared with ``TimestampNTZ``'s format instead.
"""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel
from pydantic.json_schema import GenerateJsonSchema, JsonSchemaValue
from pydantic_core import core_schema

from ..types import TimestampNTZ


class _WyrdJsonSchema(GenerateJsonSchema):
    """Pydantic's JSON Schema generator, with naive datetimes as ``TimestampNTZ``."""

    def datetime_schema(self, schema: core_schema.DatetimeSchema) -> JsonSchemaValue:
        """Declare a naive-constrained datetime with ``TimestampNTZ``'s format."""

        json_schema = super().datetime_schema(schema)
        if schema.get("tz_constraint") == "naive":
            json_schema["format"] = TimestampNTZ._FORMAT
        return json_schema


def json_schema(model: Any) -> dict[str, Any]:
    """The JSON Schema of ``model`` with its time fields as Wyrd timestamp types."""

    if isinstance(model, type) and issubclass(model, BaseModel):
        return model.model_json_schema(schema_generator=_WyrdJsonSchema)
    return dict(model.model_json_schema())
