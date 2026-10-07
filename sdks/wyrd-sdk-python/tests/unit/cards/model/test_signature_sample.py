"""Signatures inferred from sample data, and sample inputs saved beside a model."""

from pathlib import Path
from typing import Any

import numpy as np
import pandas as pd
import polars as pl
import pyarrow as pa
import pytest
import torch
from wyrd.data import Dim
from wyrd.model import ModelSignature, SampleInput

OUTPUT = np.array([0.1, 0.2], dtype=np.float32)


@pytest.mark.parametrize(
    ("inputs", "fields"),
    [
        pytest.param(
            pd.DataFrame({"age": [1.0], "score": [2]}),
            [("age", "float64"), ("score", "int64")],
            id="pandas",
        ),
        pytest.param(pl.DataFrame({"age": [1.0]}), [("age", "float64")], id="polars"),
        pytest.param(
            pa.table({"age": pa.array([1, 2], type=pa.int64())}), [("age", "int64")], id="arrow"
        ),
    ],
)
def test_tabular_inputs_infer_one_field_per_column(
    inputs: Any, fields: list[tuple[str, str]]
) -> None:
    signature = ModelSignature.from_data(inputs=inputs, outputs=OUTPUT)

    assert [(field.name, field.dtype) for field in signature.inputs] == fields


@pytest.mark.parametrize(
    ("inputs", "name", "dtype", "shape"),
    [
        pytest.param(
            np.ones((2, 3), dtype=np.float32),
            "value",
            "float32",
            [Dim.dynamic("batch"), Dim.fixed(3)],
            id="numpy",
        ),
        pytest.param(
            torch.ones((2, 4), dtype=torch.float32),
            "value",
            "float32",
            [Dim.dynamic("batch"), Dim.fixed(4)],
            id="torch",
        ),
        pytest.param(
            {"tokens": np.ones((2, 5), dtype=np.int64)},
            "tokens",
            "int64",
            [Dim.dynamic("batch"), Dim.fixed(5)],
            id="dict",
        ),
        pytest.param(["hello", "world"], "text", "utf8", [Dim.dynamic("batch")], id="text-batch"),
        pytest.param("hello", "text", "utf8", [], id="text-scalar"),
    ],
)
def test_inputs_infer_their_field_and_batch_shape(
    inputs: Any, name: str, dtype: str, shape: list[Dim]
) -> None:
    field = ModelSignature.from_data(inputs=inputs, outputs=OUTPUT).inputs[0]

    assert (field.name, field.dtype, field.shape) == (name, dtype, shape)


@pytest.mark.parametrize(
    ("value", "kind"),
    [
        pytest.param(None, "none", id="none"),
        pytest.param(pd.DataFrame({"x": [1]}), "pandas", id="pandas"),
        pytest.param(pl.DataFrame({"x": [1]}), "polars", id="polars"),
        pytest.param(pa.table({"x": pa.array([1], type=pa.int64())}), "arrow", id="arrow"),
        pytest.param(np.array([1], dtype=np.int64), "numpy", id="numpy"),
        pytest.param(torch.tensor([1], dtype=torch.int64), "torch", id="torch"),
        pytest.param({"x": [1]}, "dict", id="dict"),
        pytest.param([1, 2], "list", id="list"),
        pytest.param((1, 2), "tuple", id="tuple"),
        pytest.param("hello", "str", id="str"),
    ],
)
def test_sample_input_survives_save_and_load(tmp_path: Path, value: Any, kind: str) -> None:
    SampleInput.from_python_object(value).save(tmp_path)

    loaded = SampleInput(kind=kind)
    loaded.load(tmp_path)

    assert (loaded.kind_token, loaded.has_value) == (kind, value is not None)
