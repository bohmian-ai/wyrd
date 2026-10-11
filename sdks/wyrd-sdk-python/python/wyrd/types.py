"""The three Wyrd timestamp types.

A Bifrost column holds one of three timestamp types, named as Snowflake names
them:

- ``TimestampNTZ`` (``TIMESTAMP_NTZ``): a wall-clock reading with no instant.
- ``TimestampLTZ`` (``TIMESTAMP_LTZ``): one instant.
- ``TimestampTZ`` (``TIMESTAMP_TZ``): one instant plus the writer's wall-clock
  reading of it, so a reader can group by the writer's local hour.

Each is a ``datetime`` that checks its own zone rule, so it works anywhere a
``datetime`` does. A field of one of these types also works in a Pydantic
model: Pydantic asks the type how to validate it and what JSON Schema it has,
and this module imports Pydantic's core only when Pydantic asks. Values of
other time types are converted to these when Bifrost writes and reads them.
"""

from __future__ import annotations

from datetime import datetime
from typing import TYPE_CHECKING, Any, ClassVar, cast

from ._wyrd import bifrost as _native_bifrost

if TYPE_CHECKING:
    from typing_extensions import Self


class _Timestamp(datetime):
    """A ``datetime`` held to one Wyrd timestamp type's zone rule."""

    _NAME: ClassVar[str]
    _ZONED: ClassVar[bool]

    def __new__(cls, *args: Any, **kwargs: Any) -> Self:
        """Build the value and refuse it when its zone breaks the type's rule.

        Raises:
            ValueError: a ``TIMESTAMP_NTZ`` value with a time zone, or a
                ``TIMESTAMP_LTZ`` or ``TIMESTAMP_TZ`` value without one.
        """

        value = super().__new__(cls, *args, **kwargs)
        if (value.utcoffset() is not None) != cls._ZONED:
            rule = "requires" if cls._ZONED else "refuses"
            raise ValueError(f"{cls._NAME} {rule} a time zone: {value.isoformat()}")
        return value

    @classmethod
    def of(cls, value: datetime) -> Self:
        """Convert any ``datetime`` to this type, keeping its reading and zone.

        Raises:
            ValueError: the value's zone breaks this type's rule.
        """

        return cls(
            value.year,
            value.month,
            value.day,
            value.hour,
            value.minute,
            value.second,
            value.microsecond,
            value.tzinfo,
            fold=value.fold,
        )

    @classmethod
    def __get_pydantic_core_schema__(cls, source: Any, handler: Any) -> Any:
        """Validate as a ``datetime``, then convert it to this type."""

        from pydantic_core import core_schema

        return core_schema.no_info_after_validator_function(cls.of, handler(datetime))

    @classmethod
    def __get_pydantic_json_schema__(cls, schema: Any, handler: Any) -> dict[str, str]:
        """Declare this type's JSON Schema format, which names its column type."""

        return {"type": "string", "format": cls.json_format()}

    @classmethod
    def json_format(cls) -> str:
        """The JSON Schema ``format`` that names this type's column type.

        The format table is owned by Rust; this reads it from the native module.
        """

        return cast(str, _native_bifrost.timestamp_format(cls._NAME))


class TimestampNTZ(_Timestamp):
    """``TIMESTAMP_NTZ``: a wall-clock reading with no instant."""

    _NAME = "TIMESTAMP_NTZ"
    _ZONED = False


class TimestampLTZ(_Timestamp):
    """``TIMESTAMP_LTZ``: one instant. Reads back in UTC."""

    _NAME = "TIMESTAMP_LTZ"
    _ZONED = True


class TimestampTZ(_Timestamp):
    """``TIMESTAMP_TZ``: one instant in the writer's offset."""

    _NAME = "TIMESTAMP_TZ"
    _ZONED = True


TIMESTAMP_TYPES: dict[str, type[TimestampNTZ] | type[TimestampLTZ] | type[TimestampTZ]] = {
    kind._NAME: kind for kind in (TimestampNTZ, TimestampLTZ, TimestampTZ)
}
"""Each Wyrd timestamp type by its column type name."""


__all__ = ["TIMESTAMP_TYPES", "TimestampLTZ", "TimestampNTZ", "TimestampTZ"]
