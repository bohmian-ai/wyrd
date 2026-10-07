"""Column dtypes a DataCard records for each supported framework."""

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
    DataInterface,
    NumpyInterface,
    PandasInterface,
    PolarsInterface,
    TorchInterface,
    WyrdError,
)


@pytest.mark.parametrize(
    ("interface", "dtype"),
    [
        pytest.param(
            PandasInterface(data=pd.DataFrame({"i": pd.Series([1], dtype="int64")})),
            "int64",
            id="pandas",
        ),
        pytest.param(
            PolarsInterface(data=pl.DataFrame({"i": pl.Series([1], dtype=pl.Int64)})),
            "int64",
            id="polars",
        ),
        pytest.param(
            ArrowInterface(data=pa.table({"i": pa.array([1], type=pa.int64())})),
            "int64",
            id="arrow",
        ),
        pytest.param(NumpyInterface(data=np.array([True], dtype=bool)), "bool", id="numpy"),
        pytest.param(
            TorchInterface(data=torch.tensor([1], dtype=torch.int64)), "int64", id="torch"
        ),
    ],
)
def test_datacard_records_the_framework_dtype(interface: DataInterface, dtype: str) -> None:
    assert DataCard(interface).schema.columns[0].dtype == dtype


def test_unknown_dtype_raises_unknown_data_type() -> None:
    class Weird:
        dtype = "madeup"
        shape = [1]

    with pytest.raises(WyrdError) as exc:
        DataCard(NumpyInterface(data=Weird()))

    assert exc.value.code == "WYRD_DATA_400_UNKNOWN_DATA_TYPE"
