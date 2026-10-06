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
    Split,
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
    assert payload["spec"]["card_refs"] == []


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


def test_pandas_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = pd.DataFrame({"x": [1, 2]})
    path = tmp_path / "pandas-raw"

    card = DataCard(data)
    assert card.interface.kind == "Pandas"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data["x"].tolist() == [1, 2]
    _assert_card_json(path, "Pandas")
    _assert_stats(card)


def test_polars_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = pl.DataFrame({"x": [1, 2]})
    path = tmp_path / "polars-raw"

    card = DataCard(data)
    assert card.interface.kind == "Polars"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data["x"].to_list() == [1, 2]
    _assert_card_json(path, "Polars")
    _assert_stats(card)


def test_pyarrow_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = pa.table({"x": pa.array([1, 2], type=pa.int64())})
    path = tmp_path / "pyarrow-raw"

    card = DataCard(data)
    assert card.interface.kind == "Arrow"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data.column("x").to_pylist() == [1, 2]
    _assert_card_json(path, "Arrow")
    _assert_stats(card)


def test_numpy_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = np.array([1, 2], dtype=np.int64)
    path = tmp_path / "numpy-raw"

    card = DataCard(data)
    assert card.interface.kind == "Numpy"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data.tolist() == [1, 2]
    _assert_card_json(path, "Numpy")
    _assert_stats(card)


def test_torch_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = torch.tensor([1, 2], dtype=torch.int64)
    path = tmp_path / "torch-raw"

    card = DataCard(data)
    assert card.interface.kind == "Torch"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data["value"].tolist() == [1, 2]
    _assert_card_json(path, "Torch")
    _assert_stats(card)


def test_sql_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = {"queries": {"train": "select 1"}}
    path = tmp_path / "sql-raw"

    card = DataCard(data)
    assert card.interface.kind == "Sql"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data["queries"] == {"train": "select 1"}
    _assert_card_json(path, "Sql")
    _assert_stats(card)


def test_huggingface_raw_data_datacard_save_and_load(tmp_path: Path) -> None:
    data = _huggingface_dataset()
    path = tmp_path / "huggingface-raw"

    card = DataCard(data)
    assert card.interface.kind == "Huggingface"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data["text"] == ["hello", "world"]
    _assert_card_json(path, "Huggingface")
    _assert_stats(card)


def test_parquet_path_datacard_save_and_load(tmp_path: Path) -> None:
    data = _parquet_path(tmp_path / "source" / "data.parquet")
    path = tmp_path / "parquet-path"

    card = DataCard(data)
    assert card.interface.kind == "Parquet"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data.column("x").to_pylist() == [1, 2]
    _assert_card_json(path, "Parquet")
    _assert_stats(card)


def test_jsonl_path_datacard_save_and_load(tmp_path: Path) -> None:
    data = _jsonl_path(tmp_path / "source" / "rows.jsonl")
    path = tmp_path / "jsonl-path"

    card = DataCard(data)
    assert card.interface.kind == "Jsonl"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data == [{"x": 1}, {"x": 2}]
    _assert_card_json(path, "Jsonl")
    _assert_stats(card)


def test_gzip_jsonl_path_datacard_save_and_load(tmp_path: Path) -> None:
    data = _gzip_jsonl_path(tmp_path / "source" / "rows.jsonl.gz")
    path = tmp_path / "jsonl-gzip-path"

    card = DataCard(data)
    assert card.interface.kind == "Jsonl"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data == [{"x": 1}, {"x": 2}]
    _assert_card_json(path, "Jsonl")
    _assert_stats(card)


def test_image_directory_datacard_save_and_load(tmp_path: Path) -> None:
    data = _image_dir(tmp_path / "source" / "images")
    path = tmp_path / "image-directory"

    card = DataCard(data)
    assert card.interface.kind == "Image"
    card.save(path)

    card = DataCard.from_path(path)

    assert card.data["files"][0]["path"].endswith("a.png")
    _assert_card_json(path, "Image")
    _assert_stats(card)


def test_text_directory_datacard_save_and_load(tmp_path: Path) -> None:
    data = _text_dir(tmp_path / "source" / "text")
    path = tmp_path / "text-directory"

    card = DataCard(data)
    assert card.interface.kind == "Text"
    card.save(path)

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path)

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

    card = DataCard.from_path(path, interface=JsonInterface)

    assert isinstance(card.interface, JsonInterface)
    assert card.data == {"x": 1}
    _assert_card_json(path, "Custom")
    _assert_stats(card)


def test_load_without_path_requires_registry_configuration() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))

    with pytest.raises(WyrdError):
        card.load()


def test_save_does_not_create_artifact_cards_or_card_refs(tmp_path: Path) -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))

    card.save(tmp_path)

    payload = json.loads((tmp_path / "card.json").read_text(encoding="utf-8"))
    assert payload["spec"]["card_refs"] == []


