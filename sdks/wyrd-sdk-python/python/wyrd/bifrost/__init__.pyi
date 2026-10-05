# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
from collections.abc import AsyncIterator, Iterator
from typing import Any, Protocol, TypedDict, TypeVar, overload

import pyarrow

from ..client import WyrdClient

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
    """One sort key: ``direction`` is ``"asc"`` or ``"desc"``, ``null_order``
    is ``"first"`` or ``"last"``."""

    column: str
    direction: str
    null_order: str

class PhysicalLayout(TypedDict):
    """A table's server-resolved partitioning, sort order, and Bloom columns.

    ``partition_granularity`` is ``"hour"`` or ``"day"`` on
    ``wyrd_event_time``; ``bloom_columns`` begins with the managed columns.
    """

    partition_granularity: str
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
    write-time ``card_ref``/``run_id`` inputs, and ``managed_candidates`` the
    managed columns a writer may supply itself.
    """

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

class BifrostQueryStream(AsyncIterator[pyarrow.RecordBatch]):
    """Asynchronously yields Arrow batches from one terminal-safe Oracle query."""

    def __aiter__(self) -> BifrostQueryStream: ...
    async def __anext__(self) -> pyarrow.RecordBatch: ...
    @property
    def request_id(self) -> str:
        """Return the server lifecycle request ID before stream completion."""
        ...
    @property
    def terminal(self) -> dict[str, Any] | None:
        """Return terminal metadata only after validated completion."""
        ...
    async def aclose(self) -> None:
        """Abandon this local response stream without requesting server cancellation."""
        ...

class ResolvedTable(TypedDict):
    """The server-minted identity of a registered table."""

    table_uid: str
    fingerprint: str

class Correlation(TypedDict, total=False):
    """Optional per-row correlation; an omitted key is a null on the wire."""

    card_ref: str
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
        compaction_target_file_size_bytes: int | None = None,
        compaction_type: str | None = None,
    ) -> None:
        """Declare a table from a Pydantic model class.

        The three layout arguments travel as one layout request. Omitting all
        of them lets the server choose its default layout: hourly partitions,
        ``wyrd_event_time`` descending with nulls last, and only the managed
        Bloom columns. Layout column names are checked against the table's
        full physical schema, managed columns included, at register time.

        Args:
            model: a Pydantic model class, not an instance. Its
                ``model_json_schema()`` becomes the declared user columns.
                ``card_ref``, ``run_id``, and ``wyrd_*`` names are reserved.
            table: the ``"<namespace>.<name>"`` name SQL uses.
            partition_granularity: ``"hour"`` or ``"day"`` partitions on
                ``wyrd_event_time``. Omitted while another layout argument is
                set, ``"hour"`` is sent.
            sort_keys: up to four ``SortKey`` mappings in priority order;
                ``direction`` is ``"asc"`` or ``"desc"`` and ``null_order``
                is ``"first"`` or ``"last"``. Omitted or empty, the table
                sorts by ``wyrd_event_time`` descending with nulls last.
            bloom_columns: extra columns to Bloom-filter. The managed
                ``run_id``, ``card_uid``, and ``principal_id`` columns the
                table has always come first, and naming one is harmless.
                Omitted or empty, only those managed columns are filtered.
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
                ``WYRD_VALA_400_BIFROST_RESERVED_COLUMN`` for a reserved
                column; ``WYRD_SPEC_400_VALIDATION`` for an unknown
                granularity, sort-key, or compaction-type spelling. Column,
                file-size, and stored-setting conflicts raise from
                ``Bifrost.register()``.

        """
        ...
    @staticmethod
    def from_arrow(
        schema: pyarrow.Schema,
        table: str,
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
        compaction_target_file_size_bytes: int | None = None,
        compaction_type: str | None = None,
    ) -> TableConfig:
        """Declare a table from an explicit ``pyarrow.Schema``.

        Use this for column types JSON Schema cannot express, such as
        ``int32``, a non-UTC timestamp, or ``decimal128``. ``schema`` holds
        user columns only; every other argument and error is as for
        ``TableConfig()``.
        """
        ...
    @staticmethod
    def describe(
        table: str,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> TableConfig:
        """Fetch an already-registered table's config by name.

        The result is already resolved and carries the server's stored schema,
        layout, and compaction settings.

        Args:
            table: the ``"<namespace>.<name>"`` table to describe.
            server_url: as for ``Bifrost()``.
            credential: as for ``Bifrost()``.
            grpc_url: as for ``Bifrost()``.

        Raises:
            WyrdError: ``WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`` for an
                unknown table, ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no
                credential resolves, or the server's authorization or
                transport error.

        """
        ...
    @property
    def fqn(self) -> str:
        """The ``namespace.name`` this config addresses."""
        ...
    @property
    def arrow_schema(self) -> pyarrow.Schema:
        """The declared user columns only.

        Correlation and managed columns are appended by the write path and the
        server, so they are absent here by construction.
        """
        ...
    @property
    def compaction_target_file_size_bytes(self) -> int | None:
        """The explicit Forge compaction file target, or ``None`` for the
        server's deployment default."""
        ...
    @property
    def compaction_type(self) -> str | None:
        """The explicit Forge compaction type, or ``None`` for the
        ``small-files`` default."""
        ...
    @property
    def resolved(self) -> ResolvedTable | None:
        """The server-assigned identity, or ``None`` while unregistered."""
        ...

