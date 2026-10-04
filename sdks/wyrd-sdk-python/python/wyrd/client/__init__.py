"""The shared authenticated Wyrd client, including ``on_behalf_of`` delegation."""

from .._wyrd.client import WyrdClient

__all__ = ["WyrdClient"]
