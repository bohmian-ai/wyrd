from typing import Literal

from ._client import (
    AsyncBifrost,
    Bifrost,
    BifrostBatchIterator,
    BifrostQueryStream,
    CancelRunningQueryResult,
    Correlation,
    DataTypeSpec,
    DataTypeSpecVariants,
    FieldSpec,
    InstrumentationScope,
    PhysicalLayout,
    QueryParam,
    QueryResult,
    ResolvedTable,
    Resource,
    RowModel,
    RunningQuery,
    RunningQueryProgress,
    SortKey,
    SpanEvent,
    SpanLink,
    SpanRecord,
    SpanStatus,
    TableConfig,
    TableDescription,
    TableEntry,
)

class QueryTerminal:
    """The validated terminal frame one query stream closed with.

    Closed wire enums read back as their snake_case wire names.
    """

    @property
    def outcome(self) -> Literal["success", "degraded", "failed"]:
        """Stream outcome."""
        ...
    @property
    def query_class(self) -> Literal["interactive", "analytical"]:
        """Path Oracle ran the query on."""
        ...
    @property
    def row_count(self) -> int:
        """Rows already emitted in batch frames."""
        ...
    @property
    def warnings(self) -> list[Literal["live_tail_unavailable", "stale_cut_replanned"]]:
        """Closed warnings the server attached."""
        ...
    @property
    def source_completion(self) -> dict[str, str]:
        """Completion outcome per source tier present in the cut."""
        ...
    @property
    def error_code(self) -> str | None:
        """Stable failure code for a failed terminal, otherwise ``None``."""
        ...
    @property
    def error_detail(self) -> str | None:
        """Scrubbed failure diagnostic for a failed terminal, when sent."""
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
    "InstrumentationScope",
    "PhysicalLayout",
    "QueryParam",
    "QueryResult",
    "QueryTerminal",
    "ResolvedTable",
    "Resource",
    "RowModel",
    "RunningQuery",
    "RunningQueryProgress",
    "SortKey",
    "SpanEvent",
    "SpanLink",
    "SpanRecord",
    "SpanStatus",
    "TableConfig",
    "TableDescription",
    "TableEntry",
]