class QueryResult:
    """Arrow batches from one query, converted on demand."""

    def to_arrow(self) -> pyarrow.Table:
        """Read the result as one ``pyarrow.Table``."""
        ...
    def to_polars(self) -> Any:
        """Read the result as a Polars DataFrame, zero-copy from Arrow."""
        ...
    def to_pandas(self) -> Any:
        """Read the result as a pandas DataFrame."""
        ...
    def to_bytes(self) -> bytes:
        """The raw Arrow IPC stream, for a caller with its own reader."""
        ...
    @property
    def terminal(self) -> dict[str, Any]:
        """The validated terminal frame the server closed the stream with."""
        ...
    def __len__(self) -> int:
        """Total rows across every batch."""
        ...

_Row = TypeVar("_Row", bound="RowModel")

class RowModel(Protocol):
    """Anything that validates one row mapping and returns itself typed."""

    @classmethod
    def model_validate(cls: type[_Row], obj: Any, /) -> _Row:
        """Validate one row mapping into an instance of this model."""
        ...

class BifrostBatchIterator(Iterator[pyarrow.RecordBatch]):
    """Synchronously yields Arrow batches from one terminal-safe query."""

    def __iter__(self) -> BifrostBatchIterator: ...
    def __next__(self) -> pyarrow.RecordBatch: ...
    @property
    def request_id(self) -> str:
        """The server lifecycle request ID, available before completion."""
        ...
    @property
    def terminal(self) -> dict[str, Any] | None:
        """Terminal metadata, present only after validated completion."""
        ...
    def close(self) -> None:
        """Abandon this local response stream without server cancellation."""
        ...

