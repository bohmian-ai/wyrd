"""Type fixture pinning the Bifrost describe/trace/GenAI shapes as real types.

`ty` checks this module in the `py:typecheck` lane, so every access below is a
static assertion that the public TypedDict graph reaches the leaf without an
`Any` hop or a cast. The runtime asserts keep it a normal unit test too.
"""

from __future__ import annotations

from wyrd.bifrost import (
    Correlation,
    DataTypeSpecVariants,
    FieldSpec,
    GenAiPage,
    ResolvedTable,
    SortKey,
    TableDescription,
    TraceDetail,
)


def test_described_field_types_are_recursive_without_any() -> None:
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
    assert description["physical_layout"]["sort_keys"][0]["column"] == "trace_id"


def test_trace_and_genai_payloads_reach_their_leaves() -> None:
    detail: TraceDetail = {
        "trace": {
            "trace_id": "0102",
            "spans": [
                {
                    "span_id": "aabb",
                    "trace_state": "",
                    "flags": 1,
                    "name": "chat",
                    "kind": 3,
                    "start_time_unix_nano": 1,
                    "end_time_unix_nano": 2,
                    "duration_nano": 1,
                    "dropped_attributes_count": 0,
                    "events": [
                        {
                            "time_unix_nano": 1,
                            "name": "retry",
                            "attributes": {"attempt": 1},
                            "dropped_attributes_count": 0,
                        }
                    ],
                    "dropped_events_count": 0,
                    "links": [
                        {
                            "linked_trace_id": "0304",
                            "linked_span_id": "ccdd",
                            "trace_state": "",
                            "flags": 0,
                            "dropped_attributes_count": 0,
                        }
                    ],
                    "dropped_links_count": 0,
                    "resource_dropped_attributes_count": 0,
                    "resource_schema_url": "",
                    "scope_name": "wyrd",
                    "scope_version": "1",
                    "scope_dropped_attributes_count": 0,
                    "scope_schema_url": "",
                }
            ],
        }
    }
    page: GenAiPage = {
        "rows": [
            {
                "start_time_unix_nano": 1,
                "model": "claude",
                "input_messages": [{"role": "user"}],
            }
        ],
        "next_page_token": "cursor",
    }

    span = detail["trace"]["spans"][0]
    assert span["events"][0]["name"] == "retry"
    assert span["links"][0]["linked_span_id"] == "ccdd"
    assert page["rows"][0]["model"] == "claude"
    assert page["rows"][0]["input_messages"][0]["role"] == "user"
    assert page["next_page_token"] == "cursor"


def test_client_value_types_are_declared_without_any() -> None:
    """The three values a caller constructs or reads back are real types.

    `Correlation` is what an `insert` takes, `ResolvedTable` is what `register`
    resolves, and `SortKey` is what a `TableConfig` layout declares. Each is
    checked statically here so a caller's editor rejects a wrong key before the
    server does.
    """

    correlation: Correlation = {"card_ref": "test/Service/writer@1.0.0"}
    resolved: ResolvedTable = {"table_uid": "01J0", "fingerprint": "fp"}
    key: SortKey = {"column": "wyrd_event_time", "direction": "desc", "null_order": "last"}

    assert correlation["card_ref"].endswith("@1.0.0")
    assert "run_id" not in correlation
    assert resolved["table_uid"] == "01J0"
    assert key["column"] == "wyrd_event_time"
