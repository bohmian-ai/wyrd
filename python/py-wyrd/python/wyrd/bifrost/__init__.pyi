# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
from collections.abc import AsyncIterator
from typing import Any, TypedDict

import pyarrow

class Bifrost:
    """Vala Bifrost client write handle over the pooled native producers.

    The handle connects to the configured gRPC ingest endpoint and keeps
    credential scope, batching, backpressure, and producer pooling in Rust.
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

    def flush(self) -> None:
        """Flush queued rows and await native ingest acknowledgements."""
        ...

    def shutdown(self) -> None:
        """Drain queued rows and stop native producer tasks."""
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

    code: str
    status: int
    title: str
    message: str
    detail: str
    remediation: str
    details: Any | None

class IncompleteQueryStreamError(BifrostQueryError):
    """Raised when transport EOF arrives before a validated terminal."""

class RunningQueryProgress(TypedDict):
    """Participant progress for one live Oracle query."""

    completed_participants: int
    total_participants: int

class RunningQuery(TypedDict):
    """Canonical live-query summary projected by Bifrost."""

    request_id: str
    query_class: str
    started_at: str
    deadline: str
    state: str
    progress: RunningQueryProgress
    cancellation_requested: bool

class CancelRunningQueryResult(TypedDict):
    """Idempotent server-side cancellation acknowledgement."""

    request_id: str
    cancellation_started: bool

class BifrostQueryStream(AsyncIterator[pyarrow.RecordBatch]):
    """Asynchronously yields Arrow batches from one Oracle query."""

    def __aiter__(self) -> BifrostQueryStream: ...
    async def __anext__(self) -> pyarrow.RecordBatch: ...
    @property
    def request_id(self) -> str:
        """Return the server lifecycle request ID before completion."""
        ...
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
        visibility: str = "published_only",
        freshness: str = "strict",
        deadline_ms: int | None = None,
    ) -> BifrostQueryStream:
        """Start one authenticated terminal-safe query."""
        ...
    async def running(self) -> list[RunningQuery]:
        """List active queries visible to the authenticated tenant."""
        ...
    async def status(self, request_id: str) -> RunningQuery:
        """Return one active query by its canonical request ID."""
        ...
    async def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream."""
        ...

__all__ = [
    "Bifrost",
    "BifrostQueryClient",
    "BifrostQueryError",
    "BifrostQueryStream",
    "CancelRunningQueryResult",
    "IncompleteQueryStreamError",
    "RunningQuery",
    "RunningQueryProgress",
]
