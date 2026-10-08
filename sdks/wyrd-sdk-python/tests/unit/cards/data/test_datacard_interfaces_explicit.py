"""Interface constructor options, read back as typed values."""

from __future__ import annotations

from collections.abc import Callable

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


@pytest.mark.parametrize(
    ("interface", "read", "expected"),
    [
        pytest.param(
            PandasInterface(compression="ZSTD"),
            lambda i: i.compression,
            "zstd",
            id="pandas-compression",
        ),
        pytest.param(
            PolarsInterface(compression="gzip"),
            lambda i: i.compression,
            "gzip",
            id="polars-compression",
        ),
        pytest.param(ArrowInterface(format="ipc"), lambda i: i.format, "ipc", id="arrow-format"),
        pytest.param(
            ParquetInterface(compression="snappy", row_group_size=128),
            lambda i: (i.compression, i.row_group_size),
            ("snappy", 128),
            id="parquet-options",
        ),
        pytest.param(
            NumpyInterface(dtype="float32", shape=[2, 3], format="npz"),
            lambda i: (i.dtype, i.shape, i.format),
            ("float32", [2, 3], "npz"),
            id="numpy-options",
        ),
        pytest.param(
            TorchInterface(save_format="pickle"),
            lambda i: i.save_format,
            "pickle",
            id="torch-save-format",
        ),
        pytest.param(
            SqlInterface(dialect="postgres", connection_hint="warehouse"),
            lambda i: (i.dialect, i.connection_hint),
            ("postgres", "warehouse"),
            id="sql-options",
        ),
        pytest.param(
            JsonlInterface(compression="gzip", lines_per_file=10),
            lambda i: (i.compression, i.lines_per_file),
            ("gzip", 10),
            id="jsonl-options",
        ),
        pytest.param(
            ImageInterface(format="png", color_mode="rgb"),
            lambda i: (i.format, i.color_mode),
            ("png", "rgb"),
            id="image-options",
        ),
        pytest.param(
            TextInterface(encoding="utf-16"), lambda i: i.encoding, "utf-16", id="text-encoding"
        ),
        pytest.param(
            HuggingfaceInterface(
                dataset_id="org/ds", revision="abc1234", split="train", config="c"
            ),
            lambda i: (i.dataset_id, i.revision, i.split, i.config),
            ("org/ds", "abc1234", "train", "c"),
            id="huggingface-options",
        ),
    ],
)
def test_interface_options_read_back_as_typed_values(
    interface: DataInterface, read: Callable[[DataInterface], object], expected: object
) -> None:
    assert read(interface) == expected


@pytest.mark.parametrize(
    "interface",
    [ImageInterface(format="mixed", color_mode="rgb"), TextInterface(encoding="utf-8")],
    ids=["image", "text"],
)
def test_file_interface_has_no_source_until_data_is_given(interface: DataInterface) -> None:
    assert interface.has_source is False


def test_sql_interface_records_its_queries_on_the_card() -> None:
    card = DataCard(SqlInterface(data={"queries": {"main": "select 1"}}, dialect="duckdb"))

    assert card.data["queries"] == {"main": "select 1"}


def test_huggingface_revision_must_be_lowercase_hex() -> None:
    with pytest.raises(WyrdError) as exc:
        HuggingfaceInterface(dataset_id="local/test", revision="bad-rev")

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_interface_data_conflict_raises_validation_error() -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard(PandasInterface(data=object()))

    assert exc.value.code == "WYRD_DATA_400_VALIDATION"


def test_invalid_interface_option_lists_allowed_values() -> None:
    with pytest.raises(WyrdError) as exc:
        PandasInterface(compression="brotli")

    assert exc.value.code == "WYRD_DATA_400_INVALID_INTERFACE_OPTION"
    assert exc.value.details == {
        "field": "compression",
        "got": "brotli",
        "accepted": ["none", "snappy", "gzip", "zstd", "lz4"],
    }
