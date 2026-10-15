"""Pure-Python Bifrost client implementation, re-exported by ``wyrd.bifrost``."""

from __future__ import annotations

import asyncio
import json
import sys
from collections.abc import AsyncIterator, Iterator, Mapping, Sequence
from datetime import datetime
from typing import TYPE_CHECKING, Any, Literal, Protocol, TypedDict, TypeVar, cast, overload

import pyarrow

# The native ``_wyrd.bifrost`` submodule is private and has no stub.
from .._wyrd import bifrost as _native  # ty: ignore[unresolved-import]
from ..client import WyrdClient
from ..types import TIMESTAMP_TYPES, TimestampLTZ, TimestampNTZ, TimestampTZ

if TYPE_CHECKING:
    from . import QueryTerminal
else:
    QueryTerminal = _native.QueryTerminal

_Row = TypeVar("_Row", bound="RowModel")

QueryParam = None | bool | int | float | str
"""One positional SQL bind value: ``params[i]`` binds placeholder ``$(i + 1)``."""


class RowModel(Protocol):
    """Anything that validates one row mapping and returns itself typed.

    A Pydantic ``BaseModel`` subclass satisfies this by construction, which is
    what ``sql(query, Model)`` accepts. The shape is structural, so Pydantic is
    not required by this module and any peer offering ``model_validate`` works
    unchanged.
    """

    @classmethod
    def model_validate(cls: type[_Row], obj: Any, /) -> _Row:
        """Validate one row mapping into an instance of this model.

        Args:
            obj: one row as a column-name mapping.

        Returns:
            The validated row.
        """

        ...


class BifrostQueryStream(AsyncIterator[pyarrow.RecordBatch]):
    """Asynchronously yields Arrow batches from one terminal-safe Oracle query."""

    def __init__(self, native_stream: Any) -> None:
        """Wrap a started native query stream; ``AsyncBifrost.stream()`` builds it.

        Args:
            native_stream: The native stream that yields Arrow IPC frames.
        """
        self._native = native_stream
        self._terminal: QueryTerminal | None = None
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
            terminal = self._native.terminal
            self._done = True
            if terminal is None:
                self._native.raise_incomplete_error()
                raise AssertionError("native incomplete projection must raise")
            self._terminal = terminal
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
    def terminal(self) -> QueryTerminal | None:
        """Return terminal metadata only after validated completion."""

        return self._terminal

    async def aclose(self) -> None:
        """Abandon this local response stream without requesting server cancellation."""

        self._done = True
        await asyncio.to_thread(self._native.close)


def _batch_ipc(batch: pyarrow.RecordBatch) -> bytes:
    """Frame one batch as the single-batch Arrow IPC stream the native call takes.

    The stream carries the batch's own schema, so the field metadata a
    canonical table declares - stable field id, sensitivity - reaches the
    server exactly as ``describe_table`` handed it out.
    """

    sink = pyarrow.BufferOutputStream()
    with pyarrow.ipc.new_stream(sink, batch.schema) as writer:
        writer.write_batch(batch)
    return bytes(sink.getvalue().to_pybytes())


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
    time_column: str | None,
) -> str | None:
    """Assemble the optional physical-layout declaration for the native edge.

    Omitting every argument omits the declaration entirely, which is what makes
    the server resolve its own defaults. Supplying any part requires a
    partition granularity on the wire, so an unstated one becomes ``"hour"`` —
    the same value the server would have chosen.
    """

    if (
        partition_granularity is None
        and sort_keys is None
        and bloom_columns is None
        and time_column is None
    ):
        return None
    layout: dict[str, Any] = {
        "partition_granularity": partition_granularity or "hour",
        "sort_keys": sort_keys or [],
        "bloom_columns": bloom_columns or [],
    }
    if time_column is not None:
        layout["time_column"] = time_column
    return json.dumps(layout)


class ResolvedTable(TypedDict):
    """The server-minted identity of a registered table."""

    table_uid: str
    fingerprint: str


class Correlation(TypedDict, total=False):
    """Optional per-row correlation; an omitted key is a null on the wire."""

    card_uid: str
    run_id: str


