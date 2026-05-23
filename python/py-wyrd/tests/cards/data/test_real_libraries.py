from __future__ import annotations

import json

import numpy as np
import pandas as pd
import polars as pl
import pyarrow as pa
import torch
from wyrd.data import (
    ArrowInterface,
    DataCard,
    NumpyInterface,
    PandasInterface,
    PolarsInterface,
    Split,
    TorchInterface,
)


def test_pandas_datacard_constructor_uses_real_pandas() -> None:
    frame = pd.DataFrame(
        {
            "year": [2020, 2022, 2019, 2021],
            "n_legs": [2, 4, 5, 100],
            "animals": ["Flamingo", "Horse", "Brittle stars", "Centipede"],
        }
    )

    card = DataCard(frame, name="test", space="test")

    assert card.interface.kind == "Pandas"
    assert [field.name for field in card.schema.columns] == ["year", "n_legs", "animals"]
    assert [field.dtype for field in card.schema.columns] == ["int64", "int64", "utf8"]


def test_polars_datacard_constructor_uses_real_polars() -> None:
    frame = pl.DataFrame({"foo": [1, 2, 3], "bar": ["a", "b", "c"], "y": [1, 2, 3]})

    card = DataCard(frame, name="test", space="test")

    assert card.interface.kind == "Polars"
    assert [field.name for field in card.schema.columns] == ["foo", "bar", "y"]
    assert [field.dtype for field in card.schema.columns] == ["int64", "utf8", "int64"]


def test_arrow_numpy_torch_constructors_use_real_libraries() -> None:
    arrow_card = DataCard(pa.table({"n_legs": pa.array([2, 4], type=pa.int64())}))
    numpy_card = DataCard(np.array([[1.0, 2.0], [3.0, 4.0]], dtype=np.float64))
    torch_card = DataCard(torch.tensor([[1, 2], [3, 4]], dtype=torch.int64))

    assert arrow_card.interface.kind == "Arrow"
    assert arrow_card.schema.columns[0].dtype == "int64"
    assert numpy_card.interface.kind == "Numpy"
    assert numpy_card.schema.columns[0].dtype == "float64"
    assert torch_card.interface.kind == "Torch"
    assert torch_card.schema.columns[0].dtype == "int64"


def test_split_cases_are_declared_without_physical_execution() -> None:
    split_payloads = {
        "polars_equal": Split.column("foo", "==", 3).to_dict(),
        "polars_lte": Split.column("foo", "<=", 3).to_dict(),
        "polars_lt": Split.column("foo", "<", 3).to_dict(),
        "polars_gte": Split.column("foo", ">=", 4).to_dict(),
        "polars_gt": Split.column("foo", ">", 4).to_dict(),
        "pandas_indices": Split.indices([0, 3, 5]).to_dict(),
        "pandas_range": Split.index_range(3, 5).to_dict(),
    }

    assert split_payloads["polars_equal"]["value"]["op"] == "Eq"
    assert split_payloads["polars_lte"]["value"]["op"] == "Le"
    assert split_payloads["polars_lt"]["value"]["op"] == "Lt"
    assert split_payloads["polars_gte"]["value"]["op"] == "Ge"
    assert split_payloads["polars_gt"]["value"]["op"] == "Gt"
    assert split_payloads["pandas_indices"]["value"] == [0, 3, 5]
    assert split_payloads["pandas_range"]["value"] == {"start": 3, "stop": 5}

    card = DataCard(PandasInterface(data=pd.DataFrame({"foo": [1, 2, 3]})))
    assert not hasattr(card, "split_data")


def test_interface_save_load_uses_core_real_libraries(tmp_path) -> None:
    cases = [
        DataCard(PandasInterface(data=pd.DataFrame({"x": [1, 2, 3]}))),
        DataCard(PolarsInterface(data=pl.DataFrame({"x": [1, 2, 3]}))),
        DataCard(ArrowInterface(data=pa.table({"x": pa.array([1, 2, 3], type=pa.int64())}))),
        DataCard(NumpyInterface(data=np.array([1, 2, 3], dtype=np.int64))),
        DataCard(TorchInterface(data=torch.tensor([1, 2, 3], dtype=torch.int64))),
    ]

    for idx, card in enumerate(cases):
        export = tmp_path / f"case-{idx}"
        card.save(export)
        card.load(export)
        payload = json.loads((export / "card.json").read_text(encoding="utf-8"))
        assert payload["kind"] == "Data"
        assert payload["spec"]["artifact_refs"] == []
        assert payload["spec"]["interface"]["kind"] == card.interface.kind
