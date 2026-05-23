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
    CustomDataInterface,
    DataCard,
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
)


class JsonLoader:
    @staticmethod
    def save(data, path, **_extra):
        path.mkdir(parents=True, exist_ok=True)
        (path / "data.json").write_text(json.dumps(data), encoding="utf-8")

    @staticmethod
    def load(path, **_extra):
        return json.loads((path / "data.json").read_text(encoding="utf-8"))


def test_pandas_save_writes_parquet_and_card_json(tmp_path) -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"year": [2024], "churn": [False]})))

    card.save(tmp_path)

    assert (tmp_path / "data/data.parquet").is_file()
    payload = json.loads((tmp_path / "card.json").read_text(encoding="utf-8"))
    assert payload["spec"]["save_metadata"]["relative_path"] == "data/data.parquet"
    assert payload["spec"]["artifact_refs"] == []

    card.load(tmp_path)
    assert list(card.data.columns) == ["year", "churn"]


def test_numpy_save_load_round_trip_without_pickle(tmp_path) -> None:
    card = DataCard(NumpyInterface(data=np.array([1, 2, 3], dtype="int64")))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/data.npy").is_file()
    assert card.data.tolist() == [1, 2, 3]


def test_polars_save_load_round_trip_with_real_polars(tmp_path) -> None:
    card = DataCard(PolarsInterface(data=pl.DataFrame({"year": [2024], "churn": [False]})))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/data.parquet").is_file()
    assert card.data.columns == ["year", "churn"]


def test_arrow_parquet_save_load_round_trip_with_real_pyarrow(tmp_path) -> None:
    table = pa.table({"year": pa.array([2024], type=pa.int64()), "churn": [False]})
    card = DataCard(ArrowInterface(data=table))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/data.parquet").is_file()
    assert card.data.column_names == ["year", "churn"]


def test_arrow_ipc_save_load_round_trip_with_real_pyarrow(tmp_path) -> None:
    table = pa.table({"year": pa.array([2024], type=pa.int64()), "churn": [False]})
    card = DataCard(ArrowInterface(data=table, format="ipc"))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/data.arrow").is_file()
    assert card.data.column_names == ["year", "churn"]


def test_parquet_path_save_load_round_trip_with_real_pyarrow(tmp_path) -> None:
    source = tmp_path / "source.parquet"
    pq.write_table(pa.table({"year": pa.array([2024], type=pa.int64())}), source)
    export = tmp_path / "export"
    card = DataCard(ParquetInterface(data=source))

    card.save(export)
    card.load(export)

    assert (export / "data/data.parquet").is_file()
    assert card.data.column_names == ["year"]


def test_torch_safetensors_save_load_round_trip_with_real_torch(tmp_path) -> None:
    card = DataCard(TorchInterface(data=torch.tensor([[1, 2, 3]], dtype=torch.int64)))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/data.safetensors").is_file()
    assert card.data["value"].shape == (1, 3)


def test_torch_pickle_requires_explicit_save_format_and_round_trips(tmp_path) -> None:
    card = DataCard(TorchInterface(data=torch.tensor([1, 2, 3]), save_format="pickle"))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/data.pt").is_file()
    assert card.data.tolist() == [1, 2, 3]


def test_sql_save_load_returns_canonical_query_bundle(tmp_path) -> None:
    card = DataCard(SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"))

    card.save(tmp_path)
    card.load(tmp_path)

    assert card.data["queries"] == {"train": "select 1"}


def test_jsonl_save_load_round_trip(tmp_path) -> None:
    card = DataCard(JsonlInterface(data=[{"x": 1}, {"x": 2}]))

    card.save(tmp_path)
    card.load(tmp_path)

    assert card.data == [{"x": 1}, {"x": 2}]


def test_jsonl_gzip_save_load_round_trip(tmp_path) -> None:
    card = DataCard(JsonlInterface(data=[{"x": 1}, {"x": 2}], compression="gzip"))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/data.jsonl.gz").is_file()
    assert card.data == [{"x": 1}, {"x": 2}]


def test_image_manifest_save_load_returns_manifest(tmp_path) -> None:
    source = tmp_path / "images"
    source.mkdir()
    (source / "a.png").write_bytes(b"png")
    export = tmp_path / "export"
    card = DataCard(ImageInterface(data=source))

    card.save(export)
    card.load(export)

    assert card.data[0]["path"].endswith("a.png")


def test_text_manifest_save_load_returns_manifest(tmp_path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    (source / "a.txt").write_text("hello", encoding="utf-8")
    export = tmp_path / "export"
    card = DataCard(TextInterface(data=source))

    card.save(export)
    card.load(export)

    assert card.data[0]["path"].endswith("a.txt")


def test_huggingface_dataset_save_load_uses_real_datasets(tmp_path) -> None:
    dataset = Dataset.from_dict({"text": ["hello", "world"], "label": [0, 1]})
    card = DataCard(HuggingfaceInterface(data=dataset, dataset_id="local/test"))

    card.save(tmp_path)
    card.load(tmp_path)

    assert (tmp_path / "data/dataset").is_dir()
    assert len(card.data) == 2


def test_custom_interface_calls_user_save_and_load(tmp_path) -> None:
    card = DataCard(
        CustomDataInterface(
            data={"x": 1},
            loader_module=__name__,
            loader_class="JsonLoader",
        )
    )

    card.save(tmp_path)
    card.load(tmp_path)

    assert card.data == {"x": 1}


def test_load_requires_local_path_when_registry_has_not_downloaded(tmp_path) -> None:
    card = DataCard(SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"))
    card.save(tmp_path)

    with pytest.raises(Exception, match="local artifact path is required"):
        card.load()