class TableConfig:
    """One Bifrost table: its name, declared user columns, and requested layout.

    A config is inert until ``Bifrost.register()`` or ``TableConfig.describe()``
    resolves it; until then ``resolved`` is ``None``. The server, not the
    client, mints the table identity and schema fingerprint.
    """

    def __init__(
        self,
        model: type[Any],
        table: str,
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
        time_column: str | None = None,
        compaction_target_file_size_bytes: int | None = None,
        compaction_type: str | None = None,
    ) -> None:
        """Declare a table from a Pydantic model class.

        The four layout arguments travel as one layout request. Omitting all
        of them lets the server choose its default layout: hourly partitions
        on ``wyrd_ingested_at``, sorted by it descending with nulls last, and
        only the managed Bloom columns. Layout column names are checked against the table's
        full physical schema, managed columns included, at register time.

        Args:
            model: a Pydantic model class, not an instance. Its
                ``model_json_schema()`` becomes the declared user columns:
                a nested model is a Struct, a typed list a List, and a
                free-form (``Any``, open ``dict``) or union field a Variant.
                Datetime fields declare the timestamp types in
                ``wyrd.types``: ``NaiveDatetime`` is ``TIMESTAMP_NTZ`` and any
                other ``datetime`` is ``TIMESTAMP_LTZ``.
                ``card_uid``, ``run_id``, ``sub_principal_id``,
                ``act_principal_id``, and ``wyrd_*`` names are reserved.
            table: the ``"<namespace>.<name>"`` name SQL uses.
            partition_granularity: ``"hour"`` or ``"day"`` partitions on
                the time column. Omitted while another layout argument is
                set, ``"hour"`` is sent.
            sort_keys: up to four ``SortKey`` mappings in priority order;
                ``direction`` is ``"asc"`` or ``"desc"`` and ``null_order``
                is ``"first"`` or ``"last"``. Omitted or empty, the table
                sorts by the time column descending with nulls last.
            bloom_columns: extra columns to Bloom-filter. The managed
                ``run_id``, ``card_uid``, ``sub_principal_id``, and
                ``act_principal_id`` columns the
                table has always come first, and naming one is harmless.
                Omitted or empty, only those managed columns are filtered.
            time_column: the non-null ``TIMESTAMP_LTZ`` or ``TIMESTAMP_NTZ``
                column that partitions the table and leads its default sort
                order. Any instant is accepted. Omitted, it is
                ``wyrd_ingested_at``, the server's receipt time.
            compaction_target_file_size_bytes: the file size Forge compacts
                this table toward, at least 134217728 (128 MiB). Omitted, the
                table follows the deployment default (1 GiB unless the
                operator changed it).
            compaction_type: ``"auto"``, ``"full"``, ``"small-files"``, or
                ``"files-with-delete"``. Omitted, Forge compacts
                ``small-files``. A copy-on-write table always compacts
                ``full``.

        Raises:
            WyrdError: ``WYRD_VALA_400_SCHEMA_PARSE`` when ``table`` is not
                ``namespace.name`` or the model schema has no column mapping;
                ``WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`` for a column type
                Bifrost cannot store;
                ``WYRD_VALA_400_BIFROST_RESERVED_COLUMN`` for a reserved
                column; ``WYRD_SPEC_400_VALIDATION`` for an unknown
                granularity, sort-key, or compaction-type spelling.
                ``WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT`` from
                ``Bifrost.register()`` names a ``time_column`` that is absent,
                nullable, or not a timestamp. Column,
                file-size, and stored-setting conflicts raise from
                ``Bifrost.register()``.

        """
        if "pydantic" in sys.modules:
            from ._pydantic import json_schema

            schema = json_schema(model)
        else:
            schema = model.model_json_schema()
        self._native = TableConfig.from_json_schema(
            table,
            schema,
            partition_granularity,
            sort_keys,
            bloom_columns,
            time_column,
            compaction_target_file_size_bytes,
            compaction_type,
        )._native

    @classmethod
    def _from_native(cls, native: Any) -> TableConfig:
        """Wrap one already-built native config without re-mapping a schema."""

        config = cls.__new__(cls)
        config._native = native
        return config

    @staticmethod
    def from_json_schema(
        table: str,
        schema: Mapping[str, Any],
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
        time_column: str | None = None,
        compaction_target_file_size_bytes: int | None = None,
        compaction_type: str | None = None,
    ) -> TableConfig:
        """Declare a table from a JSON Schema object, as Rust and TypeScript do.

        Use this when the columns come from a schema document rather than a
        Pydantic model; ``TableConfig()`` calls it with
        ``model.model_json_schema()``. Each ``properties`` entry becomes one
        user column, and ``required`` marks it non-nullable.

        Args:
            table: the ``"<namespace>.<name>"`` name SQL uses.
            schema: a JSON Schema object with a ``properties`` mapping.
                ``card_uid``, ``run_id``, ``sub_principal_id``,
                ``act_principal_id``, and ``wyrd_*`` names are reserved.
            partition_granularity: ``"hour"`` or ``"day"``, as for
                ``TableConfig()``.
            sort_keys: up to four ``SortKey`` mappings, as for
                ``TableConfig()``.
            bloom_columns: extra columns to Bloom-filter, as for
                ``TableConfig()``.
            time_column: the partitioning timestamp column, as for
                ``TableConfig()``.
            compaction_target_file_size_bytes: the Forge compaction file
                target, at least 134217728 bytes. Omitted, the deployment
                default.
            compaction_type: ``"auto"``, ``"full"``, ``"small-files"``, or
                ``"files-with-delete"``. Omitted, ``small-files``.

        Returns:
            An unresolved config.

        Raises:
            WyrdError: ``WYRD_VALA_400_SCHEMA_PARSE`` when ``table`` is not
                ``namespace.name`` or ``schema`` has no column mapping;
                ``WYRD_VALA_400_BIFROST_RESERVED_COLUMN`` for a reserved
                column; otherwise as for ``TableConfig()``.
        """

        return TableConfig._from_native(
            _native.TableConfig.from_json_schema(
                table,
                json.dumps(dict(schema)),
                _layout_json(partition_granularity, sort_keys, bloom_columns, time_column),
                compaction_target_file_size_bytes,
                compaction_type,
            )
        )

    @staticmethod
    def from_arrow(
        table: str,
        schema: pyarrow.Schema,
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
        time_column: str | None = None,
        compaction_target_file_size_bytes: int | None = None,
        compaction_type: str | None = None,
    ) -> TableConfig:
        """Declare a table from an explicit ``pyarrow.Schema``.

        Use this for column types JSON Schema cannot express, such as
        ``int32`` or ``decimal128``. A Variant column is a field carrying the
        ``arrow.parquet.variant`` extension. ``schema`` holds
        user columns only; every other error is as for ``TableConfig()``.

        Args:
            table: the ``"<namespace>.<name>"`` name SQL uses.
            schema: the declared user columns.
            partition_granularity: ``"hour"`` or ``"day"``, as for
                ``TableConfig()``.
            sort_keys: up to four ``SortKey`` mappings, as for
                ``TableConfig()``.
            bloom_columns: extra columns to Bloom-filter, as for
                ``TableConfig()``.
            time_column: the partitioning timestamp column, as for
                ``TableConfig()``.
            compaction_target_file_size_bytes: the Forge compaction file
                target, at least 134217728 bytes. Omitted, the deployment
                default.
            compaction_type: ``"auto"``, ``"full"``, ``"small-files"``, or
                ``"files-with-delete"``. Omitted, ``small-files``.

        Returns:
            An unresolved config.

        Raises:
            WyrdError: As for ``TableConfig()``.
        """

        return TableConfig._from_native(
            _native.TableConfig.from_arrow_ipc(
                table,
                schema.serialize().to_pybytes(),
                _layout_json(partition_granularity, sort_keys, bloom_columns, time_column),
                compaction_target_file_size_bytes,
                compaction_type,
            )
        )

    @staticmethod
    def describe(
        table: str,
        client: WyrdClient | None = None,
    ) -> TableConfig:
        """Fetch an already-registered table's config by name.

        The result is already resolved and carries the server's stored schema,
        layout, and compaction settings.

        Args:
            table: the ``"<namespace>.<name>"`` table to describe.
            client: the ``WyrdClient`` to act as. Omitted, the ambient client
                resolves as for ``Bifrost()``.

        Returns:
            The resolved config.

        Raises:
            WyrdError: ``WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`` for an
                unknown table, ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no
                credential resolves, or the server's authorization or
                transport error.

        """

        return TableConfig._from_native(_native.TableConfig.describe(table, client))

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
    def compaction_target_file_size_bytes(self) -> int | None:
        """The explicit Forge compaction file target, or ``None`` for the
        server's deployment default."""

        value = self._native.compaction_target_file_size_bytes
        return None if value is None else int(value)

    @property
    def compaction_type(self) -> str | None:
        """The explicit Forge compaction type, or ``None`` for the
        ``small-files`` default."""

        value = self._native.compaction_type
        return None if value is None else str(value)

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
        """Wrap a collected native result; ``Bifrost.sql()`` builds it.

        Args:
            native: The native result holding the Arrow IPC stream.
        """
        self._native = native

    def to_arrow(self) -> pyarrow.Table:
        """Read the result as one ``pyarrow.Table``.

        Returns:
            Every batch concatenated under the result schema.
        """

        return pyarrow.ipc.open_stream(self._native.to_bytes()).read_all()

    @property
    def batches(self) -> list[pyarrow.RecordBatch]:
        """The result's Arrow record batches, in server order.

        Returns:
            One ``pyarrow.RecordBatch`` per batch the server streamed; empty
            when no row matched.
        """

        return list(pyarrow.ipc.open_stream(self._native.to_bytes()))

    @property
    def schema(self) -> pyarrow.Schema:
        """The result's Arrow schema.

        Returns:
            The projected columns, present even when no row matched.
        """

        return pyarrow.ipc.open_stream(self._native.to_bytes()).schema

    @property
    def num_rows(self) -> int:
        """Total rows across every batch; the same count as ``len(result)``.

        Returns:
            The row count.
        """

        return len(self._native)

    def to_polars(self) -> Any:
        """Read the result as a Polars DataFrame, zero-copy from Arrow."""

        import polars

        return polars.from_arrow(self.to_arrow())

    def to_pandas(self) -> Any:
        """Read the result as a pandas DataFrame."""

        return self.to_arrow().to_pandas()

    def to_bytes(self) -> bytes:
        """The raw Arrow IPC stream, for a caller with its own reader."""

        return bytes(self._native.to_bytes())

    @property
    def terminal(self) -> QueryTerminal:
        """The validated terminal frame the server closed the stream with."""

        return self._native.terminal

    def __len__(self) -> int:
        """Total rows across every batch."""

        return len(self._native)


