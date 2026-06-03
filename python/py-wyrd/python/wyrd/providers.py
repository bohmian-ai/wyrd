"""Provider registry helpers for Python agent tests."""

from __future__ import annotations

from ._wyrd.providers import _build_mock_registry, _ProviderRegistryInner


class ProviderRegistry:
    """Python handle around the native provider registry."""

    def __init__(self, inner: _ProviderRegistryInner | None = None) -> None:
        self._inner = inner or _ProviderRegistryInner()

    @property
    def names(self) -> list[str]:
        return list(self._inner.names())


def mock_registry(text: str = "mock response") -> ProviderRegistry:
    """Return a fresh registry with a deterministic provider named ``mock``."""
    return ProviderRegistry(_build_mock_registry(text))


__all__ = ["ProviderRegistry", "mock_registry"]
