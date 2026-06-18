"""Typed Wyrd error aliases for structured exception handling."""

from ._wyrd import WyrdError

CfgInvalidToml = WyrdError
CfgSchemaMismatch = WyrdError

__all__ = [
    "CfgInvalidToml",
    "CfgSchemaMismatch",
    "WyrdError",
]
