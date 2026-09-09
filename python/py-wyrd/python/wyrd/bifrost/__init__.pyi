# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
from collections.abc import AsyncIterator, Iterator
from typing import Any, TypedDict

import pyarrow

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

class NoCredentialsError(BifrostQueryError):
    """Raised when the credential chain yields nothing for an omitted credential."""

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

class ResolvedTable(TypedDict):
    """The server-minted identity of a registered table."""

    table_uid: str
    fingerprint: str

class Correlation(TypedDict, total=False):
    """Optional per-row correlation; an omitted key is a null on the wire."""

    card_ref: str
    run_id: str

class TableConfig:
    """One Bifrost table: its name, declared columns, and physical layout.

    The schema comes from a Pydantic model class through ``model_json_schema()``
    and is mapped by the same Rust owner every language uses. ``resolved`` stays
    ``None`` until ``Bifrost.register()`` or ``TableConfig.describe()``.
    """

    def __init__(
        self,
        model: type[Any],
        table: str,
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
    ) -> None: ...
    @staticmethod
    def from_arrow(
        schema: pyarrow.Schema,
        table: str,
        partition_granularity: str | None = None,
        sort_keys: list[SortKey] | None = None,
        bloom_columns: list[str] | None = None,
    ) -> TableConfig:
        """Build from an explicit Arrow schema, for types JSON Schema cannot express."""
        ...
    @staticmethod
    def describe(
        table: str,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> TableConfig:
        """Fetch an already-registered table's config by name."""
        ...
    @property
    def fqn(self) -> str:
        """The ``namespace.name`` this config addresses."""
        ...
    @property
    def arrow_schema(self) -> pyarrow.Schema:
        """The declared user columns only."""
        ...
    @property
    def resolved(self) -> ResolvedTable | None:
        """The server-assigned identity, or ``None`` while unregistered."""
        ...

class QueryResult:
    """Arrow batches from one query, converted on demand."""

    def to_arrow(self) -> pyarrow.Table: ...
    def to_polars(self) -> Any: ...
    def to_pandas(self) -> Any: ...
    def to_bytes(self) -> bytes: ...
    @property
    def terminal(self) -> dict[str, Any]:
        """The validated terminal frame the server closed the stream with."""
        ...
    def __len__(self) -> int: ...

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
    """Construction and the two non-IO operations both facades share."""

    def __init__(
        self,
        table: TableConfig | None = None,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> None:
        """Connect one client; every argument resolves from the chain if omitted."""
        ...
    def use_table(self, table: TableConfig) -> TableConfig | None:
        """Bind ``table`` as the write target, returning the previous binding."""
        ...
    @property
    def table(self) -> TableConfig | None:
        """The active write binding, if any."""
        ...
    def insert(self, row: Any, correlation: Correlation | None = None) -> None:
        """Enqueue one row into the active table; durable after ``flush()``."""
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
    """Query any authorized table; write to the active one. Synchronous."""

    def register(self) -> str: ...
    def use_table_by_name(self, table: str) -> None: ...
    def flush(self) -> None: ...
    def shutdown(self) -> None: ...
    def sql(self, query: str) -> QueryResult: ...
    def stream(
        self,
        query: str,
        *,
        visibility: str = "published_only",
        freshness: str = "strict",
        deadline_ms: int | None = None,
    ) -> BifrostBatchIterator: ...
    def running(self) -> list[RunningQuery]: ...
    def status(self, request_id: str) -> RunningQuery: ...
    def cancel(self, request_id: str) -> CancelRunningQueryResult: ...
    def describe_table(self, namespace: str, name: str) -> TableDescription: ...

class AsyncBifrost(_BifrostBase):
    """The ``await`` surface over the same native client."""

    async def register(self) -> str: ...
    async def use_table_by_name(self, table: str) -> None: ...
    async def flush(self) -> None: ...
    async def shutdown(self) -> None: ...
    async def sql(self, query: str) -> QueryResult: ...
    async def stream(
        self,
        query: str,
        *,
        visibility: str = "published_only",
        freshness: str = "strict",
        deadline_ms: int | None = None,
    ) -> BifrostQueryStream: ...
    async def running(self) -> list[RunningQuery]: ...
    async def status(self, request_id: str) -> RunningQuery: ...
    async def cancel(self, request_id: str) -> CancelRunningQueryResult: ...
    async def describe_table(self, namespace: str, name: str) -> TableDescription: ...

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
