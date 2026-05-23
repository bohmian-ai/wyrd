from __future__ import annotations

import json

import numpy as np
import pandas as pd
import polars as pl
import pyarrow as pa
import pytest
from wyrd import DataCard, WyrdError
from wyrd.data import (
    ArrowInterface,
    CustomDataInterface,
    ImageInterface,
    JsonlInterface,
    NumpyInterface,
    PandasInterface,
    ParquetInterface,
    PolarsInterface,
    Split,
    SqlInterface,
    TextInterface,
)


def test_pandas_interface_captures_inferred_schema_and_version() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"year": [2024], "churn": [False]})))

    payload = json.loads(card.model_dump_json())

    assert payload["spec"]["interface"]["kind"] == "Pandas"
    assert payload["spec"]["schema"]["columns"][0]["name"] == "year"
    assert payload["spec"]["interface"]["meta"]["framework_version"] != "unknown"


def test_numpy_interface_captures_shape_dtype_and_schema() -> None:
    card = DataCard(NumpyInterface(data=np.array([[1, 2], [3, 4]], dtype="int64")))

    payload = json.loads(card.model_dump_json())

    assert payload["spec"]["interface"]["kind"] == "Numpy"
    assert payload["spec"]["interface"]["meta"]["shape"] == [2, 2]
    assert payload["spec"]["schema"]["columns"][0]["dtype"] == "int64"


def test_polars_interface_captures_real_schema_and_version() -> None:
    card = DataCard(PolarsInterface(data=pl.DataFrame({"year": [2024], "churn": [False]})))

    payload = json.loads(card.model_dump_json())

    assert payload["spec"]["interface"]["kind"] == "Polars"
    assert payload["spec"]["schema"]["columns"][0]["name"] == "year"
    assert payload["spec"]["interface"]["meta"]["framework_version"] != "unknown"


def test_pyarrow_interface_captures_real_schema_and_version() -> None:
    table = pa.table({"year": pa.array([2024], type=pa.int64()), "churn": [False]})
    card = DataCard(ArrowInterface(data=table))

    payload = json.loads(card.model_dump_json())

    assert payload["spec"]["interface"]["kind"] == "Arrow"
    assert payload["spec"]["schema"]["columns"][0]["dtype"] == "int64"
    assert payload["spec"]["interface"]["meta"]["framework_version"] != "unknown"


def test_sql_interface_captures_queries() -> None:
    card = DataCard(SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"))

    payload = json.loads(card.model_dump_json())

    assert payload["spec"]["interface"]["kind"] == "Sql"
    assert payload["spec"]["sql"]["queries"] == {"train": "select 1"}


def test_path_interfaces_use_path_metadata(tmp_path) -> None:
    jsonl = tmp_path / "rows.jsonl"
    jsonl.write_text('{"x": 1}\n', encoding="utf-8")
    text_dir = tmp_path / "text"
    text_dir.mkdir()
    (text_dir / "a.txt").write_text("hello", encoding="utf-8")
    image_dir = tmp_path / "images"
    image_dir.mkdir()
    (image_dir / "a.png").write_bytes(b"png")

    assert DataCard(JsonlInterface(data=jsonl)).interface.kind == "Jsonl"
    assert DataCard(TextInterface(data=text_dir)).interface.kind == "Text"
    assert DataCard(ImageInterface(data=image_dir)).interface.kind == "Image"


def test_datacard_rejects_non_data_interface() -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(object())

    assert exc.value.code == "WYRD_DATA_400_UNKNOWN_DATA_TYPE"


def test_explicit_interfaces_map_to_locked_meta(tmp_path) -> None:
    parquet = tmp_path / "data.parquet"
    parquet.write_bytes(b"PAR1")
    custom = CustomDataInterface(data={"x": 1}, loader_module="json", loader_class="JSONDecoder")

    assert PandasInterface(compression="zstd").to_dict()["meta"]["compression"] == "Zstd"
    parquet_meta = ParquetInterface(data=parquet, row_group_size=128).to_dict()["meta"]
    assert parquet_meta["row_group_size"] == 128
    assert JsonlInterface(compression="gzip").to_dict()["meta"]["compression"] == "Gzip"
    assert custom.to_dict()["meta"]["loader_module"] == "json"


def test_split_builders_serialize_and_validate() -> None:
    assert Split.column("year", "<=", 2024).to_dict()["value"]["op"] == "Le"
    assert Split.index_range(0, 10).to_dict()["value"] == {"start": 0, "stop": 10}
    assert Split.indices([0, 2, 3]).to_dict()["value"] == [0, 2, 3]
    assert Split.materialized({"kind": "Artifact", "name": "train", "version": "1"}).to_dict()

    with pytest.raises(WyrdError) as exc:
        Split.column("year", "between", [2020, 2024])
    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"

    with pytest.raises(WyrdError):
        Split.indices([1, 1])


def test_datacard_does_not_expose_register() -> None:
    card = DataCard(SqlInterface(data={"queries": {"q": "select 1"}}, dialect="duckdb"))

    assert not hasattr(card, "register")
