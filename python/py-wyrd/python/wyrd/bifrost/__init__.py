"""Vala Bifrost write and terminal-safe query clients."""

from __future__ import annotations

import asyncio
import json
import uuid
from collections.abc import AsyncIterator
from datetime import datetime
from typing import Any, TypedDict

import pyarrow

from .. import _wyrd as _native
from .._wyrd.bifrost import (
    Bifrost,
    BifrostQueryError,
    IncompleteQueryStreamError,
)


class BifrostQueryStream(AsyncIterator[pyarrow.RecordBatch]):
    """Asynchronously yields Arrow batches from one terminal-safe Oracle query."""

    def __init__(self, native_stream: Any) -> None:
        self._native = native_stream
        self._terminal: dict[str, Any] | None = None
        self._done = False

    def __aiter__(self) -> BifrostQueryStream:
        return self

    async def __anext__(self) -> pyarrow.RecordBatch:
        if self._done:
            raise StopAsyncIteration
        try:
            payload = await asyncio.to_thread(self._native.next_ipc)
        except asyncio.CancelledError:
            self._done = True
            cleanup = asyncio.create_task(asyncio.to_thread(self._native.close))
            while not cleanup.done():
                try:
                    await asyncio.shield(cleanup)
                except asyncio.CancelledError:
                    continue
                except Exception:
                    break
            if cleanup.done() and not cleanup.cancelled():
                try:
                    cleanup.result()
                except Exception:
                    pass
            raise
        except Exception:
            self._done = True
            try:
                await asyncio.to_thread(self._native.close)
            except Exception:
                pass
            raise
        if payload is None:
            terminal_json = self._native.terminal_json
            if terminal_json is None:
                self._done = True
                self._native.raise_incomplete_error()
                raise AssertionError("native incomplete projection must raise")
            try:
                self._terminal = json.loads(terminal_json)
            except Exception:
                self._done = True
                try:
                    await asyncio.to_thread(self._native.close)
                except Exception:
                    pass
                raise
            self._done = True
            raise StopAsyncIteration
        try:
            return pyarrow.ipc.open_stream(payload).read_next_batch()
        except Exception:
            self._done = True
            try:
                await asyncio.to_thread(self._native.close)
            except Exception:
                pass
            raise

    @property
    def request_id(self) -> str:
        """Return the server lifecycle request ID before stream completion."""

        return self._native.request_id

    @property
    def terminal(self) -> dict[str, Any] | None:
        """Return terminal metadata only after validated completion."""

        return self._terminal

    async def aclose(self) -> None:
        """Abandon this local response stream without requesting server cancellation."""

        self._done = True
        await asyncio.to_thread(self._native.close)


