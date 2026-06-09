from __future__ import annotations

import json

import numpy as np
import pandas as pd
import polars as pl
import pyarrow as pa
import pyarrow.parquet as pq
import pytest
import torch
from datasets import Dataset
from wyrd.data import (
    ArrowInterface,
    DataCard,
    DataInterface,
    HuggingfaceInterface,
    ImageInterface,
    JsonlInterface,
    NumpyInterface,
    PandasInterface,
    ParquetInterface,
    PolarsInterface,
    SqlInterface,
    TextInterface,
    TorchInterface,
    WyrdError,
)
from wyrd.cards import CardRef, Kind


def _payload(card: DataCard) -> dict:
    return json.loads(card.model_dump_json())


def test_pandas_interface_captures_inferred_schema_and_version(tmp_path) -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"year": [2024], "churn": [False]})))
    payload = _payload(card)

    assert payload["spec"]["interface"]["kind"] == "Pandas"
    assert payload["spec"]["schema"]["columns"][0]["name"] == "year"
    assert payload["spec"]["interface"]["meta"]["framework_version"] != "unknown"


def test_polars_interface_captures_inferred_schema_and_version(tmp_path) -> None:
    card = DataCard(PolarsInterface(data=pl.DataFrame({"year": [2024], "churn": [False]})))
    payload = _payload(card)

    assert payload["spec"]["interface"]["kind"] == "Polars"
    assert payload["spec"]["schema"]["columns"][1]["dtype"] == "bool"
    assert payload["spec"]["interface"]["meta"]["framework_version"] != "unknown"


def test_pyarrow_interface_captures_inferred_schema_and_version(tmp_path) -> None:
    table = pa.table({"year": pa.array([2024], type=pa.int64())})
    payload = _payload(DataCard(ArrowInterface(data=table)))

    assert payload["spec"]["interface"]["kind"] == "Arrow"
    assert payload["spec"]["schema"]["columns"][0]["dtype"] == "int64"
    assert payload["spec"]["interface"]["meta"]["framework_version"] != "unknown"


def test_numpy_interface_captures_shape_dtype_and_empty_schema(tmp_path) -> None:
    payload = _payload(DataCard(NumpyInterface(data=np.array([[1, 2]], dtype=np.int64))))

    assert payload["spec"]["interface"]["kind"] == "Numpy"
    assert payload["spec"]["interface"]["meta"]["shape"] == [1, 2]
    assert payload["spec"]["schema"]["columns"][0]["dtype"] == "int64"


def test_torch_interface_captures_shape_dtype_when_installed(tmp_path) -> None:
    card = DataCard(TorchInterface(data=torch.tensor([[1, 2]], dtype=torch.int64)))

    assert card.interface.to_dict()["kind"] == "Torch"
    assert card.schema.columns[0].dtype == "int64"


def test_huggingface_interface_captures_dataset_metadata_when_installed(tmp_path) -> None:
    dataset = Dataset.from_dict({"text": ["a"], "label": [1]})
    payload = _payload(DataCard(HuggingfaceInterface(data=dataset, dataset_id="local/test")))

    assert payload["spec"]["interface"]["kind"] == "Huggingface"
    assert payload["spec"]["interface"]["meta"]["dataset_id"] == "local/test"


