"""The one Bifrost client: query any authorized table, write to the active one."""

from __future__ import annotations

import asyncio
import json
from collections.abc import AsyncIterator, Iterator
from typing import Any, TypedDict, cast

import pyarrow

from .. import _wyrd as _native
from .._wyrd.bifrost import (
    BifrostQueryError,
    IncompleteQueryStreamError,
    NoCredentialsError,
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


def _row_json(row: Any) -> str:
    """Serialize one row the three ways a caller supplies it.

    A Pydantic model instance dumps itself, a JSON string passes through
    untouched, and anything else is dumped as a mapping. The declared columns
    live in the ``TableConfig``, so nothing here inspects the row's shape.
    """

    dump = getattr(row, "model_dump_json", None)
    if callable(dump):
        return str(dump())
    if isinstance(row, str):
        return row
    return json.dumps(row)


def _layout_json(
    partition_granularity: str | None,
    sort_keys: list[SortKey] | None,
    bloom_columns: list[str] | None,
) -> str | None:
    """Assemble the optional physical-layout declaration for the native edge.

    Omitting every argument omits the declaration entirely, which is what makes
    the server resolve its own defaults. Supplying any part requires a
    partition granularity on the wire, so an unstated one becomes ``"hour"`` —
    the same value the server would have chosen.
    """

    if partition_granularity is None and sort_keys is None and bloom_columns is None:
        return None
    return json.dumps(
        {
            "partition_granularity": partition_granularity or "hour",
            "sort_keys": sort_keys or [],
            "bloom_columns": bloom_columns or [],
        }
    )


class ResolvedTable(TypedDict):
    """The server-minted identity of a registered table."""

    table_uid: str
    fingerprint: str


class Correlation(TypedDict, total=False):
    """Optional per-row correlation; an omitted key is a null on the wire."""

    card_ref: str
    run_id: str


class TableConfig:
    """One Bifrost table: its name, the columns a model declares, and the
    physical layout to request.

    The schema comes from a Pydantic model class (not an instance) through
    ``model_json_schema()``, mapped by the same Rust owner every language uses.
    No fingerprint is computed here: the server mints it, so ``resolved`` stays
    ``None`` until ``Bifrost.register()`` or ``TableConfig.describe()``.
    """

    def __init__(
        self,
        model: type[Any],
        table: str,
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
    ) -> None:
        schema = model.model_json_schema()
        self._native = _native.bifrost.TableConfig.from_json_schema(
            table,
            json.dumps(schema),
            _layout_json(partition_granularity, sort_keys, bloom_columns),
        )

    @classmethod
    def _from_native(cls, native: Any) -> TableConfig:
        """Wrap one already-built native config without re-mapping a schema."""

        config = cls.__new__(cls)
        config._native = native
        return config

    @staticmethod
    def from_arrow(
        schema: pyarrow.Schema,
        table: str,
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
    ) -> TableConfig:
        """Build from an explicit ``pyarrow.Schema``.

        The precision door, for the types JSON Schema cannot express:
        ``int32``, a non-UTC timestamp, ``decimal128``.
        """

        return TableConfig._from_native(
            _native.bifrost.TableConfig.from_arrow_ipc(
                table,
                schema.serialize().to_pybytes(),
                _layout_json(partition_granularity, sort_keys, bloom_columns),
            )
        )

    @staticmethod
    def describe(
        table: str,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> TableConfig:
        """Fetch an already-registered table's config by name.

        Every transport argument is optional; an omitted one resolves through
        the same chain the client constructor uses.
        """

        return TableConfig._from_native(
            _native.bifrost.TableConfig.describe(table, server_url, credential, grpc_url)
        )

    @property
    def fqn(self) -> str:
        """The ``namespace.name`` this config addresses."""

        return str(self._native.fqn)

    @property
    def arrow_schema(self) -> pyarrow.Schema:
        """The declared user columns only.

        Correlation and managed columns are appended by the write path and the
        server, so they are absent here by construction.
        """

        return pyarrow.ipc.open_stream(self._native.arrow_schema_ipc).schema

    @property
    def resolved(self) -> ResolvedTable | None:
        """The server-assigned identity, or ``None`` while unregistered."""

        resolved = self._native.resolved
        if resolved is None:
            return None
        return ResolvedTable(table_uid=resolved[0], fingerprint=resolved[1])


class QueryResult:
    """Arrow batches from one query, converted on demand.

    The batches cross the boundary once as a single Arrow IPC stream, so every
    conversion below reads the same bytes rather than rebuilding the result per
    format.
    """

    def __init__(self, native: Any) -> None:
        self._native = native

    def to_arrow(self) -> pyarrow.Table:
        """Read the result as one ``pyarrow.Table``."""

        return pyarrow.ipc.open_stream(self._native.to_ipc()).read_all()

    def to_polars(self) -> Any:
        """Read the result as a Polars DataFrame, zero-copy from Arrow."""

        import polars

        return polars.from_arrow(self.to_arrow())

    def to_pandas(self) -> Any:
        """Read the result as a pandas DataFrame."""

        return self.to_arrow().to_pandas()

    def to_bytes(self) -> bytes:
        """The raw Arrow IPC stream, for a caller with its own reader."""

        return bytes(self._native.to_ipc())

    @property
    def terminal(self) -> dict[str, Any]:
        """The validated terminal frame the server closed the stream with."""

        return json.loads(self._native.terminal_json)

    def __len__(self) -> int:
        """Total rows across every batch."""

        return len(self._native)


class BifrostBatchIterator(Iterator[pyarrow.RecordBatch]):
    """Synchronously yields Arrow batches from one terminal-safe query.

    Completion requires the terminal frame: a stream that ends without one
    raises rather than looking like an empty result.
    """

    def __init__(self, native_stream: Any) -> None:
        self._native = native_stream
        self._terminal: dict[str, Any] | None = None
        self._done = False

    def __iter__(self) -> BifrostBatchIterator:
        return self

    def __next__(self) -> pyarrow.RecordBatch:
        if self._done:
            raise StopIteration
        try:
            payload = self._native.next_ipc()
        except Exception:
            self._done = True
            self._native.close()
            raise
        if payload is None:
            self._done = True
            terminal_json = self._native.terminal_json
            if terminal_json is None:
                self._native.raise_incomplete_error()
                raise AssertionError("native incomplete projection must raise")
            self._terminal = json.loads(terminal_json)
            raise StopIteration
        try:
            return pyarrow.ipc.open_stream(payload).read_next_batch()
        except Exception:
            self._done = True
            self._native.close()
            raise

    @property
    def request_id(self) -> str:
        """The server lifecycle request ID, available before completion."""

        return str(self._native.request_id)

    @property
    def terminal(self) -> dict[str, Any] | None:
        """Terminal metadata, present only after validated completion."""

        return self._terminal

    def close(self) -> None:
        """Abandon this local response stream without server cancellation."""

        self._done = True
        self._native.close()


class _BifrostBase:
    """Everything the sync and async clients share.

    Construction and the two non-IO operations live here because they are
    identical on both facades: ``insert`` is a bounded channel send and
    ``use_table`` is a lock swap, so neither has an ``await`` form to differ in.
    """

    def __init__(
        self,
        table: TableConfig | None = None,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> None:
        """Connect one client, optionally already bound to a write target.

        Args:
            table: the active write target. Omitted, the client is query-only
                until ``use_table``.
            server_url: the Bifrost server URL. Resolved from
                ``WYRD_SERVER_URL`` and then the compiled default if omitted.
            credential: the API key or bearer token. Resolved through
                ``WYRD_ACCESS_TOKEN`` → ``WYRD_WORKLOAD_TOKEN`` + tenant →
                ``WYRD_API_KEY`` → ``~/.config/wyrd/credentials.toml``
                ``[default].api_key`` if omitted.
            grpc_url: the ingest endpoint. Resolved from ``WYRD_GRPC_URL`` if
                omitted.

        Raises:
            NoCredentialsError: the credential chain yielded nothing.

        """

        self._native = _native.bifrost.Bifrost(
            table._native if table is not None else None,
            server_url,
            credential,
            grpc_url,
        )

    def use_table(self, table: TableConfig) -> TableConfig | None:
        """Bind ``table`` as the write target, returning the previous binding.

        The previous table's producer stays pooled, so its buffered rows still
        flush; a swap loses nothing.
        """

        previous = self._native.use_table(table._native)
        return None if previous is None else TableConfig._from_native(previous)

    @property
    def table(self) -> TableConfig | None:
        """The active write binding, if any."""

        native = self._native.table
        return None if native is None else TableConfig._from_native(native)

    def insert(self, row: Any, correlation: Correlation | None = None) -> None:
        """Enqueue one row into the active table.

        Accepts a Pydantic model instance, a mapping, or a JSON string.
        Non-blocking; the row is durable after ``flush()``. A saturated queue
        refuses here rather than dropping silently.

        Raises:
            BifrostQueryError: no table is bound, or the producer queue is full.

        """

        correlation = correlation or {}
        self._native.insert(
            _row_json(row),
            correlation.get("card_ref"),
            correlation.get("run_id"),
        )

    @property
    def dropped(self) -> int:
        """Rows dropped by the fire-and-forget ``wyrd.observe.record`` path."""

        return int(self._native.dropped)

    @property
    def producer_count(self) -> int:
        """Number of distinct table producers currently pooled."""

        return int(self._native.producer_count)


class Bifrost(_BifrostBase):
    """Query any authorized table; write to the active one.

    Synchronous. Use ``AsyncBifrost`` for the ``await`` surface — the same
    methods on the same native client, differing only in who drives them.
    """

    def register(self) -> str:
        """Create the active table; returns ``"created"`` or ``"already_exists"``."""

        return str(self._native.register())

    def use_table_by_name(self, table: str) -> None:
        """Bind an already-registered table by name, describing it first."""

        self._native.use_table_by_name(table)

    def flush(self) -> None:
        """Flush every pooled producer and await each durable acknowledgement."""

        self._native.flush()

    def shutdown(self) -> None:
        """Drain every producer and stop its background task."""

        self._native.shutdown()

    def sql(self, query: str) -> QueryResult:
        """Run one SQL SELECT over any authorized table and collect every batch."""

        return QueryResult(self._native.sql(query))

    def stream(
        self,
        query: str,
        *,
        visibility: str = "published_only",
        freshness: str = "strict",
        deadline_ms: int | None = None,
    ) -> BifrostBatchIterator:
        """Run one SQL SELECT and iterate its batches as they arrive."""

        return BifrostBatchIterator(self._native.stream(query, visibility, freshness, deadline_ms))

    def running(self) -> list[RunningQuery]:
        """List active queries visible to the authenticated tenant."""

        return list(self._native.running())

    def status(self, request_id: str) -> RunningQuery:
        """Return one active query by its canonical request ID."""

        return cast("RunningQuery", self._native.status(request_id))

    def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream."""

        return cast("CancelRunningQueryResult", self._native.cancel(request_id))

    def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe one registered table's stored physical schema."""

        return cast("TableDescription", self._native.describe_table(namespace, name))


class AsyncBifrost(_BifrostBase):
    """The ``await`` surface over the same native client.

    Every blocking native call runs in a worker thread, so the event loop is
    never blocked. ``insert`` and ``use_table`` stay synchronous because they
    perform no IO.
    """

    async def register(self) -> str:
        """Create the active table; returns ``"created"`` or ``"already_exists"``."""

        return str(await asyncio.to_thread(self._native.register))

    async def use_table_by_name(self, table: str) -> None:
        """Bind an already-registered table by name, describing it first."""

        await asyncio.to_thread(self._native.use_table_by_name, table)

    async def flush(self) -> None:
        """Flush every pooled producer and await each durable acknowledgement."""

        await asyncio.to_thread(self._native.flush)

    async def shutdown(self) -> None:
        """Drain every producer and stop its background task."""

        await asyncio.to_thread(self._native.shutdown)

    async def sql(self, query: str) -> QueryResult:
        """Run one SQL SELECT over any authorized table and collect every batch."""

        return QueryResult(await asyncio.to_thread(self._native.sql, query))

    async def stream(
        self,
        query: str,
        *,
        visibility: str = "published_only",
        freshness: str = "strict",
        deadline_ms: int | None = None,
    ) -> BifrostQueryStream:
        """Start one query and iterate its batches with ``async for``."""

        native_stream = await asyncio.to_thread(
            self._native.stream,
            query,
            visibility,
            freshness,
            deadline_ms,
        )
        return BifrostQueryStream(native_stream)

    async def running(self) -> list[RunningQuery]:
        """List active queries visible to the authenticated tenant."""

        return list(await asyncio.to_thread(self._native.running))

    async def status(self, request_id: str) -> RunningQuery:
        """Return one active query by its canonical request ID."""

        return cast("RunningQuery", await asyncio.to_thread(self._native.status, request_id))

    async def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream."""

        return cast(
            "CancelRunningQueryResult",
            await asyncio.to_thread(self._native.cancel, request_id),
        )

    async def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe one registered table's stored physical schema."""

        return cast(
            "TableDescription",
            await asyncio.to_thread(self._native.describe_table, namespace, name),
        )


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
    "AsyncBifrost",
    "Bifrost",
    "BifrostBatchIterator",
    "BifrostQueryError",
    "BifrostQueryStream",
    "CancelRunningQueryResult",
    "Correlation",
    "DataTypeSpec",
    "DataTypeSpecVariants",
    "FieldSpec",
    "IncompleteQueryStreamError",
    "NoCredentialsError",
    "PhysicalLayout",
    "QueryResult",
    "ResolvedTable",
    "RunningQuery",
    "RunningQueryProgress",
    "SortKey",
    "TableConfig",
    "TableDescription",
    "TableEntry",
]