class BifrostBatchIterator(Iterator[pyarrow.RecordBatch]):
    """Synchronously yields Arrow batches from one terminal-safe query.

    Completion requires the terminal frame: a stream that ends without one
    raises rather than looking like an empty result.
    """

    def __init__(self, native_stream: Any) -> None:
        """Wrap a started native query stream; ``Bifrost.stream()`` builds it.

        Args:
            native_stream: The native stream that yields Arrow IPC frames.
        """
        self._native = native_stream
        self._terminal: QueryTerminal | None = None
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
            terminal = self._native.terminal
            if terminal is None:
                self._native.raise_incomplete_error()
                raise AssertionError("native incomplete projection must raise")
            self._terminal = terminal
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
    def terminal(self) -> QueryTerminal | None:
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
        client: WyrdClient | None = None,
        client_byte_limit_bytes: int | None = None,
    ) -> None:
        """Connect one client, optionally already bound to a write target.

        Args:
            table: the active write target. Omitted, the client is query-only
                until ``use_table``.
            client: the ``WyrdClient`` to act as, such as one returned by
                ``on_behalf_of``, sharing its authentication and transport.
                Omitted, the ambient client resolves from ``[client]`` in the
                Wyrd ``config.toml``, then the environment, then the saved
                ``wyrd auth login``.
            client_byte_limit_bytes: the handle-wide ingestion byte budget
                shared by every table this client writes. 256 MiB if omitted.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no ambient
                credential resolves;
                ``WYRD_CLIENT_400_CONFIG_INVALID`` when the byte budget is too
                small to seal one message.

        """

        self._native = _native.Bifrost(
            table._native if table is not None else None,
            client,
            client_byte_limit_bytes,
        )

    def use_table(self, table: TableConfig) -> TableConfig | None:
        """Bind ``table`` as the write target, returning the previous binding.

        The previous table's producer stays pooled, so its buffered rows still
        flush; a swap loses nothing.

        Args:
            table: the new write target.

        Returns:
            The previous binding, or ``None``.
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

        Non-blocking; the row is durable after ``flush()``. A saturated queue
        refuses here rather than dropping silently.

        Args:
            row: a Pydantic model instance, a mapping, or a JSON object string
                holding the table's declared columns.
            correlation: optional ``card_uid`` and ``run_id`` for this row.
                Omitted keys are stored as null.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unparsable
                ``card_uid``; ``WYRD_VALA_412_NO_ACTIVE_TABLE`` when no table
                is bound; ``WYRD_CLIENT_429_QUEUE_FULL`` when the producer
                queue is full.

        """

        correlation = correlation or {}
        self._native.insert(
            _row_json(row),
            correlation.get("card_uid"),
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
        """Create the active table; returns ``"created"`` or ``"already_exists"``.

        Registering again with the same schema and settings is idempotent.

        Returns:
            ``"created"`` or ``"already_exists"``.

        Raises:
            WyrdError: ``WYRD_VALA_412_NO_ACTIVE_TABLE`` when no table is
                bound; ``WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT`` or
                ``WYRD_VALA_400_BIFROST_INVALID_COMPACTION_TARGET`` for a bad
                layout or file target; ``WYRD_VALA_409_BIFROST_*_MISMATCH``
                when the table exists with different columns, layout, or
                compaction settings.

        """

        return str(self._native.register())

    def use_table_by_name(self, table: str) -> None:
        """Bind the registered ``"<namespace>.<name>"`` table, describing it first.

        Args:
            table: the registered ``"<namespace>.<name>"`` table.

        Raises:
            WyrdError: as for ``TableConfig.describe()``.

        """

        self._native.use_table_by_name(table)

    def write_batch(self, table: str, batch: pyarrow.RecordBatch) -> None:
        """Write one already-built Arrow batch to ``table`` and await durability.

        The precision write door, beside ``insert``: it names its destination
        instead of using the active binding, carries correlation as ordinary
        columns, and is durable when it returns, so no ``flush`` follows it.
        Build the batch against ``TableConfig.describe(...).arrow_schema``.
        Columns match the table's declared columns by name, in any order; the
        server judges every column's type and refuses a stale declaration.

        Args:
            table: the destination ``"<namespace>.<name>"`` table.
            batch: the rows, built against the table's Arrow schema.

        Raises:
            WyrdError: the batch exceeded the byte envelope, or the server
                refused it; the stable ``code`` says which.

        """

        self._native.write_batch(table, _batch_ipc(batch))

    def flush(self) -> None:
        """Flush every pooled producer and await each durable acknowledgement.

        Raises:
            WyrdError: the first producer or server failure, after every
                producer has been attempted.

        """

        self._native.flush()

    def shutdown(self) -> None:
        """Drain every producer and stop its background task.

        Raises:
            WyrdError: as for ``flush()``.

        """

        self._native.shutdown()

    @overload
    def sql(self, query: str, params: Sequence[QueryParam] | None = None) -> QueryResult: ...

    @overload
    def sql(
        self, query: str, params: Sequence[QueryParam] | None = None, *, model: type[_Row]
    ) -> list[_Row]: ...

    def sql(
        self,
        query: str,
        params: Sequence[QueryParam] | None = None,
        *,
        model: type[_Row] | None = None,
    ) -> QueryResult | list[_Row]:
        """Run one SQL SELECT over any authorized table and collect every batch.

        ``params`` bind the ``$1..$n`` placeholders in order as typed values;
        they are never interpolated into ``query``. Passing ``model``
        validates every row through it locally and returns model instances
        instead of the Arrow-backed ``QueryResult``. The model never reaches
        the server and describes the columns the query selects, not how a
        table is stored.

        Args:
            query: one SELECT statement.
            params: typed values for the ``$1..$n`` placeholders.
            model: a row model that validates each row.

        Returns:
            A ``QueryResult``, or a list of ``model`` instances.

        Raises:
            WyrdError: invalid or refused SQL, an authorization or transport
                failure, or ``WYRD_VALA_502_QUERY_STREAM_INCOMPLETE`` when
                the response ends without its terminal frame.
            pydantic.ValidationError: a row did not fit ``model``. Nothing
                partially converted is returned.
        """

        result = QueryResult(self._native.sql(query, params))
        return result if model is None else _validated_rows(result, model)

    def stream(
        self,
        query: str,
        params: Sequence[QueryParam] | None = None,
        deadline_ms: int | None = None,
    ) -> BifrostBatchIterator:
        """Run one SQL SELECT and iterate its batches as they arrive.

        Args:
            query: one SELECT statement.
            params: typed values for the ``$1..$n`` placeholders.
            deadline_ms: the query deadline, from 1 to 4294967295
                milliseconds. Omitted, the server's default deadline applies.

        Returns:
            An iterator over the result batches.

        Raises:
            WyrdError: an invalid request or a transport failure when the
                query starts; later failures raise during iteration.

        """

        return BifrostBatchIterator(self._native.stream(query, params, deadline_ms))

    def running(self) -> list[RunningQuery]:
        """List the authenticated tenant's active queries.

        Returns:
            The tenant's active queries.

        Raises:
            WyrdError: The server's refusal or a transport failure.
        """

        return list(self._native.running())

    def status(self, request_id: str) -> RunningQuery:
        """Return one active query by the ``request_id`` its stream reported.

        Args:
            request_id: the ``request_id`` a stream reported.

        Returns:
            The active query.

        Raises:
            WyrdError: a malformed ID, or the server's refusal or a
                transport failure.

        """

        return cast("RunningQuery", self._native.status(request_id))

    def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream.

        Repeating the request is harmless.

        Args:
            request_id: the ``request_id`` a stream reported.

        Returns:
            The cancellation result.

        Raises:
            WyrdError: As for ``status()``.
        """

        return cast("CancelRunningQueryResult", self._native.cancel(request_id))

    def trace_timeline(self, trace_id: str) -> list[SpanRecord]:
        """Read one trace's span tree plus each trace linking directly to it.

        Spans come back deduplicated, ordered by start time, then trace id,
        then span id. A trace no span names reads as an empty list.

        Args:
            trace_id: the 32-character lowercase hex W3C trace id.

        Returns:
            The canonical span records.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a malformed or
                all-zero ``trace_id``, before any request; otherwise a query,
                authorization, or transport failure.
        """

        return cast("list[SpanRecord]", self._native.trace_timeline(trace_id))

    def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe the registered ``namespace.name`` table's stored schema and layout.

        Args:
            namespace: the table namespace.
            name: the table name.

        Returns:
            The stored schema and layout.

        Raises:
            WyrdError: ``WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`` or an
                authorization or transport failure.

        """

        return cast("TableDescription", self._native.describe_table(namespace, name))


class AsyncBifrost(_BifrostBase):
    """The ``await`` surface over the same native client.

    Every blocking native call runs in a worker thread, so the event loop is
    never blocked. ``insert`` and ``use_table`` stay synchronous because they
    perform no IO. Each method behaves as its ``Bifrost`` counterpart.
    """

    async def register(self) -> str:
        """Create the active table; returns ``"created"`` or ``"already_exists"``.

        Returns:
            ``"created"`` or ``"already_exists"``.

        Raises:
            WyrdError: As for ``Bifrost.register()``.
        """

        return str(await asyncio.to_thread(self._native.register))

    async def use_table_by_name(self, table: str) -> None:
        """Bind the registered ``"<namespace>.<name>"`` table, describing it first.

        Args:
            table: the registered ``"<namespace>.<name>"`` table.

        Raises:
            WyrdError: As for ``TableConfig.describe()``.
        """

        await asyncio.to_thread(self._native.use_table_by_name, table)

    async def write_batch(self, table: str, batch: pyarrow.RecordBatch) -> None:
        """Write one already-built Arrow batch to ``table`` and await durability.

        The ``await`` form of ``Bifrost.write_batch()``; the encode and the
        blocking native write both run in a worker thread.

        Args:
            table: the destination ``"<namespace>.<name>"`` table.
            batch: the rows, built against the table's Arrow schema.

        Raises:
            WyrdError: As for ``Bifrost.write_batch()``.
        """

        await asyncio.to_thread(self._native.write_batch, table, _batch_ipc(batch))

    async def flush(self) -> None:
        """Flush every pooled producer and await each durable acknowledgement.

        Raises:
            WyrdError: As for ``Bifrost.flush()``.
        """

        await asyncio.to_thread(self._native.flush)

    async def shutdown(self) -> None:
        """Drain every producer and stop its background task.

        Raises:
            WyrdError: As for ``Bifrost.flush()``.
        """

        await asyncio.to_thread(self._native.shutdown)

    @overload
    async def sql(self, query: str, params: Sequence[QueryParam] | None = None) -> QueryResult: ...

    @overload
    async def sql(
        self, query: str, params: Sequence[QueryParam] | None = None, *, model: type[_Row]
    ) -> list[_Row]: ...

    async def sql(
        self,
        query: str,
        params: Sequence[QueryParam] | None = None,
        *,
        model: type[_Row] | None = None,
    ) -> QueryResult | list[_Row]:
        """Run one SQL SELECT over any authorized table and collect every batch.

        As ``Bifrost.sql``, including the optional ``model`` projection; only
        the query itself moves to a worker thread.

        Args:
            query: one SELECT statement.
            params: typed values for the ``$1..$n`` placeholders.
            model: a row model that validates each row.

        Returns:
            A ``QueryResult``, or a list of ``model`` instances.

        Raises:
            WyrdError: As for ``Bifrost.sql()``.
            pydantic.ValidationError: a row did not fit ``model``. Nothing
                partially converted is returned.
        """

        result = QueryResult(await asyncio.to_thread(self._native.sql, query, params))
        return result if model is None else _validated_rows(result, model)

    async def stream(
        self,
        query: str,
        params: Sequence[QueryParam] | None = None,
        deadline_ms: int | None = None,
    ) -> BifrostQueryStream:
        """Start one query and iterate its batches with ``async for``.

        Args:
            query: one SELECT statement.
            params: typed values for the ``$1..$n`` placeholders.
            deadline_ms: the query deadline, from 1 to 4294967295
                milliseconds. Omitted, the server's default deadline applies.

        Returns:
            An async iterator over the result batches.

        Raises:
            WyrdError: As for ``Bifrost.stream()``.
        """

        native_stream = await asyncio.to_thread(
            self._native.stream,
            query,
            params,
            deadline_ms,
        )
        return BifrostQueryStream(native_stream)

    async def running(self) -> list[RunningQuery]:
        """List the authenticated tenant's active queries.

        Returns:
            The tenant's active queries.

        Raises:
            WyrdError: The server's refusal or a transport failure.
        """

        return list(await asyncio.to_thread(self._native.running))

    async def status(self, request_id: str) -> RunningQuery:
        """Return one active query by its ``request_id``.

        Args:
            request_id: the ``request_id`` a stream reported.

        Returns:
            The active query.

        Raises:
            WyrdError: As for ``Bifrost.status()``.
        """

        return cast("RunningQuery", await asyncio.to_thread(self._native.status, request_id))

    async def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream.

        Args:
            request_id: the ``request_id`` a stream reported.

        Returns:
            The cancellation result.

        Raises:
            WyrdError: As for ``Bifrost.status()``.
        """

        return cast(
            "CancelRunningQueryResult",
            await asyncio.to_thread(self._native.cancel, request_id),
        )

    async def trace_timeline(self, trace_id: str) -> list[SpanRecord]:
        """Read one trace's span tree; as ``Bifrost.trace_timeline()``.

        Args:
            trace_id: the 32-character lowercase hex W3C trace id.

        Returns:
            The canonical span records.

        Raises:
            WyrdError: As for ``Bifrost.trace_timeline()``.
        """

        return cast(
            "list[SpanRecord]",
            await asyncio.to_thread(self._native.trace_timeline, trace_id),
        )

    async def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe the registered ``namespace.name`` table's stored schema and layout.

        Args:
            namespace: the table namespace.
            name: the table name.

        Returns:
            The stored schema and layout.

        Raises:
            WyrdError: As for ``Bifrost.describe_table()``.
        """

        return cast(
            "TableDescription",
            await asyncio.to_thread(self._native.describe_table, namespace, name),
        )


class DataTypeSpecVariants(TypedDict, total=False):
    """The parameterized `DataTypeSpec` forms, tagged by their variant name.

    A scalar type is the bare variant name as a string; everything below carries
    parameters, so it arrives as a single-key object. `List`, `Struct`, and
    `Map` hold full field declarations, which is what makes the type recursive.
    `Map` carries its non-nullable `key` and its `value` declarations.
    """

    FixedSizeBinary: dict[str, int]
    Timestamp: dict[str, str | None]
    Time32: dict[str, str]
    Time64: dict[str, str]
    Decimal128: dict[str, int]
    List: FieldSpec
    Struct: list[FieldSpec]
    Map: dict[str, FieldSpec]


DataTypeSpec = str | DataTypeSpecVariants
"""One column's logical type: a scalar variant name or a parameterized form."""


class _FieldSpecOptional(TypedDict, total=False):
    """The described-column keys the server omits when they carry nothing.

    `metadata` holds the stable field id under `PARQUET:field_id`; an empty
    map is omitted.
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
    """One sort key: ``direction`` is ``"asc"`` or ``"desc"``, ``null_order``
    is ``"first"`` or ``"last"``."""

    column: str
    direction: str
    null_order: str


class PhysicalLayout(TypedDict):
    """A table's server-resolved partitioning, sort order, and Bloom columns.

    ``partition_granularity`` is ``"hour"`` or ``"day"`` on ``time_column``,
    which is ``wyrd_ingested_at`` unless the table declared its own;
    ``bloom_columns`` begins with the managed columns.
    """

    partition_granularity: str
    time_column: str
    sort_keys: list[SortKey]
    bloom_columns: list[str]


class _TableDescriptionOptional(TypedDict, total=False):
    """Description fields the server omits when they do not apply.

    ``canonical_physical_fingerprint`` is present only for built-in signal
    tables. The compaction keys are present only when the table stores its
    own value; absent means the server default.
    """

    canonical_physical_fingerprint: str
    compaction_target_file_size_bytes: int
    compaction_type: str


class TableDescription(_TableDescriptionOptional):
    """Server projection of one registered table's stored physical schema.

    ``user_fields`` are the declared columns, ``correlation_fields`` the
    write-time ``card_uid``/``run_id`` inputs.
    """

    entry: TableEntry
    user_fields: list[FieldSpec]
    correlation_fields: list[FieldSpec]
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


class _ResourceOptional(TypedDict, total=False):
    attributes: dict[str, Any]
    service_instance_id: str | None
    service_namespace: str | None
    service_version: str | None


class Resource(_ResourceOptional):
    """OTel resource of the process that produced a span."""

    service_name: str


class _InstrumentationScopeOptional(TypedDict, total=False):
    attributes: dict[str, Any]
    version: str | None


class InstrumentationScope(_InstrumentationScopeOptional):
    """OTel instrumentation scope that produced a span."""

    name: str


class _SpanStatusOptional(TypedDict, total=False):
    description: str | None


class SpanStatus(_SpanStatusOptional):
    """OTel span status; ``description`` appears only on ``error``."""

    code: Literal["unset", "ok", "error"]


class _SpanEventOptional(TypedDict, total=False):
    attributes: dict[str, Any]
    dropped_attributes_count: int


class SpanEvent(_SpanEventOptional):
    """One timestamped event inside a span."""

    name: str
    timestamp: str


class _SpanLinkOptional(TypedDict, total=False):
    attributes: dict[str, Any]
    dropped_attributes_count: int
    flags: int
    trace_state: str


class SpanLink(_SpanLinkOptional):
    """A link from one span to another span's identity."""

    trace_id: str
    span_id: str


class _SpanRecordOptional(TypedDict, total=False):
    attributes: dict[str, Any]
    dropped_attributes_count: int
    dropped_events_count: int
    dropped_links_count: int
    events: list[SpanEvent]
    flags: int
    links: list[SpanLink]
    parent_span_id: str | None
    trace_state: str


class SpanRecord(_SpanRecordOptional):
    """One canonical OTel span as ``trace_timeline`` returns it."""

    trace_id: str
    span_id: str
    name: str
    kind: Literal["internal", "server", "client", "producer", "consumer"]
    start_time: str
    end_time: str
    duration_ms: int
    resource: Resource
    scope: InstrumentationScope
    status: SpanStatus


def _validated_rows(result: QueryResult, model: type[_Row]) -> list[_Row]:
    """Validate every row of a completed result through ``model``.

    The rows come from the Arrow table the result already decoded, so the keys
    ``model`` sees are exactly the columns the server returned. Validation is
    all-or-nothing: the first rejected row raises and no partial list escapes.
    Variant cells, top level or nested in a Struct or List, arrive as their
    native ``dict``, ``list``, or scalar value rather than as encoded bytes.
    """

    table = result.to_arrow()
    decode = result._native.variant_to_python
    variant_columns = [field for field in table.schema if _holds_variant(field)]
    timestamp_columns = [
        (name, TIMESTAMP_TYPES[kind]) for name, kind in result._native.timestamp_kinds()
    ]
    rows = table.to_pylist()
    for name, timestamp in timestamp_columns:
        if timestamp is TimestampTZ:
            for row, text in zip(rows, result._native.timestamp_tz_texts(name), strict=True):
                row[name] = text
    for row in rows:
        for field in variant_columns:
            row[field.name] = _native_value(field, row[field.name], decode)
        for name, timestamp in timestamp_columns:
            row[name] = _wyrd_timestamp(timestamp, row[name])
    return [model.model_validate(row) for row in rows]


def _wyrd_timestamp(
    timestamp: type[TimestampNTZ] | type[TimestampLTZ] | type[TimestampTZ], value: Any
) -> Any:
    """Convert one top-level timestamp cell to its Wyrd timestamp type.

    A ``TIMESTAMP_TZ`` cell arrives as the RFC 3339 text the native module
    rendered in the writer's offset; the other two arrive as the ``datetime``
    pyarrow produced. Every model field, Wyrd or ``datetime``, then validates a
    Wyrd value.
    """

    if value is None:
        return None
    if isinstance(value, str):
        value = datetime.fromisoformat(value)
    return timestamp.of(value)


_VARIANT_EXTENSION = b"arrow.parquet.variant"


def _is_variant(field: pyarrow.Field) -> bool:
    """Whether ``field`` carries the Arrow Variant extension.

    pyarrow either keeps the extension as field metadata or, when it knows the
    type, as an extension type; both spellings name the same column.
    """

    name = getattr(field.type, "extension_name", None)
    if name is not None:
        return bool(name.encode() == _VARIANT_EXTENSION)
    return (field.metadata or {}).get(b"ARROW:extension:name") == _VARIANT_EXTENSION


def _holds_variant(field: pyarrow.Field) -> bool:
    """Whether ``field`` is a Variant or nests one inside a Struct or List."""

    if _is_variant(field):
        return True
    kind = field.type
    if pyarrow.types.is_struct(kind):
        return any(_holds_variant(kind.field(index)) for index in range(kind.num_fields))
    if pyarrow.types.is_list(kind) or pyarrow.types.is_large_list(kind):
        return _holds_variant(kind.value_field)
    return False


def _native_value(field: pyarrow.Field, value: Any, decode: Any) -> Any:
    """Replace every Variant cell inside one ``to_pylist`` value with its
    native Python value, leaving every other value as pyarrow produced it."""

    if value is None or not _holds_variant(field):
        return value
    if _is_variant(field):
        return decode(value["metadata"], value["value"])
    kind = field.type
    if pyarrow.types.is_struct(kind):
        children = (kind.field(index) for index in range(kind.num_fields))
        return {child.name: _native_value(child, value[child.name], decode) for child in children}
    return [_native_value(kind.value_field, item, decode) for item in value]