class BifrostQueryClient:
    """Async Python facade over the Rust-owned terminal-safe Oracle client."""

    def __init__(self, server_url: str, token: str) -> None:
        native_type = _native.bifrost._NativeBifrostQueryClient
        self._native = native_type(server_url, token)

    async def query(
        self,
        sql: str,
        *,
        visibility: str = "published_only",
        freshness: str = "strict",
        deadline_ms: int | None = None,
    ) -> BifrostQueryStream:
        """Start one authenticated Oracle query without blocking the event loop."""

        native_stream = await asyncio.to_thread(
            self._native.query,
            sql,
            visibility,
            freshness,
            deadline_ms,
        )
        return BifrostQueryStream(native_stream)

    async def running(self) -> list[RunningQuery]:
        """List active queries visible to the authenticated tenant."""

        return await asyncio.to_thread(self._native.running)

    async def status(self, request_id: str) -> RunningQuery:
        """Return one active query by its canonical request ID."""

        return await asyncio.to_thread(self._native.status, request_id)

    async def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream."""

        return await asyncio.to_thread(self._native.cancel, request_id)

    async def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe one registered table's stored physical schema."""

        return await asyncio.to_thread(self._native.describe_table, namespace, name)

    async def writable_schema(
        self,
        description: TableDescription,
        *,
        include_event_time: bool = False,
    ) -> pyarrow.Schema:
        """Return the exact Arrow schema a writer builds batches on.

        The schema is decoded from schema-only Arrow IPC produced by Rust from
        the same description, so Python never rebuilds the physical contract
        from the description's field declarations.
        """

        payload = await asyncio.to_thread(
            self._native.writable_schema_ipc,
            json.dumps(description),
            include_event_time,
        )
        return pyarrow.ipc.open_stream(payload).schema

    async def get_trace(
        self,
        trace_id: str,
        *,
        since: datetime | None = None,
        until: datetime | None = None,
    ) -> TraceDetail:
        """Read one complete authorized cut of a single trace."""

        return await asyncio.to_thread(
            self._native.get_trace,
            trace_id,
            since.isoformat() if since is not None else None,
            until.isoformat() if until is not None else None,
        )

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

        request: dict[str, object] = {}
        if since is not None:
            request["since"] = since.isoformat()
        if until is not None:
            request["until"] = until.isoformat()
        if limit is not None:
            request["limit"] = limit
        if page_token is not None:
            request["page_token"] = page_token
        if conversation_id is not None:
            request["conversation_id"] = conversation_id
        if model is not None:
            request["model"] = model
        if provider is not None:
            request["provider"] = provider
        return await asyncio.to_thread(self._native.query_genai, json.dumps(request))

    async def insert_batch(self, table: str, batch: pyarrow.RecordBatch) -> uuid.UUID:
        """Send one Arrow batch built on a described schema, returning its durable identity.

        The batch travels as written — this is not the buffered JSON row path
        and rebuilds no schema. The transport mints the batch identity and
        retries that one identity itself, so the caller neither supplies nor
        reconciles it.
        """

        sink = pyarrow.BufferOutputStream()
        with pyarrow.ipc.new_stream(sink, batch.schema) as writer:
            writer.write_batch(batch)
        acked = await asyncio.to_thread(
            self._native.insert_batch,
            table,
            sink.getvalue().to_pybytes(),
        )
        return uuid.UUID(bytes=bytes(acked))


class DataTypeSpecVariants(TypedDict, total=False):
    """The parameterized `DataTypeSpec` forms, tagged by their variant name.

    A scalar type is the bare variant name as a string; everything below carries
    parameters, so it arrives as a single-key object. `List` and `Struct` hold
    full field declarations, which is what makes the type recursive.
    """

    FixedSizeBinary: dict[str, int]
    Timestamp: dict[str, str | None]
    Time32: dict[str, str]
    Time64: dict[str, str]
    Decimal128: dict[str, int]
    List: FieldSpec
    Struct: list[FieldSpec]


DataTypeSpec = str | DataTypeSpecVariants
"""One column's logical type: a scalar variant name or a parameterized form."""


class _FieldSpecOptional(TypedDict, total=False):
    """The described-column keys the server omits when they carry nothing.

    `metadata` holds the stable field id under `PARQUET:field_id` and, for a
    write-time correlation input, `wyrd:input_class`; an empty map is omitted.
    """

    metadata: dict[str, str]


class FieldSpec(_FieldSpecOptional):
    """One described column, carrying its own identity metadata."""

    name: str
    data_type: DataTypeSpec
    nullable: bool


class TableEntry(TypedDict):
    """The lightweight table identity shared by the list and describe routes."""

    namespace: str
    name: str
    table_uid: str
    status: str
    fingerprint: str
    registered_at: str
    updated_at: str


class SortKey(TypedDict):
    """One resolved sort key of a table's physical layout."""

    column: str
    direction: str
    null_order: str


class PhysicalLayout(TypedDict):
    """A table's server-resolved partitioning, sort order, and Bloom columns."""

    partition_granularity: str
    sort_keys: list[SortKey]
    bloom_columns: list[str]


