from __future__ import annotations

import pytest
from wyrd.data import (
    ArrowInterface,
    CustomDataInterface,
    HuggingfaceInterface,
    ImageInterface,
    JsonlInterface,
    NumpyInterface,
    PandasInterface,
    PolarsInterface,
    TextInterface,
    TorchInterface,
    WyrdError,
)


def test_explicit_interface_options_round_trip() -> None:
    assert PandasInterface(compression="zstd").to_dict()["meta"]["compression"] == "Zstd"
    assert PolarsInterface(compression="gzip").to_dict()["meta"]["compression"] == "Gzip"
    assert ArrowInterface(format="ipc").to_dict()["meta"]["format"] == "Ipc"
    assert NumpyInterface(dtype="int64", shape=[2, 2]).to_dict()["meta"]["shape"] == [2, 2]
    assert TorchInterface(save_format="pickle").to_dict()["meta"]["save_format"] == "Pickle"
    assert JsonlInterface(compression="gzip").to_dict()["meta"]["compression"] == "Gzip"
    assert ImageInterface(format="png", color_mode="rgba").to_dict()["meta"]["color_mode"] == "Rgba"
    assert TextInterface(encoding="utf-16").to_dict()["meta"]["encoding"] == "utf-16"
    assert HuggingfaceInterface(dataset_id="local").to_dict()["meta"]["dataset_id"] == "local"
    assert CustomDataInterface(
        loader_module="json",
        loader_class="JSONDecoder",
        extra={"mode": "strict"},
    ).to_dict()["meta"]["extra"] == {"mode": "strict"}


def test_invalid_interface_option_lists_allowed_values() -> None:
    with pytest.raises(WyrdError):
        PandasInterface(compression="brotli")
