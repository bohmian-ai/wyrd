"""Saving a DataCard locally and loading it back with ``DataCard.from_path``."""

from __future__ import annotations

import gzip
import json
from collections.abc import Callable
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

ROWS = [{"x": 1}, {"x": 2}]


class JsonInterface(DataInterface):
    """A user-defined interface that stores its data as one JSON file."""

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


@pytest.fixture
def sources(tmp_path: Path) -> dict[str, Path]:
    """Local files and directories a user would point a DataCard at."""
    root = tmp_path / "source"
    (root / "images").mkdir(parents=True)
    (root / "images" / "a.png").write_bytes(b"png")
    (root / "texts").mkdir()
    (root / "texts" / "a.txt").write_text("hello", encoding="utf-8")
    pq.write_table(pa.table({"x": pa.array([1, 2], type=pa.int64())}), root / "data.parquet")
    (root / "rows.jsonl").write_text('{"x": 1}\n{"x": 2}\n', encoding="utf-8")
    with gzip.open(root / "rows.jsonl.gz", "wt", encoding="utf-8") as handle:
        handle.write('{"x": 1}\n{"x": 2}\n')
    return {
        "parquet": root / "data.parquet",
        "jsonl": root / "rows.jsonl",
        "jsonl.gz": root / "rows.jsonl.gz",
        "images": root / "images",
        "texts": root / "texts",
    }


@pytest.fixture
def greetings() -> Dataset:
    """A small Hugging Face dataset with a recorded name."""
    data = Dataset.from_dict({"text": ["hello", "world"], "label": [0, 1]})
    data.info.dataset_name = "local/test"
    return data


def first_file(data: dict) -> str:
    return Path(data["files"][0]["path"]).name


Input = Callable[[dict[str, Path], Dataset], object]
Read = Callable[[object], object]


@pytest.mark.parametrize(
    ("make_data", "kind", "read", "expected"),
    [
        pytest.param(
            lambda s, h: pd.DataFrame(ROWS),
            "Pandas",
            lambda d: d["x"].tolist(),
            [1, 2],
            id="pandas",
        ),
        pytest.param(
            lambda s, h: pl.DataFrame(ROWS),
            "Polars",
            lambda d: d["x"].to_list(),
            [1, 2],
            id="polars",
        ),
        pytest.param(
            lambda s, h: pa.Table.from_pylist(ROWS),
            "Arrow",
            lambda d: d.column("x").to_pylist(),
            [1, 2],
            id="arrow",
        ),
        pytest.param(
            lambda s, h: np.array([1, 2]), "Numpy", lambda d: d.tolist(), [1, 2], id="numpy"
        ),
        pytest.param(
            lambda s, h: torch.tensor([1, 2]),
            "Torch",
            lambda d: d["value"].tolist(),
            [1, 2],
            id="torch",
        ),
        pytest.param(
            lambda s, h: {"queries": {"train": "select 1"}},
            "Sql",
            lambda d: d["queries"],
            {"train": "select 1"},
            id="sql",
        ),
        pytest.param(
            lambda s, h: h, "Huggingface", lambda d: d["text"], ["hello", "world"], id="huggingface"
        ),
        pytest.param(
            lambda s, h: s["parquet"],
            "Parquet",
            lambda d: d.column("x").to_pylist(),
            [1, 2],
            id="parquet-file",
        ),
        pytest.param(lambda s, h: s["jsonl"], "Jsonl", lambda d: d, ROWS, id="jsonl-file"),
        pytest.param(lambda s, h: s["jsonl.gz"], "Jsonl", lambda d: d, ROWS, id="gzip-jsonl-file"),
        pytest.param(lambda s, h: s["images"], "Image", first_file, "a.png", id="image-directory"),
        pytest.param(lambda s, h: s["texts"], "Text", first_file, "a.txt", id="text-directory"),
    ],
)
def test_detected_data_round_trips_through_a_saved_card(
    tmp_path: Path,
    sources: dict[str, Path],
    greetings: Dataset,
    make_data: Input,
    kind: str,
    read: Read,
    expected: object,
) -> None:
    DataCard(make_data(sources, greetings)).save(tmp_path / "card")

    card = DataCard.from_path(tmp_path / "card")

    assert card.interface.kind == kind
    assert read(card.data) == expected
    assert card.stats.byte_count > 0


