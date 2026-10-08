"""Interface and schema inference when a DataCard is built from data."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pandas as pd
import polars as pl
import pyarrow as pa
import pyarrow.parquet as pq
import pytest
import torch
from datasets import Dataset
from wyrd.cards import CardKind, CardRef
from wyrd.data import (
    ArrowInterface,
    DataCard,
    DataInterface,
    HuggingfaceInterface,
    ImageInterface,
    NumpyInterface,
    PandasInterface,
    ParquetInterface,
    PolarsInterface,
    SqlInterface,
    TorchInterface,
    WyrdError,
)


@pytest.fixture
def rows_parquet(tmp_path: Path) -> Path:
    """A one-column Parquet file on local disk."""
    path = tmp_path / "rows.parquet"
    pq.write_table(pa.table({"year": pa.array([2024], type=pa.int64())}), path)
    return path


@pytest.mark.parametrize(
    ("interface", "kind", "first_column"),
    [
        pytest.param(
            PandasInterface(data=pd.DataFrame({"year": [2024]})),
            "Pandas",
            ("year", "int64"),
            id="pandas",
        ),
        pytest.param(
            PolarsInterface(data=pl.DataFrame({"churn": [False]})),
            "Polars",
            ("churn", "bool"),
            id="polars",
        ),
        pytest.param(
            ArrowInterface(data=pa.table({"year": pa.array([2024], type=pa.int64())})),
            "Arrow",
            ("year", "int64"),
            id="arrow",
        ),
        pytest.param(
            NumpyInterface(data=np.array([[1, 2]], dtype=np.int64)),
            "Numpy",
            ("value", "int64"),
            id="numpy",
        ),
        pytest.param(
            TorchInterface(data=torch.tensor([[1, 2]], dtype=torch.int64)),
            "Torch",
            ("value", "int64"),
            id="torch",
        ),
    ],
)
def test_datacard_infers_the_schema_of_its_data(
    interface: DataInterface, kind: str, first_column: tuple[str, str]
) -> None:
    card = DataCard(interface)
    column = card.schema.columns[0]

    assert card.interface.kind == kind
    assert (column.name, column.dtype) == first_column


def test_huggingface_interface_records_its_dataset_id() -> None:
    dataset = Dataset.from_dict({"text": ["a"], "label": [1]})

    card = DataCard(HuggingfaceInterface(data=dataset, dataset_id="local/test"))

    assert card.interface.dataset_id == "local/test"


def test_sql_interface_captures_queries() -> None:
    card = DataCard(SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"))

    assert (card.interface.kind, card.data["queries"]) == ("Sql", {"train": "select 1"})


def test_parquet_interface_reads_the_schema_from_the_file(rows_parquet: Path) -> None:
    card = DataCard(ParquetInterface(data=rows_parquet))

    assert card.schema.column_names() == ["year"]


def test_datacard_rejects_non_data_interface() -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(object())

    assert exc.value.code == "WYRD_DATA_400_UNKNOWN_DATA_TYPE"


def test_artifact_card_input_is_recorded_as_a_card_reference() -> None:
    artifact = CardRef(
        kind=CardKind.Artifact, name="existing-data", version="1.0.0", space="default"
    )

    card = DataCard(artifact)

    assert card.metadata.to_dict()["card_refs"] == [
        {"kind": "Artifact", "name": "existing-data", "version": "1.0.0", "space": "default"}
    ]


def test_artifact_card_input_rejects_non_artifact_card_ref() -> None:
    data_ref = CardRef(kind=CardKind.Data, name="existing-data", version="1.0.0", space="default")

    with pytest.raises(WyrdError) as exc:
        DataCard(data_ref)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"
    assert exc.value.details["expected_kind"] == "Artifact"
    assert exc.value.details["actual_kind"] == "Data"


def test_manifest_ref_allows_authored_ref_without_space() -> None:
    interface = ImageInterface(
        manifest_ref={"kind": "Artifact", "name": "manifest", "version": "1.0.0"}
    )

    assert interface.to_dict()["meta"]["manifest_ref"] == {
        "kind": "Artifact",
        "name": "manifest",
        "version": "1.0.0",
    }


def test_set_interface_replaces_spec_metadata_and_schema() -> None:
    card: DataCard[DataInterface] = DataCard(PandasInterface(data=pd.DataFrame({"year": [2024]})))

    card.interface = ArrowInterface(data=pa.table({"score": pa.array([1], type=pa.int64())}))

    assert (card.interface.kind, card.schema.column_names()) == ("Arrow", ["score"])