class _TableDescriptionOptional(TypedDict, total=False):
    """The whole-physical-schema identity only a canonical built-in publishes."""

    canonical_physical_fingerprint: str


class TableDescription(_TableDescriptionOptional):
    """Server projection of one registered table's stored physical schema."""

    entry: TableEntry
    user_fields: list[FieldSpec]
    correlation_fields: list[FieldSpec]
    managed_candidates: list[FieldSpec]
    physical_layout: PhysicalLayout


class _SpanEventOptional(TypedDict, total=False):
    """The event payload omitted without `bifrost_trace_payload:read`."""

    attributes: Any


class SpanEvent(_SpanEventOptional):
    """One event nested on its owning span, in producer order."""

    time_unix_nano: int
    name: str
    dropped_attributes_count: int


class _SpanLinkOptional(TypedDict, total=False):
    """The link payload omitted without `bifrost_trace_payload:read`."""

    attributes: Any


class SpanLink(_SpanLinkOptional):
    """One link nested on its owning span, in producer order."""

    linked_trace_id: str
    linked_span_id: str
    trace_state: str
    flags: int
    dropped_attributes_count: int


class _SpanOptional(TypedDict, total=False):
    """The span keys the server omits.

    Each is either genuinely absent on the record or payload-gated: without
    `bifrost_trace_payload:read` the server omits it from the wire rather than
    returning it empty.
    """

    parent_span_id: str
    status_code: int
    status_message: str
    attributes: Any
    events: list[SpanEvent]
    links: list[SpanLink]
    service_name: str
    resource_attributes: Any
    scope_attributes: Any


class Span(_SpanOptional):
    """One complete span, carrying its own events and links."""

    span_id: str
    trace_state: str
    flags: int
    name: str
    kind: int
    start_time_unix_nano: int
    end_time_unix_nano: int
    duration_nano: int
    dropped_attributes_count: int
    dropped_events_count: int
    dropped_links_count: int
    resource_dropped_attributes_count: int
    resource_schema_url: str
    scope_name: str
    scope_version: str
    scope_dropped_attributes_count: int
    scope_schema_url: str


class TraceWaterfall(TypedDict):
    """Every authorized span of one trace, flat, each carrying its own events and links."""

    trace_id: str
    spans: list[Span]


class TraceDetail(TypedDict):
    """One complete authorized cut of a trace, as returned by `get_trace`."""

    trace: TraceWaterfall


class _GenAiRowOptional(TypedDict, total=False):
    """The generation keys the server omits.

    Each promoted scalar is absent when its source attribute was; the two
    message payloads are additionally gated on `bifrost_genai_payload:read`.
    They stay `Any` because a message list is producer-defined JSON, not a
    fixed wire shape.
    """

    conversation_id: str
    model: str
    provider: str
    input_tokens: int
    output_tokens: int
    input_messages: Any
    output_messages: Any


class GenAiRow(_GenAiRowOptional):
    """One GenAI generation read from the canonical span table."""

    start_time_unix_nano: int


class _GenAiPageOptional(TypedDict, total=False):
    """The continuation token, absent on the last page."""

    next_page_token: str


class GenAiPage(_GenAiPageOptional):
    """One page of GenAI generation records."""

    rows: list[GenAiRow]


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


__all__ = [
    "Bifrost",
    "BifrostQueryClient",
    "BifrostQueryError",
    "BifrostQueryStream",
    "CancelRunningQueryResult",
    "DataTypeSpec",
    "DataTypeSpecVariants",
    "FieldSpec",
    "GenAiPage",
    "GenAiRow",
    "IncompleteQueryStreamError",
    "PhysicalLayout",
    "RunningQuery",
    "RunningQueryProgress",
    "SortKey",
    "Span",
    "SpanEvent",
    "SpanLink",
    "TableDescription",
    "TableEntry",
    "TraceDetail",
    "TraceWaterfall",
]
