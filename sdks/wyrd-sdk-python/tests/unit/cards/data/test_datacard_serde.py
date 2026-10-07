"""A DataCard's JSON envelope round-trips and carries user metadata."""

import json

import numpy as np
import pandas as pd
import pyarrow as pa
import pytest
from wyrd.data import (
    ArrowInterface,
    DataCard,
    DataInterface,
    JsonlInterface,
    NumpyInterface,
    PandasInterface,
    SqlInterface,
    WyrdError,
)


@pytest.mark.parametrize(
    "interface",
    [
        pytest.param(PandasInterface(data=pd.DataFrame({"x": [1]})), id="pandas"),
        pytest.param(
            ArrowInterface(data=pa.table({"x": pa.array([1], type=pa.int64())})), id="arrow"
        ),
        pytest.param(NumpyInterface(data=np.array([1], dtype=np.int64)), id="numpy"),
        pytest.param(JsonlInterface(data=[{"x": 1}]), id="jsonl"),
        pytest.param(
            SqlInterface(data={"queries": {"main": "select 1"}}, dialect="duckdb"), id="sql"
        ),
    ],
)
def test_json_envelope_round_trips_the_interface_kind(interface: DataInterface) -> None:
    card = DataCard(interface)

    assert (
        DataCard.model_validate_json(card.model_dump_json()).interface.kind == card.interface.kind
    )


def test_model_validate_json_rehydrates_interface_from_metadata() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))
    restored = DataCard.model_validate_json(card.model_dump_json())

    assert restored.interface.kind == "Pandas"
    assert restored.interface.has_source is False


def test_json_envelope_carries_interface_metadata_without_the_data() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))

    assert "data" not in card.metadata.to_dict()["interface"]["meta"]


def test_datacard_str_is_pretty_card_json() -> None:
    payload = json.loads(str(DataCard(JsonlInterface(data=[{"x": 1}]))))

    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Data"


def test_labels_and_annotations_survive_the_json_round_trip() -> None:
    card = DataCard(
        PandasInterface(data=pd.DataFrame({"x": [1]})),
        labels={"domain": "churn"},
        annotations={"acme.com/source": "warehouse.customer_churn"},
    )
    restored = DataCard.model_validate_json(card.model_dump_json())

    assert restored.labels == {"domain": "churn"}
    assert restored.annotations == {"acme.com/source": "warehouse.customer_churn"}


@pytest.mark.parametrize(
    "user_metadata",
    [
        pytest.param({"labels": {"bad key": "value"}}, id="malformed-label-key"),
        pytest.param({"labels": {"wyrd.io/readme": "value"}}, id="reserved-label-prefix"),
        pytest.param({"annotations": {"acme.com/token": "value"}}, id="secret-annotation-key"),
        pytest.param(
            {"annotations": {"acme.com/source": "sk-secret"}}, id="secret-annotation-value"
        ),
    ],
)
def test_invalid_user_metadata_is_refused(user_metadata: dict[str, dict[str, str]]) -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(
            PandasInterface(data=pd.DataFrame({"x": [1]})),
            labels=user_metadata.get("labels"),
            annotations=user_metadata.get("annotations"),
        )

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"
