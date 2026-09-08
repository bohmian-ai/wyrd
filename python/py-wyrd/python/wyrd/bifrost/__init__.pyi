# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
import uuid
from collections.abc import AsyncIterator
from datetime import datetime
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

class TableDescription(TypedDict):
    """Server projection of one registered table's stored physical schema."""

    entry: dict[str, Any]
    user_fields: list[dict[str, Any]]
    correlation_fields: list[dict[str, Any]]
    managed_candidates: list[dict[str, Any]]
    physical_layout: dict[str, Any]

class TraceDetail(TypedDict):
    """One complete authorized cut of a trace; children nest on their span."""

    trace: dict[str, Any]

class GenAiPage(TypedDict):
    """One page of GenAI generation records."""

    rows: list[dict[str, Any]]

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
    async def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe one registered table's stored physical schema."""
        ...
    async def writable_schema(
        self,
        description: TableDescription,
        *,
        include_event_time: bool = False,
    ) -> pyarrow.Schema:
        """Return the exact Arrow schema a writer builds batches on."""
        ...
    async def get_trace(
        self,
        trace_id: str,
        *,
        since: datetime | None = None,
        until: datetime | None = None,
    ) -> TraceDetail:
        """Read one complete authorized cut of a single trace."""
        ...
    async def query_genai(
        self,
        *,
        since: datetime | None = None,
        until: datetime | None = None,
        limit: int | None = None,
        page_token: str | None = None,
        conversation_id: str | None = None,
        model: str | None = None,
        provider: str | None = None,
    ) -> GenAiPage:
        """Read one page of GenAI generation records."""
        ...
    async def insert_batch(
        self,
        table: str,
        batch_id: uuid.UUID,
        batch: pyarrow.RecordBatch,
    ) -> uuid.UUID:
        """Send one Arrow batch built on a described schema."""
        ...

__all__ = [
    "Bifrost",
    "BifrostQueryClient",
    "BifrostQueryError",
    "BifrostQueryStream",
    "CancelRunningQueryResult",
    "GenAiPage",
    "IncompleteQueryStreamError",
    "RunningQuery",
    "RunningQueryProgress",
    "TableDescription",
    "TraceDetail",
]
