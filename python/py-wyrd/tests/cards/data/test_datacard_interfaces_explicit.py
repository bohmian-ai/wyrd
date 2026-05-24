from __future__ import annotations

import pytest
from wyrd.data import (
    ArrowInterface,
    DataCard,
    DataInterface,
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


def test_explicit_pandas_interface_overrides_default_compression() -> None:
    assert PandasInterface(compression="zstd").to_dict()["meta"]["compression"] == "Zstd"


def test_explicit_polars_interface() -> None:
    assert PolarsInterface(compression="gzip").to_dict()["meta"]["compression"] == "Gzip"


def test_explicit_arrow_interface_ipc() -> None:
    assert ArrowInterface(format="ipc").to_dict()["meta"]["format"] == "Ipc"


def test_explicit_parquet_interface_row_group_size(tmp_path) -> None:
    assert ParquetInterface(row_group_size=64).to_dict()["meta"]["row_group_size"] == 64


def test_explicit_numpy_interface_dtype_shape() -> None:
    meta = NumpyInterface(dtype="float64", shape=[2, 3]).to_dict()["meta"]

    assert meta["dtype"] == "float64"
    assert meta["shape"] == [2, 3]


def test_explicit_torch_interface_safetensors_when_installed() -> None:
    assert (
        TorchInterface(save_format="safetensors").to_dict()["meta"]["save_format"] == "Safetensors"
    )


def test_explicit_sql_interface_requires_sql_dict() -> None:
    card = DataCard(SqlInterface(data={"queries": {"main": "select 1"}}, dialect="duckdb"))

    assert card.metadata.to_dict()["sql"]["queries"] == {"main": "select 1"}


def test_explicit_jsonl_interface(tmp_path) -> None:
    assert (
        JsonlInterface(compression="zstd", lines_per_file=10).to_dict()["meta"]["lines_per_file"]
        == 10
    )


def test_explicit_image_interface_requires_manifest_or_directory(tmp_path) -> None:
    assert ImageInterface(format="mixed", color_mode="rgb").has_source is False


def test_explicit_text_interface_requires_manifest_or_directory(tmp_path) -> None:
    assert TextInterface(encoding="utf-8").has_source is False


def test_explicit_huggingface_interface_revision_validation() -> None:
    with pytest.raises(WyrdError) as exc:
        HuggingfaceInterface(dataset_id="local/test", revision="bad-rev")

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_explicit_custom_interface_round_trips_extra() -> None:
    class MyInterface(DataInterface):
        def __init__(self):
            super().__init__()
            self.data = {"x": 1}

    assert DataCard(MyInterface()).interface.kind == "Custom"


def test_interface_data_conflict_raises_validation_error() -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(PandasInterface(data=object()))

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_invalid_interface_option_lists_allowed_values() -> None:
    with pytest.raises(WyrdError) as exc:
        PandasInterface(compression="brotli")

    assert exc.value.code == "WYRD_DATA_400_INVALID_INTERFACE_OPTION"
    assert "snappy" in str(exc.value)
