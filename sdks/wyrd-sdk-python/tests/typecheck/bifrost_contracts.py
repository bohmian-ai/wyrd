"""Type fixture pinning the Bifrost describe/trace/GenAI shapes as real types.

`ty` checks this module in the `py:typecheck` lane, so every access below is a
static assertion that the public TypedDict graph reaches the leaf without an
`Any` hop or a cast. It sits outside pytest collection.
"""

from __future__ import annotations

from wyrd.bifrost import (
    Correlation,
    DataTypeSpecVariants,
    FieldSpec,
    ResolvedTable,
    SortKey,
    SpanRecord,
    SpanStatus,
    TableDescription,
)


def described_field_types_are_recursive_without_any() -> None:
    struct: DataTypeSpecVariants = {
        "Struct": [{"name": "inner", "data_type": "Utf8", "nullable": True}]
    }
    attributes: FieldSpec = {
        "name": "attributes",
        "data_type": {"List": {"name": "item", "data_type": struct, "nullable": False}},
        "nullable": True,
        "metadata": {"PARQUET:field_id": "7"},
    }
    description: TableDescription = {
        "entry": {
            "namespace": "vala.traces",
            "name": "spans",
            "table_uid": "01J0",
            "status": "Active",
            "fingerprint": "fp",
            "registered_at": "2026-07-01T00:00:00Z",
            "updated_at": "2026-07-01T00:00:00Z",
        },
        "table_version": "1",
        "user_fields": [attributes],
        "correlation_fields": [],
        "canonical_physical_fingerprint": "canonical-fp",
        "compaction_target_file_size_bytes": 1_073_741_824,
        "compaction_type": "small-files",
        "physical_layout": {
            "partition_by": [{"column": "start_time_unix_nano", "transform": "hour"}],
            "time_column": "start_time_unix_nano",
            "sort_keys": [{"column": "trace_id", "direction": "Ascending", "null_order": "Last"}],
            "bloom_columns": ["trace_id"],
        },
    }

    nested = description["user_fields"][0]["data_type"]
    assert not isinstance(nested, str)
    inner = nested["List"]["data_type"]
    assert not isinstance(inner, str)
    assert inner["Struct"][0]["name"] == "inner"
    assert description["user_fields"][0]["metadata"]["PARQUET:field_id"] == "7"
    assert description["canonical_physical_fingerprint"] == "canonical-fp"
    assert description["compaction_target_file_size_bytes"] == 1_073_741_824
    assert description["compaction_type"] == "small-files"
    assert description["physical_layout"]["sort_keys"][0]["column"] == "trace_id"
    assert description["physical_layout"]["time_column"] == "start_time_unix_nano"


def client_value_types_are_declared_without_any() -> None:
    """The three values a caller constructs or reads back are real types.

    `Correlation` is what an `insert` takes, `ResolvedTable` is what `register`
    resolves, and `SortKey` is what a `TableConfig` layout declares. Each is
    checked statically here so a caller's editor rejects a wrong key before the
    server does.
    """

    correlation: Correlation = {"card_uid": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"}
    resolved: ResolvedTable = {"table_uid": "01J0", "fingerprint": "fp"}
    key: SortKey = {"column": "created_at", "direction": "desc", "null_order": "last"}

    assert correlation["card_uid"].endswith("1b01")
    assert "run_id" not in correlation
    assert resolved["table_uid"] == "01J0"
    assert key["column"] == "created_at"


def span_records_require_every_always_emitted_key() -> None:
    record: SpanRecord = {
        "trace_id": "0af7651916cd43dd8448eb211c80319c",
        "span_id": "b7ad6b7169203331",
        "parent_span_id": "00f067aa0ba902b7",
        "flags": 1,
        "trace_state": "",
        "name": "request",
        "kind": "server",
        "start_time": "2026-07-01T00:00:00Z",
        "end_time": "2026-07-01T00:00:01Z",
        "duration_ms": 1000,
        "status": {"code": "error", "description": "timed out"},
        "attributes": {},
        "dropped_attributes_count": 0,
        "events": [
            {
                "timestamp": "2026-07-01T00:00:00Z",
                "name": "log",
                "attributes": {},
                "dropped_attributes_count": 0,
            }
        ],
        "dropped_events_count": 0,
        "links": [
            {
                "trace_id": "0af7651916cd43dd8448eb211c80319c",
                "span_id": "00f067aa0ba902b7",
                "trace_state": "",
                "flags": 1,
                "attributes": {},
                "dropped_attributes_count": 0,
            }
        ],
        "dropped_links_count": 0,
        "scope": {"name": "journey.service", "attributes": {}},
        "resource": {"service_name": "assistant", "attributes": {}},
    }
    assert record["links"][0]["flags"] == 1
    assert record["scope"]["name"] == "journey.service"

    unlinked: SpanRecord = {  # ty: ignore[missing-typed-dict-key]
        "trace_id": "0af7651916cd43dd8448eb211c80319c",
        "span_id": "b7ad6b7169203331",
        "flags": 1,
        "trace_state": "",
        "name": "request",
        "kind": "server",
        "start_time": "2026-07-01T00:00:00Z",
        "end_time": "2026-07-01T00:00:01Z",
        "duration_ms": 1000,
        "status": {"code": "unset"},
        "attributes": {},
        "dropped_attributes_count": 0,
        "events": [],
        "dropped_events_count": 0,
        "dropped_links_count": 0,
        "scope": {"name": "journey.service", "attributes": {}},
        "resource": {"service_name": "assistant", "attributes": {}},
    }
    assert unlinked["name"] == "request"


def only_an_error_status_carries_a_description() -> None:
    failed: SpanStatus = {"code": "error", "description": "timed out"}
    passed: SpanStatus = {"code": "ok", "description": "fine"}  # ty: ignore[invalid-assignment]
    unset: SpanStatus = {"code": "unset", "description": "none"}  # ty: ignore[invalid-assignment]
    assert [failed["code"], passed["code"], unset["code"]] == ["error", "ok", "unset"]