def test_huggingface_pointer_only_save_writes_pointer_json(tmp_path: Path) -> None:
    interface = HuggingfaceInterface(dataset_id="acme/data", revision="abcdef0")
    card = DataCard(interface)
    card.save(tmp_path)

    assert (tmp_path / "data" / "dataset_pointer.json").is_file()
    _assert_card_json(tmp_path, "Huggingface")
    _assert_stats(card)


def test_huggingface_pointer_load_without_allow_remote_raises(tmp_path: Path) -> None:
    interface = HuggingfaceInterface(dataset_id="acme/data", revision="abcdef0")
    card = DataCard(interface)
    card.save(tmp_path)

    card2 = DataCard.model_validate_json((tmp_path / "card.json").read_text(encoding="utf-8"))
    with pytest.raises(WyrdError):
        card2.load(tmp_path)


def test_from_path_loads_saved_directory_in_one_call(tmp_path: Path) -> None:
    path = tmp_path / "pandas-from-path"
    DataCard(pd.DataFrame({"x": [1, 2]}), name="churn").save(path)

    card = DataCard.from_path(path)

    assert card.name == "churn"
    assert card.interface.kind == "Pandas"
    assert card.data["x"].tolist() == [1, 2]


def test_from_path_reads_card_yaml_file_without_loading_data(tmp_path: Path) -> None:
    path = tmp_path / "pandas-yaml"
    saved = DataCard(pd.DataFrame({"x": [1, 2]}), name="churn")
    saved.save(path)
    yaml_file = tmp_path / "card.yaml"
    yaml_file.write_text((path / "card.json").read_text(encoding="utf-8"), encoding="utf-8")

    card = DataCard.from_path(yaml_file)

    assert card.uid == saved.uid
    assert card.interface.kind == "Pandas"


def test_from_path_missing_file_raises_loader_error(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as error:
        DataCard.from_path(tmp_path / "absent.yaml")

    assert error.value.code == "WYRD_LOADER_400_IO"


def test_from_path_rejects_non_data_envelope(tmp_path: Path) -> None:
    path = tmp_path / "not-a-card.yaml"
    path.write_text("hello: world\n", encoding="utf-8")

    with pytest.raises(WyrdError) as error:
        DataCard.from_path(path)

    assert error.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"


def test_datacard_declares_typed_splits_and_target_columns(tmp_path: Path) -> None:
    data = pd.DataFrame({"x": [1, 2, 3], "label": [0, 1, 0]})
    path = tmp_path / "supervised"

    card = DataCard(
        data,
        splits={"train": Split.index_range(0, 2), "test": Split.column("x", ">", 2)},
        target_columns=["label"],
    )
    card.save(path)
    restored = DataCard.from_path(path)

    for holder in (card, restored):
        assert sorted(holder.splits) == ["test", "train"]
        assert isinstance(holder.splits["train"], Split)
        assert holder.splits["train"].to_dict() == Split.index_range(0, 2).to_dict()
        assert holder.target_columns == ["label"]


def test_datacard_rejects_target_column_missing_from_schema() -> None:
    with pytest.raises(WyrdError) as error:
        DataCard(pd.DataFrame({"x": [1]}), target_columns=["label"])

    assert error.value.code == "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN"


def test_datacard_rejects_split_on_unknown_column() -> None:
    with pytest.raises(WyrdError) as error:
        DataCard(pd.DataFrame({"x": [1]}), splits={"test": Split.column("y", "==", 1)})

    assert error.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_interface_options_read_back_as_typed_values() -> None:
    assert PandasInterface(compression="ZSTD").compression == "zstd"
    assert ArrowInterface(format="ipc").format == "ipc"
    parquet = ParquetInterface(compression="snappy", row_group_size=128)
    assert (parquet.compression, parquet.row_group_size) == ("snappy", 128)
    numpy = NumpyInterface(dtype="float32", shape=[2, 3], format="npz")
    assert (numpy.dtype, numpy.shape, numpy.format) == ("float32", [2, 3], "npz")
    assert TorchInterface(save_format="pickle").save_format == "pickle"
    sql = SqlInterface(dialect="postgres", connection_hint="warehouse")
    assert (sql.dialect, sql.connection_hint) == ("postgres", "warehouse")
    jsonl = JsonlInterface(compression="gzip", lines_per_file=10)
    assert (jsonl.compression, jsonl.lines_per_file) == ("gzip", 10)
    image = ImageInterface(format="png", color_mode="rgb")
    assert (image.format, image.color_mode) == ("png", "rgb")
    assert TextInterface(encoding="utf-8").encoding == "utf-8"
    hf = HuggingfaceInterface(dataset_id="org/ds", revision="abc1234", split="train", config="c")
    assert (hf.dataset_id, hf.revision, hf.split, hf.config) == ("org/ds", "abc1234", "train", "c")