@pytest.mark.parametrize(
    ("make_interface", "kind", "read", "expected"),
    [
        pytest.param(
            lambda s, h: PandasInterface(data=pd.DataFrame(ROWS)),
            "Pandas",
            lambda d: d["x"].tolist(),
            [1, 2],
            id="pandas",
        ),
        pytest.param(
            lambda s, h: PolarsInterface(data=pl.DataFrame(ROWS)),
            "Polars",
            lambda d: d["x"].to_list(),
            [1, 2],
            id="polars",
        ),
        pytest.param(
            lambda s, h: ArrowInterface(data=pa.Table.from_pylist(ROWS)),
            "Arrow",
            lambda d: d.column("x").to_pylist(),
            [1, 2],
            id="arrow-parquet",
        ),
        pytest.param(
            lambda s, h: ArrowInterface(data=pa.Table.from_pylist(ROWS), format="ipc"),
            "Arrow",
            lambda d: d.column("x").to_pylist(),
            [1, 2],
            id="arrow-ipc",
        ),
        pytest.param(
            lambda s, h: ParquetInterface(data=pa.Table.from_pylist(ROWS)),
            "Parquet",
            lambda d: d.column("x").to_pylist(),
            [1, 2],
            id="parquet",
        ),
        pytest.param(
            lambda s, h: NumpyInterface(data=np.array([1, 2])),
            "Numpy",
            lambda d: d.tolist(),
            [1, 2],
            id="numpy-npy",
        ),
        pytest.param(
            lambda s, h: NumpyInterface(data=np.array([1, 2]), format="npz"),
            "Numpy",
            lambda d: d.tolist(),
            [1, 2],
            id="numpy-npz",
        ),
        pytest.param(
            lambda s, h: TorchInterface(data=torch.tensor([1, 2])),
            "Torch",
            lambda d: d["value"].tolist(),
            [1, 2],
            id="torch-safetensors",
        ),
        pytest.param(
            lambda s, h: TorchInterface(data=torch.tensor([1, 2]), save_format="pickle"),
            "Torch",
            lambda d: d.tolist(),
            [1, 2],
            id="torch-pickle",
        ),
        pytest.param(
            lambda s, h: SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"),
            "Sql",
            lambda d: d["queries"],
            {"train": "select 1"},
            id="sql",
        ),
        pytest.param(
            lambda s, h: JsonlInterface(data=ROWS), "Jsonl", lambda d: d, ROWS, id="jsonl"
        ),
        pytest.param(
            lambda s, h: JsonlInterface(data=ROWS, compression="gzip"),
            "Jsonl",
            lambda d: d,
            ROWS,
            id="jsonl-gzip",
        ),
        pytest.param(
            lambda s, h: JsonlInterface(data=ROWS, compression="zstd"),
            "Jsonl",
            lambda d: d,
            ROWS,
            id="jsonl-zstd",
        ),
        pytest.param(
            lambda s, h: ImageInterface(data=s["images"]), "Image", first_file, "a.png", id="image"
        ),
        pytest.param(
            lambda s, h: TextInterface(data=s["texts"]), "Text", first_file, "a.txt", id="text"
        ),
        pytest.param(
            lambda s, h: HuggingfaceInterface(data=h, dataset_id="local/test"),
            "Huggingface",
            lambda d: d["text"],
            ["hello", "world"],
            id="huggingface",
        ),
    ],
)
def test_interface_data_round_trips_through_a_saved_card(
    tmp_path: Path,
    sources: dict[str, Path],
    greetings: Dataset,
    make_interface: Input,
    kind: str,
    read: Read,
    expected: object,
) -> None:
    DataCard(make_interface(sources, greetings)).save(tmp_path / "card")

    card = DataCard.from_path(tmp_path / "card")

    assert card.interface.kind == kind
    assert read(card.data) == expected
    assert card.stats.byte_count > 0


