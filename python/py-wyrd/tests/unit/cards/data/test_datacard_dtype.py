from __future__ import annotations

import numpy as np
import pandas as pd
import polars as pl
import pyarrow as pa
import pytest
import torch
from wyrd.data import (
    ArrowInterface,
    DataCard,
    NumpyInterface,
    PandasInterface,
    PolarsInterface,
    TorchInterface,
    WyrdError,
)


def test_pandas_dtype_table() -> None:
    assert (
        DataCard(PandasInterface(data=pd.DataFrame({"i": pd.Series([1], dtype="int64")})))
        .schema.columns[0]
        .dtype
        == "int64"
    )


def test_polars_dtype_table() -> None:
    assert (
        DataCard(PolarsInterface(data=pl.DataFrame({"i": pl.Series([1], dtype=pl.Int64)})))
        .schema.columns[0]
        .dtype
        == "int64"
    )


def test_pyarrow_dtype_table() -> None:
    assert (
        DataCard(ArrowInterface(data=pa.table({"i": pa.array([1], type=pa.int64())})))
        .schema.columns[0]
        .dtype
        == "int64"
    )


def test_numpy_dtype_table() -> None:
    assert (
        DataCard(NumpyInterface(data=np.array([True], dtype=bool))).schema.columns[0].dtype
        == "bool"
    )


def test_torch_dtype_table_when_installed() -> None:
    assert (
        DataCard(TorchInterface(data=torch.tensor([1], dtype=torch.int64))).schema.columns[0].dtype
        == "int64"
    )


def test_unknown_dtype_raises_unknown_data_type() -> None:
    class Weird:
        dtype = "madeup"
        shape = [1]

    with pytest.raises(WyrdError) as exc:
        DataCard(NumpyInterface(data=Weird()))

    assert exc.value.code == "WYRD_DATA_400_UNKNOWN_DATA_TYPE"