class _BifrostBase:
    """Everything the sync and async clients share."""

    def __init__(
        self,
        table: TableConfig | None = None,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
        client: WyrdClient | None = None,
        client_byte_limit_bytes: int | None = None,
    ) -> None:
        """Connect one client, optionally already bound to a write target.

        Args:
            table: the active write target. Omitted, the client is query-only
                until ``use_table``.
            server_url: the Bifrost server URL. Resolved from
                ``WYRD_SERVER_URL`` and then the compiled default if omitted.
            credential: the API key or bearer token. Resolved through
                ``WYRD_ACCESS_TOKEN`` → ``WYRD_WORKLOAD_TOKEN`` + tenant →
                ``WYRD_API_KEY`` → this server's saved ``wyrd auth login`` →
                ``~/.config/wyrd/credentials.toml``
                ``[default].api_key`` if omitted.
            grpc_url: the ingest endpoint. Resolved from ``WYRD_GRPC_URL`` if
                omitted.
            client: an existing ``WyrdClient``, such as one returned by
                ``on_behalf_of``. Bifrost then uses its authentication and
                transport; it cannot be combined with ``server_url``,
                ``credential``, or ``grpc_url``.
            client_byte_limit_bytes: the handle-wide ingestion byte budget
                shared by every table this client writes. 256 MiB if omitted.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` when ``client`` is combined
                with a transport argument; ``WYRD_CLIENT_401_NO_CREDENTIALS``
                when the credential chain yielded nothing;
                ``WYRD_CLIENT_400_CONFIG_INVALID`` when the byte budget is too
                small to seal one message.

        """
        ...
    def use_table(self, table: TableConfig) -> TableConfig | None:
        """Bind ``table`` as the write target, returning the previous binding.

        The previous table's producer stays pooled, so its buffered rows still
        flush; a swap loses nothing.
        """
        ...
    @property
    def table(self) -> TableConfig | None:
        """The active write binding, if any."""
        ...
    def insert(self, row: Any, correlation: Correlation | None = None) -> None:
        """Enqueue one row into the active table.

        Non-blocking; the row is durable after ``flush()``. A saturated queue
        refuses here rather than dropping silently.

        Args:
            row: a Pydantic model instance, a mapping, or a JSON object string
                holding the table's declared columns.
            correlation: optional ``card_ref`` and ``run_id`` for this row.
                Omitted keys are stored as null.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unparsable
                ``card_ref``; ``WYRD_VALA_412_NO_ACTIVE_TABLE`` when no table
                is bound; ``WYRD_CLIENT_429_QUEUE_FULL`` when the producer
                queue is full.

        """
        ...
    @property
    def dropped(self) -> int:
        """Rows dropped by the fire-and-forget ``wyrd.observe.record`` path."""
        ...
    @property
    def producer_count(self) -> int:
        """Number of distinct table producers currently pooled."""
        ...

class Bifrost(_BifrostBase):
    """Query any authorized table; write to the active one.

    Synchronous. Use ``AsyncBifrost`` for the ``await`` surface — the same
    methods on the same native client, differing only in who drives them.
    """

    def register(self) -> str:
        """Create the active table; returns ``"created"`` or ``"already_exists"``.

        Registering again with the same schema and settings is idempotent.

        Raises:
            WyrdError: ``WYRD_VALA_412_NO_ACTIVE_TABLE`` when no table is
                bound; ``WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT`` or
                ``WYRD_VALA_400_BIFROST_INVALID_COMPACTION_TARGET`` for a bad
                layout or file target; ``WYRD_VALA_409_BIFROST_*_MISMATCH``
                when the table exists with different columns, layout, or
                compaction settings.

        """
        ...
    def use_table_by_name(self, table: str) -> None:
        """Bind the registered ``"<namespace>.<name>"`` table, describing it first.

        Raises:
            WyrdError: as for ``TableConfig.describe()``.

        """
        ...
    def write_batch(self, table: str, batch: pyarrow.RecordBatch) -> None:
        """Write one already-built Arrow batch to ``table`` and await durability.

        The precision write door, beside ``insert``: it names its destination
        instead of using the active binding, carries correlation as ordinary
        columns, and is durable when it returns, so no ``flush`` follows it.
        Build the batch against ``TableConfig.describe(...).arrow_schema``; a
        canonical table compares an incoming block against its declared fields
        exactly, metadata included.

        Raises:
            WyrdError: the batch exceeded the byte envelope, or the server
                refused it; the stable ``code`` says which.

        """
        ...
    def flush(self) -> None:
        """Flush every pooled producer and await each durable acknowledgement.

        Raises:
            WyrdError: the first producer or server failure, after every
                producer has been attempted.

        """
        ...
    def shutdown(self) -> None:
        """Drain every producer and stop its background task.

        Raises:
            WyrdError: as for ``flush()``.

        """
        ...
    @overload
    def sql(self, query: str) -> QueryResult:
        """Run one SQL SELECT over any authorized table and collect every batch.

        Args:
            query: one SELECT statement; tables are named
                ``namespace.name``.
            model: optional row model such as a Pydantic class. Given, every
                row is validated locally into it and a list of instances is
                returned instead of a ``QueryResult``. It describes the
                selected columns and never reaches the server.

        Raises:
            WyrdError: invalid or refused SQL, an authorization or transport
                failure, or ``WYRD_VALA_502_QUERY_STREAM_INCOMPLETE`` when
                the response ends without its terminal frame.
            pydantic.ValidationError: a row did not fit ``model``. Nothing
                partially converted is returned.

        """
        ...
    @overload
    def sql(self, query: str, model: type[_Row]) -> list[_Row]: ...
    def stream(
        self,
        query: str,
        *,
        deadline_ms: int | None = None,
    ) -> BifrostBatchIterator:
        """Run one SQL SELECT and iterate its batches as they arrive.

        Args:
            query: one SELECT statement.
            deadline_ms: the query deadline, from 1 to 4294967295
                milliseconds. Omitted, the server's default deadline applies.

        Raises:
            WyrdError: an invalid request or a transport failure when the
                query starts; later failures raise during iteration.

        """
        ...
    def running(self) -> list[RunningQuery]:
        """List the authenticated tenant's active queries."""
        ...
    def status(self, request_id: str) -> RunningQuery:
        """Return one active query by the ``request_id`` its stream reported.

        Raises:
            WyrdError: a malformed ID, or the server's refusal or a
                transport failure.

        """
        ...
    def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream.

        Repeating the request is harmless. Raises as ``status()`` does.
        """
        ...
    def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe the registered ``namespace.name`` table's stored schema and layout.

        Raises:
            WyrdError: ``WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND`` or an
                authorization or transport failure.

        """
        ...