def test_custom_interface_round_trips_through_a_saved_card(tmp_path: Path) -> None:
    DataCard(JsonInterface(data={"x": 1})).save(tmp_path)

    card = DataCard.from_path(tmp_path, interface=JsonInterface)

    assert isinstance(card.interface, JsonInterface)
    assert card.data == {"x": 1}


def test_load_without_path_requires_registry_configuration() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"x": [1]})))

    with pytest.raises(WyrdError) as error:
        card.load()

    assert error.value.code == "WYRD_DATA_400_VALIDATION"


def test_save_does_not_create_artifact_cards_or_card_refs(tmp_path: Path) -> None:
    DataCard(PandasInterface(data=pd.DataFrame({"x": [1]}))).save(tmp_path)

    assert DataCard.from_path(tmp_path).metadata.to_dict()["card_refs"] == []


def test_huggingface_pointer_reloads_as_the_same_pointer(tmp_path: Path) -> None:
    DataCard(HuggingfaceInterface(dataset_id="acme/data", revision="abcdef0")).save(tmp_path)

    card = DataCard.from_path(tmp_path / "card.json")

    assert (card.interface.dataset_id, card.interface.revision) == ("acme/data", "abcdef0")


def test_huggingface_pointer_load_without_allow_remote_raises(tmp_path: Path) -> None:
    DataCard(HuggingfaceInterface(dataset_id="acme/data", revision="abcdef0")).save(tmp_path)

    with pytest.raises(WyrdError) as error:
        DataCard.from_path(tmp_path)

    assert error.value.code == "WYRD_DATA_400_VALIDATION"


def test_from_path_keeps_the_card_identity(tmp_path: Path) -> None:
    DataCard(pd.DataFrame({"x": [1, 2]}), name="churn").save(tmp_path)

    assert DataCard.from_path(tmp_path).name == "churn"


def test_from_path_reads_a_card_file_without_loading_data(tmp_path: Path) -> None:
    saved = DataCard(pd.DataFrame({"x": [1, 2]}), name="churn")
    saved.save(tmp_path)

    card = DataCard.from_path(tmp_path / "card.json")

    assert (card.uid, card.interface.kind, card.interface.has_source) == (
        saved.uid,
        "Pandas",
        False,
    )


def test_from_path_missing_file_raises_loader_error(tmp_path: Path) -> None:
    with pytest.raises(WyrdError) as error:
        DataCard.from_path(tmp_path / "absent.yaml")

    assert error.value.code == "WYRD_LOADER_400_IO"


def test_from_path_rejects_non_data_envelope(fixtures_dir: Path) -> None:
    with pytest.raises(WyrdError) as error:
        DataCard.from_path(fixtures_dir / "authoring" / "prompt" / "seeded-prompt.yaml")

    assert error.value.code == "WYRD_DATA_400_VALIDATION"


def test_splits_and_target_columns_survive_save_and_load(tmp_path: Path) -> None:
    DataCard(
        pd.DataFrame({"x": [1, 2, 3], "label": [0, 1, 0]}),
        splits={"train": Split.index_range(0, 2), "test": Split.column("x", ">", 2)},
        target_columns=["label"],
    ).save(tmp_path)

    card = DataCard.from_path(tmp_path)

    assert {name: split.to_dict() for name, split in card.splits.items()} == {
        "train": Split.index_range(0, 2).to_dict(),
        "test": Split.column("x", ">", 2).to_dict(),
    }
    assert card.target_columns == ["label"]


def test_datacard_rejects_target_column_missing_from_schema() -> None:
    with pytest.raises(WyrdError) as error:
        DataCard(pd.DataFrame({"x": [1]}), target_columns=["label"])

    assert error.value.code == "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN"


def test_datacard_rejects_split_on_unknown_column() -> None:
    with pytest.raises(WyrdError) as error:
        DataCard(pd.DataFrame({"x": [1]}), splits={"test": Split.column("y", "==", 1)})

    assert error.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"