def test_sql_interface_captures_queries() -> None:
    payload = _payload(
        DataCard(SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"))
    )

    assert payload["spec"]["interface"]["kind"] == "Sql"
    assert payload["spec"]["sql"]["queries"] == {"train": "select 1"}


def test_parquet_interface_uses_path_metadata(tmp_path) -> None:
    source = tmp_path / "rows.parquet"
    pq.write_table(pa.table({"year": pa.array([2024], type=pa.int64())}), source)
    payload = _payload(DataCard(ParquetInterface(data=source)))

    assert payload["spec"]["interface"]["kind"] == "Parquet"
    assert payload["spec"]["schema"]["columns"][0]["name"] == "year"


def test_jsonl_interface_uses_path_metadata(tmp_path) -> None:
    source = tmp_path / "rows.jsonl"
    source.write_text('{"x": 1}\n')

    assert DataCard(JsonlInterface(data=source)).interface.kind == "Jsonl"


def test_image_interface_uses_directory_manifest_metadata(tmp_path) -> None:
    source = tmp_path / "images"
    source.mkdir()
    (source / "a.png").write_bytes(b"png")

    assert DataCard(ImageInterface(data=source)).interface.kind == "Image"


def test_text_interface_uses_directory_manifest_metadata(tmp_path) -> None:
    source = tmp_path / "text"
    source.mkdir()
    (source / "a.txt").write_text("hello")

    assert DataCard(TextInterface(data=source)).interface.kind == "Text"


def test_datacard_rejects_non_data_interface() -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(object())

    assert exc.value.code == "WYRD_DATA_400_UNKNOWN_DATA_TYPE"


def test_pandas_interface_maps_to_rust_pandas_meta() -> None:
    assert PandasInterface(compression="zstd").to_dict()["meta"]["compression"] == "Zstd"


def test_polars_interface_maps_to_rust_polars_meta() -> None:
    assert PolarsInterface(compression="gzip").to_dict()["meta"]["compression"] == "Gzip"


def test_arrow_interface_maps_to_rust_arrow_meta() -> None:
    assert ArrowInterface(format="ipc").to_dict()["meta"]["format"] == "Ipc"


def test_parquet_interface_maps_to_rust_parquet_meta() -> None:
    assert ParquetInterface(row_group_size=128).to_dict()["meta"]["row_group_size"] == 128


def test_numpy_interface_maps_to_rust_numpy_meta() -> None:
    assert NumpyInterface(dtype="int64", shape=[2, 2]).to_dict()["meta"]["shape"] == [2, 2]


def test_torch_interface_maps_to_rust_torch_meta_when_installed() -> None:
    assert TorchInterface(save_format="pickle").to_dict()["meta"]["save_format"] == "Pickle"


def test_sql_interface_maps_to_rust_sql_meta() -> None:
    assert (
        SqlInterface(data={"queries": {"q": "select 1"}}, dialect="duckdb").to_dict()["meta"][
            "dialect"
        ]
        == "duckdb"
    )


def test_jsonl_interface_maps_to_rust_jsonl_meta() -> None:
    assert JsonlInterface(compression="gzip").to_dict()["meta"]["compression"] == "Gzip"


def test_image_interface_maps_to_rust_image_meta() -> None:
    assert ImageInterface(format="png", color_mode="rgba").to_dict()["meta"]["color_mode"] == "Rgba"


def test_text_interface_maps_to_rust_text_meta() -> None:
    assert TextInterface(encoding="utf-16").to_dict()["meta"]["encoding"] == "utf-16"


def test_huggingface_interface_maps_to_rust_huggingface_meta_when_installed() -> None:
    assert (
        HuggingfaceInterface(dataset_id="local/test").to_dict()["meta"]["dataset_id"]
        == "local/test"
    )


def test_custom_interface_maps_to_rust_custom_meta() -> None:
    class MyInterface(DataInterface):
        def __init__(self):
            super().__init__()
            self.data = {"x": 1}

    payload = _payload(DataCard(MyInterface()))

    assert payload["spec"]["interface"]["kind"] == "Custom"


def test_interface_metadata_contains_no_live_python_objects() -> None:
    payload = _payload(DataCard(PandasInterface(data=pd.DataFrame({"x": [1]}))))

    assert "data" not in payload["spec"]["interface"]["meta"]


def test_datacard_to_data_spec_uses_rust_interface_metadata() -> None:
    payload = _payload(DataCard(PolarsInterface(data=pl.DataFrame({"x": [1]}), compression="gzip")))

    assert payload["spec"]["interface"]["meta"]["compression"] == "Gzip"


def test_datacard_to_card_body_returns_data_variant() -> None:
    assert _payload(DataCard(PandasInterface(data=pd.DataFrame({"x": [1]}))))["kind"] == "Data"


def test_artifact_card_input_requires_or_infers_interface_metadata(tmp_path) -> None:
    artifact = CardRef(kind=Kind.Artifact, name="existing-data", version="1.0.0")
    card = DataCard(artifact)
    metadata = card.metadata.to_dict()

    assert metadata["artifact_refs"][0]["kind"] == "Artifact"
    assert metadata["artifact_refs"][0]["name"] == "existing-data"


def test_artifact_card_input_rejects_non_artifact_card_ref() -> None:
    data_ref = CardRef(kind=Kind.Data, name="existing-data", version="1.0.0")

    with pytest.raises(WyrdError) as exc:
        DataCard(data_ref)

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"
    assert exc.value.details["expected_kind"] == "Artifact"
    assert exc.value.details["actual_kind"] == "Data"


def test_set_interface_replaces_spec_metadata_and_schema() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"year": [2024]})))
    assert card.interface.kind == "Pandas"
    assert any(c.name == "year" for c in card.schema.columns)

    card.interface = ArrowInterface(data=pa.table({"score": pa.array([1], type=pa.int64())}))

    assert card.interface.kind == "Arrow"
    assert any(c.name == "score" for c in card.schema.columns)
    assert _payload(card)["spec"]["interface"]["kind"] == "Arrow"
