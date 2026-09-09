# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
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
