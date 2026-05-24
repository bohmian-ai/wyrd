from __future__ import annotations

import gzip
import json
from hashlib import sha256
from pathlib import Path

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
    DataStats,
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


class JsonInterface(DataInterface):
    def __init__(self, data=None):
        super().__init__()
        self.data = data

    def save(self, path, save_kwargs=None):
        out = path / "data" / "custom"
        out.mkdir(parents=True, exist_ok=True)
        payload = json.dumps(self.data).encode("utf-8")
        (out / "data.json").write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path, load_kwargs=None):
        self.data = json.loads((path / "data" / "custom" / "data.json").read_text())


def _assert_card_json(path: Path, interface_kind: str) -> None:
    payload = json.loads((path / "card.json").read_text(encoding="utf-8"))
    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Data"
    assert payload["spec"]["interface"]["kind"] == interface_kind
    assert payload["spec"]["artifact_refs"] == []


def _assert_stats(card: DataCard) -> None:
    assert card.stats.byte_count > 0
    assert len(card.stats.sha256) == 64


def _huggingface_dataset() -> Dataset:
    data = Dataset.from_dict({"text": ["hello", "world"], "label": [0, 1]})
    data.info.dataset_name = "local/test"
    return data


def _parquet_path(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    table = pa.table({"x": pa.array([1, 2], type=pa.int64())})
    pq.write_table(table, path)
    return path


def _jsonl_path(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('{"x": 1}\n{"x": 2}\n', encoding="utf-8")
    return path


def _gzip_jsonl_path(path: Path) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    with gzip.open(path, "wt", encoding="utf-8") as handle:
        handle.write('{"x": 1}\n{"x": 2}\n')
    return path


def _image_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    (path / "a.png").write_bytes(b"png")
    return path


def _text_dir(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    (path / "a.txt").write_text("hello", encoding="utf-8")
    return path


@pytest.mark.wyrd_covers("python:DataCard.save")
@pytest.mark.wyrd_covers("python:DataCard.load")
def test_pandas_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = pd.DataFrame({"x": [1, 2]})
    path = tmp_path / "pandas-raw"

    card = DataCard(data)
    assert card.interface.kind == "Pandas"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["x"].tolist() == [1, 2]
    _assert_card_json(path, "Pandas")
    _assert_stats(card)


def test_polars_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = pl.DataFrame({"x": [1, 2]})
    path = tmp_path / "polars-raw"

    card = DataCard(data)
    assert card.interface.kind == "Polars"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["x"].to_list() == [1, 2]
    _assert_card_json(path, "Polars")
    _assert_stats(card)


def test_pyarrow_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = pa.table({"x": pa.array([1, 2], type=pa.int64())})
    path = tmp_path / "pyarrow-raw"

    card = DataCard(data)
    assert card.interface.kind == "Arrow"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.column("x").to_pylist() == [1, 2]
    _assert_card_json(path, "Arrow")
    _assert_stats(card)


def test_numpy_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = np.array([1, 2], dtype=np.int64)
    path = tmp_path / "numpy-raw"

    card = DataCard(data)
    assert card.interface.kind == "Numpy"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.tolist() == [1, 2]
    _assert_card_json(path, "Numpy")
    _assert_stats(card)


def test_torch_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = torch.tensor([1, 2], dtype=torch.int64)
    path = tmp_path / "torch-raw"

    card = DataCard(data)
    assert card.interface.kind == "Torch"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["value"].tolist() == [1, 2]
    _assert_card_json(path, "Torch")
    _assert_stats(card)


def test_sql_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = {"queries": {"train": "select 1"}}
    path = tmp_path / "sql-raw"

    card = DataCard(data)
    assert card.interface.kind == "Sql"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["queries"] == {"train": "select 1"}
    _assert_card_json(path, "Sql")
    _assert_stats(card)


def test_huggingface_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = _huggingface_dataset()
    path = tmp_path / "huggingface-raw"

    card = DataCard(data)
    assert card.interface.kind == "Huggingface"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["text"] == ["hello", "world"]
    _assert_card_json(path, "Huggingface")
    _assert_stats(card)


def test_parquet_path_datacard_save_and_load(tmp_path: Path) -> None:
    data = _parquet_path(tmp_path / "source" / "data.parquet")
    path = tmp_path / "parquet-path"

    card = DataCard(data)
    assert card.interface.kind == "Parquet"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.column("x").to_pylist() == [1, 2]
    _assert_card_json(path, "Parquet")
    _assert_stats(card)


def test_jsonl_path_datacard_save_and_load(tmp_path: Path) -> None:
    data = _jsonl_path(tmp_path / "source" / "rows.jsonl")
    path = tmp_path / "jsonl-path"

    card = DataCard(data)
    assert card.interface.kind == "Jsonl"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data == [{"x": 1}, {"x": 2}]
    _assert_card_json(path, "Jsonl")
    _assert_stats(card)


def test_gzip_jsonl_path_datacard_save_and_load(tmp_path: Path) -> None:
    data = _gzip_jsonl_path(tmp_path / "source" / "rows.jsonl.gz")
    path = tmp_path / "jsonl-gzip-path"

    card = DataCard(data)
    assert card.interface.kind == "Jsonl"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data == [{"x": 1}, {"x": 2}]
    _assert_card_json(path, "Jsonl")
    _assert_stats(card)


def test_image_directory_datacard_save_and_load(tmp_path: Path) -> None:
    data = _image_dir(tmp_path / "source" / "images")
    path = tmp_path / "image-directory"

    card = DataCard(data)
    assert card.interface.kind == "Image"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["files"][0]["path"].endswith("a.png")
    _assert_card_json(path, "Image")
    _assert_stats(card)


def test_text_directory_datacard_save_and_load(tmp_path: Path) -> None:
    data = _text_dir(tmp_path / "source" / "text")
    path = tmp_path / "text-directory"

    card = DataCard(data)
    assert card.interface.kind == "Text"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["files"][0]["path"].endswith("a.txt")
    _assert_card_json(path, "Text")
    _assert_stats(card)


def test_pandas_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = pd.DataFrame({"x": [1, 2]})
    interface = PandasInterface(data=data)
    path = tmp_path / "pandas-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Pandas"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["x"].tolist() == [1, 2]
    assert (path / "data" / "data.parquet").is_file()
    _assert_card_json(path, "Pandas")
    _assert_stats(card)


def test_polars_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = pl.DataFrame({"x": [1, 2]})
    interface = PolarsInterface(data=data)
    path = tmp_path / "polars-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Polars"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["x"].to_list() == [1, 2]
    assert (path / "data" / "data.parquet").is_file()
    _assert_card_json(path, "Polars")
    _assert_stats(card)


def test_arrow_parquet_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = pa.table({"x": pa.array([1, 2], type=pa.int64())})
    interface = ArrowInterface(data=data)
    path = tmp_path / "arrow-parquet-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Arrow"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.column("x").to_pylist() == [1, 2]
    assert (path / "data" / "data.parquet").is_file()
    _assert_card_json(path, "Arrow")
    _assert_stats(card)


def test_arrow_ipc_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = pa.table({"x": pa.array([1, 2], type=pa.int64())})
    interface = ArrowInterface(data=data, format="ipc")
    path = tmp_path / "arrow-ipc-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Arrow"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.column("x").to_pylist() == [1, 2]
    assert (path / "data" / "data.arrow").is_file()
    _assert_card_json(path, "Arrow")
    _assert_stats(card)


def test_parquet_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = _parquet_path(tmp_path / "source" / "data.parquet")
    interface = ParquetInterface(data=data)
    path = tmp_path / "parquet-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Parquet"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.column("x").to_pylist() == [1, 2]
    assert (path / "data" / "data.parquet").is_file()
    _assert_card_json(path, "Parquet")
    _assert_stats(card)


def test_numpy_npy_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = np.array([1, 2], dtype=np.int64)
    interface = NumpyInterface(data=data)
    path = tmp_path / "numpy-npy-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Numpy"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.tolist() == [1, 2]
    assert (path / "data" / "data.npy").is_file()
    _assert_card_json(path, "Numpy")
    _assert_stats(card)


def test_numpy_npz_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = np.array([1, 2], dtype=np.int64)
    interface = NumpyInterface(data=data, format="npz")
    path = tmp_path / "numpy-npz-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Numpy"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.tolist() == [1, 2]
    assert (path / "data" / "data.npz").is_file()
    _assert_card_json(path, "Numpy")
    _assert_stats(card)


def test_torch_safetensors_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = torch.tensor([1, 2], dtype=torch.int64)
    interface = TorchInterface(data=data)
    path = tmp_path / "torch-safetensors-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Torch"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["value"].tolist() == [1, 2]
    assert (path / "data" / "data.safetensors").is_file()
    _assert_card_json(path, "Torch")
    _assert_stats(card)


def test_torch_pickle_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = torch.tensor([1, 2], dtype=torch.int64)
    interface = TorchInterface(data=data, save_format="pickle")
    path = tmp_path / "torch-pickle-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Torch"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data.tolist() == [1, 2]
    assert (path / "data" / "data.pt").is_file()
    _assert_card_json(path, "Torch")
    _assert_stats(card)


def test_sql_interface_datacard_save_and_load(tmp_path: Path) -> None:
    interface = SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb")
    path = tmp_path / "sql-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Sql"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["queries"] == {"train": "select 1"}
    assert (path / "data" / "sql.json").is_file()
    _assert_card_json(path, "Sql")
    _assert_stats(card)


def test_jsonl_interface_datacard_save_and_load(tmp_path: Path) -> None:
    interface = JsonlInterface(data=[{"x": 1}, {"x": 2}])
    path = tmp_path / "jsonl-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Jsonl"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data == [{"x": 1}, {"x": 2}]
    assert (path / "data" / "data.jsonl").is_file()
    _assert_card_json(path, "Jsonl")
    _assert_stats(card)


def test_jsonl_gzip_interface_datacard_save_and_load(tmp_path: Path) -> None:
    interface = JsonlInterface(data=[{"x": 1}, {"x": 2}], compression="gzip")
    path = tmp_path / "jsonl-gzip-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Jsonl"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data == [{"x": 1}, {"x": 2}]
    assert (path / "data" / "data.jsonl.gz").is_file()
    _assert_card_json(path, "Jsonl")
    _assert_stats(card)


def test_jsonl_zstd_interface_datacard_save_and_load(tmp_path: Path) -> None:
    interface = JsonlInterface(data=[{"x": 1}, {"x": 2}], compression="zstd")
    path = tmp_path / "jsonl-zstd-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Jsonl"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data == [{"x": 1}, {"x": 2}]
    assert (path / "data" / "data.jsonl.zst").is_file()
    _assert_card_json(path, "Jsonl")
    _assert_stats(card)


def test_image_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = _image_dir(tmp_path / "source" / "images")
    interface = ImageInterface(data=data)
    path = tmp_path / "image-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Image"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["files"][0]["path"].endswith("a.png")
    assert (path / "data" / "manifest.json").is_file()
    _assert_card_json(path, "Image")
    _assert_stats(card)


def test_text_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = _text_dir(tmp_path / "source" / "text")
    interface = TextInterface(data=data)
    path = tmp_path / "text-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Text"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["files"][0]["path"].endswith("a.txt")
    assert (path / "data" / "manifest.json").is_file()
    _assert_card_json(path, "Text")
    _assert_stats(card)


def test_huggingface_interface_datacard_save_and_load(tmp_path: Path) -> None:
    data = _huggingface_dataset()
    interface = HuggingfaceInterface(data=data, dataset_id="local/test")
    path = tmp_path / "huggingface-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Huggingface"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text())
    card.load(path)

    assert card.data["text"] == ["hello", "world"]
    assert (path / "data" / "dataset").is_dir()
    _assert_card_json(path, "Huggingface")
    _assert_stats(card)


def test_custom_interface_datacard_save_and_load(tmp_path: Path) -> None:
    interface = JsonInterface(data={"x": 1})
    path = tmp_path / "custom-interface"

    card = DataCard(interface)
    assert card.interface.kind == "Custom"
    card.save(path)

    card = DataCard.model_validate_json((path / "card.json").read_text(), interface=JsonInterface)
    card.load(path)

    assert isinstance(card.interface, JsonInterface)
    assert card.data == {"x": 1}
    _assert_card_json(path, "Custom")
    _assert_stats(card)


def test_load_requires_a_local_path_when_artifacts_are_not_downloaded() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))

    with pytest.raises(WyrdError) as exc:
        card.load()

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_save_does_not_create_artifact_cards_or_artifact_refs(tmp_path: Path) -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))

    card.save(tmp_path)

    payload = json.loads((tmp_path / "card.json").read_text(encoding="utf-8"))
    assert payload["spec"]["artifact_refs"] == []
