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
        "user_fields": [attributes],
        "correlation_fields": [],
        "managed_candidates": [],
        "canonical_physical_fingerprint": "canonical-fp",
        "compaction_target_file_size_bytes": 1_073_741_824,
        "compaction_type": "small-files",
        "physical_layout": {
            "partition_granularity": "Hour",
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


def client_value_types_are_declared_without_any() -> None:
    """The three values a caller constructs or reads back are real types.

    `Correlation` is what an `insert` takes, `ResolvedTable` is what `register`
    resolves, and `SortKey` is what a `TableConfig` layout declares. Each is
    checked statically here so a caller's editor rejects a wrong key before the
    server does.
    """

    correlation: Correlation = {"card_uid": "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"}
    resolved: ResolvedTable = {"table_uid": "01J0", "fingerprint": "fp"}
    key: SortKey = {"column": "wyrd_event_time", "direction": "desc", "null_order": "last"}

    assert correlation["card_uid"].endswith("1b01")
    assert "run_id" not in correlation
    assert resolved["table_uid"] == "01J0"
    assert key["column"] == "wyrd_event_time"
