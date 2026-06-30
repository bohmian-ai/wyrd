"""Typed Wyrd error aliases for structured exception handling."""

from ._wyrd import WyrdError

# NOTE: The Cfg* names below are aliases of WyrdError, not distinct exception
# subclasses. `except CfgInvalidToml` catches every WyrdError. To distinguish
# config error types, inspect `err.code` after catching `WyrdError`.
CfgInvalidToml = WyrdError
CfgSchemaMismatch = WyrdError
CfgNameDefaultRejected = WyrdError

__all__ = [
    "CfgInvalidToml",
    "CfgNameDefaultRejected",
    "CfgSchemaMismatch",
    "WyrdError",
]
