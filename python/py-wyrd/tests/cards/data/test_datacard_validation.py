from __future__ import annotations

import json

import pandas as pd
import pytest
from wyrd.data import DataCard, HuggingfaceInterface, PandasInterface, Split, WyrdError


def _payload() -> dict:
    return json.loads(
        DataCard(
            PandasInterface(data=pd.DataFrame({"feature": [1], "target": [0]}))
        ).model_dump_json()
    )


def test_target_column_outside_schema_raises_target_column_unknown() -> None:
    payload = _payload()
    payload["spec"]["target_columns"] = ["missing"]

    with pytest.raises(WyrdError) as exc:
        DataCard.model_validate_json(json.dumps(payload))

    assert exc.value.code == "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN"


def test_empty_schema_for_pandas_rejected() -> None:
    payload = _payload()
    payload["spec"]["schema"] = {"columns": []}

    with pytest.raises(WyrdError):
        DataCard.model_validate_json(json.dumps(payload))


def test_duplicate_column_rejected() -> None:
    payload = _payload()
    payload["spec"]["schema"]["columns"].append(payload["spec"]["schema"]["columns"][0])

    with pytest.raises(WyrdError):
        DataCard.model_validate_json(json.dumps(payload))


def test_column_split_unknown_column_rejected() -> None:
    payload = _payload()
    payload["spec"]["splits"] = {
        "bad": {"label": "bad", "strategy": Split.column("missing", "==", 1).to_dict()}
    }

    with pytest.raises(WyrdError) as exc:
        DataCard.model_validate_json(json.dumps(payload))

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_bad_sha256_rejected_from_rust_validator() -> None:
    payload = _payload()
    payload["spec"]["stats"]["sha256"] = "bad"

    with pytest.raises(WyrdError):
        DataCard.model_validate_json(json.dumps(payload))


def test_zero_byte_count_rejected_from_rust_validator() -> None:
    payload = _payload()
    payload["spec"]["stats"]["byte_count"] = 0

    with pytest.raises(WyrdError):
        DataCard.model_validate_json(json.dumps(payload))


def test_sql_default_query_must_exist() -> None:
    payload = {
        "apiVersion": "wyrd/v1",
        "kind": "Data",
        "metadata": {"name": "sql", "version": "1.0.0"},
        "spec": {
            "interface": {"kind": "Sql", "meta": {"dialect": "duckdb"}},
            "schema": {"columns": []},
            "sql": {"queries": {"main": "select 1"}, "default_query": "missing"},
            "stats": {"byte_count": 1, "sha256": "a" * 64},
        },
    }

    with pytest.raises(WyrdError):
        DataCard.model_validate_json(json.dumps(payload))


def test_huggingface_revision_must_be_hex_7_to_40_chars() -> None:
    with pytest.raises(WyrdError):
        HuggingfaceInterface(dataset_id="local/test", revision="bad-rev")
