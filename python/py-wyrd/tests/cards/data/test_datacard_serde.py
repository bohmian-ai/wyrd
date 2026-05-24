from __future__ import annotations

import json

import numpy as np
import pandas as pd
import pyarrow as pa
import pytest
from wyrd.data import (
    ArrowInterface,
    DataCard,
    JsonlInterface,
    NumpyInterface,
    PandasInterface,
    SqlInterface,
    WyrdError,
)


def test_model_dump_json_round_trips_through_rust_for_all_interfaces(tmp_path) -> None:
    cards = [
        DataCard(PandasInterface(data=pd.DataFrame({"x": [1]}))),
        DataCard(ArrowInterface(data=pa.table({"x": pa.array([1], type=pa.int64())}))),
        DataCard(NumpyInterface(data=np.array([1], dtype=np.int64))),
        DataCard(JsonlInterface(data=[{"x": 1}])),
        DataCard(SqlInterface(data={"queries": {"main": "select 1"}}, dialect="duckdb")),
    ]

    for card in cards:
        restored = DataCard.model_validate_json(card.model_dump_json())
        assert restored.interface.kind == card.interface.kind


@pytest.mark.wyrd_covers("python:DataCard.model_validate_json")
def test_model_validate_json_rehydrates_interface_from_metadata(tmp_path) -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))
    restored = DataCard.model_validate_json(card.model_dump_json())

    assert restored.interface.kind == "Pandas"
    assert restored.interface.has_source is False


@pytest.mark.wyrd_covers("python:DataCard.model_dump_json")
def test_serialized_datacard_contains_spec_interface_metadata_not_python_state(tmp_path) -> None:
    payload = json.loads(DataCard(PandasInterface(data=pd.DataFrame({"x": [1]}))).model_dump_json())

    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Data"
    assert "data" not in payload["spec"]["interface"]["meta"]


def test_labels_annotations_replace_tags_in_metadata() -> None:
    card = DataCard(
        PandasInterface(data=pd.DataFrame({"x": [1]})),
        labels={"domain": "churn"},
        annotations={"acme.com/source": "warehouse.customer_churn"},
    )
    payload = json.loads(card.model_dump_json())
    restored = DataCard.model_validate_json(card.model_dump_json())

    assert payload["metadata"]["labels"] == {"domain": "churn"}
    assert payload["metadata"]["annotations"] == {"acme.com/source": "warehouse.customer_churn"}
    assert "tags" not in payload["metadata"]
    assert restored.labels == {"domain": "churn"}
    assert restored.annotations == {"acme.com/source": "warehouse.customer_churn"}
    with pytest.raises(TypeError):
        DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})), tags=["old"])


def test_user_metadata_rejects_invalid_reserved_and_secret_values() -> None:
    cases = [
        {"labels": {"bad key": "value"}},
        {"labels": {"wyrd.io/readme": "value"}},
        {"annotations": {"acme.com/token": "value"}},
        {"annotations": {"acme.com/source": "sk-secret"}},
    ]

    for kwargs in cases:
        with pytest.raises(WyrdError) as exc:
            DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})), **kwargs)
        assert exc.value.code == "WYRD_DATA_400_VALIDATION"
