"""Vala Bifrost write and terminal-safe query clients."""

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator
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
    "IncompleteQueryStreamError",
    "RunningQuery",
    "RunningQueryProgress",
]