class AsyncBifrost(_BifrostBase):
    """The ``await`` surface over the same native client.

    Every blocking native call runs in a worker thread, so the event loop is
    never blocked. ``insert`` and ``use_table`` stay synchronous because they
    perform no IO. Each method behaves as its ``Bifrost`` counterpart.
    """

    async def register(self) -> str:
        """Create the active table; returns ``"created"`` or ``"already_exists"``."""
        ...
    async def use_table_by_name(self, table: str) -> None:
        """Bind the registered ``"<namespace>.<name>"`` table, describing it first."""
        ...
    async def write_batch(self, table: str, batch: pyarrow.RecordBatch) -> None:
        """Write one already-built Arrow batch to ``table`` and await durability."""
        ...
    async def flush(self) -> None:
        """Flush every pooled producer and await each durable acknowledgement."""
        ...
    async def shutdown(self) -> None:
        """Drain every producer and stop its background task."""
        ...
    @overload
    async def sql(self, query: str) -> QueryResult:
        """Run one SQL SELECT and collect every batch; ``model`` validates rows locally."""
        ...
    @overload
    async def sql(self, query: str, model: type[_Row]) -> list[_Row]: ...
    async def stream(
        self,
        query: str,
        *,
        deadline_ms: int | None = None,
    ) -> BifrostQueryStream:
        """Start one query and iterate its batches with ``async for``.

        ``deadline_ms`` is 1 to 4294967295 milliseconds; omitted, the server's
        default deadline applies.
        """
        ...
    async def running(self) -> list[RunningQuery]:
        """List the authenticated tenant's active queries."""
        ...
    async def status(self, request_id: str) -> RunningQuery:
        """Return one active query by its ``request_id``."""
        ...
    async def cancel(self, request_id: str) -> CancelRunningQueryResult:
        """Request server-side cancellation without closing a local stream."""
        ...
    async def describe_table(self, namespace: str, name: str) -> TableDescription:
        """Describe the registered ``namespace.name`` table's stored schema and layout."""
        ...

__all__ = [
    "AsyncBifrost",
    "Bifrost",
    "BifrostBatchIterator",
    "BifrostQueryStream",
    "CancelRunningQueryResult",
    "Correlation",
    "DataTypeSpec",
    "DataTypeSpecVariants",
    "FieldSpec",
    "PhysicalLayout",
    "QueryResult",
    "ResolvedTable",
    "RowModel",
    "RunningQuery",
    "RunningQueryProgress",
    "SortKey",
    "TableConfig",
    "TableDescription",
    "TableEntry",
]
