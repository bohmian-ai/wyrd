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

    async def insert_batch(
        self,
        table: str,
        batch_id: uuid.UUID,
        batch: pyarrow.RecordBatch,
    ) -> uuid.UUID:
        """Send one Arrow batch built on a described schema, returning its acked identity.

        The batch travels as written — this is not the buffered JSON row path
        and rebuilds no schema. The returned identity is the server's echo of
        the caller's own batch id, so a retry of the same bytes stays one
        durable batch.
        """

        sink = pyarrow.BufferOutputStream()
        with pyarrow.ipc.new_stream(sink, batch.schema) as writer:
            writer.write_batch(batch)
        acked = await asyncio.to_thread(
            self._native.insert_batch,
            table,
            batch_id.bytes,
            sink.getvalue().to_pybytes(),
        )
        acked = uuid.UUID(bytes=bytes(acked))
        if acked != batch_id:
            raise BifrostQueryError(f"bifrost acked batch {acked} for submitted batch {batch_id}")
        return acked


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
