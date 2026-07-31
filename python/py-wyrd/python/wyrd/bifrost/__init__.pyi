# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
from collections.abc import AsyncIterator
from typing import Any

import pyarrow

class Bifrost:
    """Vala Bifrost client write handle over the pooled producers.

    Keyed under a credential-fingerprint scope derived from ``server_url`` and
    ``api_key``; the scope and pool keys are never exposed to Python. Until the
    networked ingest transport is wired, the handle drains into an in-process
    loopback sink so the enqueue/observe/drop-count boundary is exercised
    without a server.
    """

    def __init__(self, server_url: str, api_key: str) -> None: ...
    def insert(
        self,
        table: str,
        schema: str,
        row: str,
        card_ref: str,
        run_id: str | None = None,
    ) -> None:
        """Enqueue one JSON ``row``, propagating queue-full to the caller.

        ``schema`` is JSON-Schema text; ``card_ref`` is ``space/Kind/name@version``.
        """
        ...

    @property
    def dropped(self) -> int:
        """Rows dropped by the fire-and-forget observe path under backpressure."""
        ...

    @property
    def producer_count(self) -> int:
        """Number of distinct producers currently pooled."""
        ...

class BifrostQueryError(RuntimeError):
    """Base exception for terminal-safe Bifrost query failures."""

class IncompleteQueryStreamError(BifrostQueryError):
    """Raised when transport EOF arrives before a validated terminal."""

class BifrostQueryStream(AsyncIterator[pyarrow.RecordBatch]):
    """Asynchronously yields Arrow batches from one Oracle query."""

    def __aiter__(self) -> BifrostQueryStream: ...
    async def __anext__(self) -> pyarrow.RecordBatch: ...
    @property
    def terminal(self) -> dict[str, Any] | None:
        """Return terminal metadata after validated completion."""
        ...
    async def aclose(self) -> None:
        """Cancel response-body consumption."""
        ...

class BifrostQueryClient:
    """Async Python facade over the Rust-owned Oracle query client."""

    def __init__(self, server_url: str, token: str) -> None: ...
    async def query(
        self,
        sql: str,
        *,
        visibility: str = "fused",
        freshness: str = "allow_degraded",
        deadline_ms: int | None = None,
    ) -> BifrostQueryStream:
        """Start one authenticated terminal-safe query."""
        ...

__all__ = [
    "Bifrost",
    "BifrostQueryClient",
    "BifrostQueryError",
    "BifrostQueryStream",
    "IncompleteQueryStreamError",
]
