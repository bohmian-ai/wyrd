"""Vala Bifrost write and terminal-safe query clients."""

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator
from typing import Any

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
            await asyncio.shield(asyncio.to_thread(self._native.close))
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
                raise IncompleteQueryStreamError("query completed without terminal metadata")
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
    def terminal(self) -> dict[str, Any] | None:
        """Return terminal metadata only after validated completion."""

        return self._terminal

    async def aclose(self) -> None:
        """Cancel the query by dropping its Rust-owned HTTP response stream."""

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


__all__ = [
    "Bifrost",
    "BifrostQueryClient",
    "BifrostQueryError",
    "BifrostQueryStream",
    "IncompleteQueryStreamError",
]
