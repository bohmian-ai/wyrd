from __future__ import annotations

from pathlib import Path

import numpy as np
import pandas as pd
import polars as pl
import pyarrow as pa
import pytest
import torch
from wyrd.model import ModelSignature, SampleInput


def _output() -> np.ndarray:
    return np.array([0.1, 0.2], dtype=np.float32)


def _dynamic_batch_shape(width: int | None = None) -> list[dict]:
    shape = [{"kind": "Dynamic", "value": "batch"}]
    if width is not None:
        shape.append({"kind": "Fixed", "value": width})
    return shape


def test_model_signature_from_tabular_dataframes_and_arrow_tables() -> None:
    pandas_sig = ModelSignature.from_data(
        inputs=pd.DataFrame({"age": [1.0], "score": [2]}),
        outputs=_output(),
    )
    polars_sig = ModelSignature.from_data(
        inputs=pl.DataFrame({"age": [1.0]}),
        outputs=_output(),
    )
    arrow_sig = ModelSignature.from_data(
        inputs=pa.table({"age": pa.array([1, 2], type=pa.int64())}),
        outputs=_output(),
    )

    assert [(field.name, field.dtype) for field in pandas_sig.inputs] == [
        ("age", "float64"),
        ("score", "int64"),
    ]
    assert polars_sig.inputs[0].name == "age"
    assert polars_sig.inputs[0].dtype == "float64"
    assert arrow_sig.inputs[0].name == "age"
    assert arrow_sig.inputs[0].dtype == "int64"


def test_model_signature_from_tensor_like_inputs_marks_batch_axis_dynamic() -> None:
    numpy_sig = ModelSignature.from_data(
        inputs=np.ones((2, 3), dtype=np.float32),
        outputs=_output(),
    )
    torch_sig = ModelSignature.from_data(
        inputs=torch.ones((2, 4), dtype=torch.float32),
        outputs=_output(),
    )
    dict_sig = ModelSignature.from_data(
        inputs={"tokens": np.ones((2, 5), dtype=np.int64)},
        outputs=_output(),
    )

    assert numpy_sig.inputs[0].name == "value"
    assert numpy_sig.inputs[0].dtype == "float32"
    assert numpy_sig.inputs[0].shape == _dynamic_batch_shape(3)
    assert torch_sig.inputs[0].shape == _dynamic_batch_shape(4)
    assert dict_sig.inputs[0].name == "tokens"
    assert dict_sig.inputs[0].dtype == "int64"
    assert dict_sig.inputs[0].shape == _dynamic_batch_shape(5)


def test_model_signature_from_text_inputs_uses_text_field_contract() -> None:
    batch_sig = ModelSignature.from_data(inputs=["hello", "world"], outputs=_output())
    scalar_sig = ModelSignature.from_data(inputs="hello", outputs=_output())

    assert batch_sig.inputs[0].name == "text"
    assert batch_sig.inputs[0].dtype == "utf8"
    assert batch_sig.inputs[0].shape == _dynamic_batch_shape()
    assert scalar_sig.inputs[0].name == "text"
    assert scalar_sig.inputs[0].dtype == "utf8"
    assert scalar_sig.inputs[0].shape == []


@pytest.mark.parametrize(
    ("value", "kind", "filename"),
    [
        pytest.param(None, "none", None, id="none"),
        pytest.param(pd.DataFrame({"x": [1]}), "pandas", "sample_input.parquet", id="pandas"),
        pytest.param(pl.DataFrame({"x": [1]}), "polars", "sample_input.parquet", id="polars"),
        pytest.param(
            pa.table({"x": pa.array([1], type=pa.int64())}),
            "arrow",
            "sample_input.parquet",
            id="arrow",
        ),
        pytest.param(np.array([1], dtype=np.int64), "numpy", "sample_input.npy", id="numpy"),
        pytest.param(
            torch.tensor([1], dtype=torch.int64), "torch", "sample_input.safetensors", id="torch"
        ),
        pytest.param({"x": [1]}, "dict", "sample_input.json", id="dict"),
        pytest.param([1, 2], "list", "sample_input.json", id="list"),
        pytest.param((1, 2), "tuple", "sample_input.json", id="tuple"),
        pytest.param("hello", "str", "sample_input.txt", id="str"),
    ],
)
def test_sample_input_classification_save_and_load(
    tmp_path: Path,
    value,
    kind: str,
    filename: str | None,
) -> None:
    sample = SampleInput.from_python_object(value)
    path = tmp_path / kind

    assert sample.kind_token == kind
    assert sample.has_value is (value is not None)

    sample.save(path)
    if filename is None:
        assert not path.exists() or list(path.iterdir()) == []
    else:
        assert (path / filename).exists()

    loaded = SampleInput(kind=kind)
    loaded.load(path)
    assert loaded.kind_token == kind
    assert loaded.has_value is (value is not None)
