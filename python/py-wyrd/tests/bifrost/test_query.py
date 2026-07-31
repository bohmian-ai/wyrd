"""Public Python Bifrost query projection tests."""

import asyncio
import json

import pyarrow
from wyrd.bifrost import (
    BifrostQueryClient,
    BifrostQueryStream,
    IncompleteQueryStreamError,
)


def _ipc_batch() -> bytes:
    batch = pyarrow.record_batch([pyarrow.array([1, 2])], names=["id"])
    sink = pyarrow.BufferOutputStream()
    with pyarrow.ipc.new_stream(sink, batch.schema) as writer:
        writer.write_batch(batch)
    return sink.getvalue().to_pybytes()


class _NativeStream:
    def __init__(self, payloads: list[bytes], terminal: dict[str, object] | None) -> None:
        self._payloads = payloads
        self.terminal_json = json.dumps(terminal) if terminal is not None else None
        self.closed = False

    def next_ipc(self) -> bytes | None:
        return self._payloads.pop(0) if self._payloads else None

    def close(self) -> None:
        self.closed = True


def test_bifrost_query_public_imports_are_available() -> None:
    assert BifrostQueryClient.__module__ == "wyrd.bifrost"
    assert BifrostQueryStream.__module__ == "wyrd.bifrost"


def test_bifrost_query_async_iterator_yields_pyarrow_and_terminal() -> None:
    terminal = {"outcome": "success", "row_count": 2}
    native = _NativeStream([_ipc_batch()], terminal)

    async def consume() -> tuple[list[pyarrow.RecordBatch], dict[str, object] | None]:
        stream = BifrostQueryStream(native)
        batches = [batch async for batch in stream]
        return batches, stream.terminal

    batches, observed = asyncio.run(consume())
    assert len(batches) == 1
    assert isinstance(batches[0], pyarrow.RecordBatch)
    assert batches[0].num_rows == 2
    assert observed == terminal


def test_bifrost_query_missing_terminal_raises_typed_exception() -> None:
    native = _NativeStream([], None)

    async def consume() -> None:
        stream = BifrostQueryStream(native)
        await stream.__anext__()

    try:
        asyncio.run(consume())
    except IncompleteQueryStreamError:
        pass
    else:
        raise AssertionError("missing terminal must raise IncompleteQueryStreamError")


def test_bifrost_query_aclose_drops_native_stream() -> None:
    native = _NativeStream([], {"outcome": "success", "row_count": 0})

    async def close() -> None:
        await BifrostQueryStream(native).aclose()

    asyncio.run(close())
    assert native.closed
