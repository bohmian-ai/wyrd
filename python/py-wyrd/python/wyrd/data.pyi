from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from wyrd._native import (
        ArrowInterface,
        DataCard,
        DataInterface,
        DataSchema,
        DataStats,
        FieldSpec,
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
else:
    # R12 registers this private native submodule dynamically in `sys.modules`.
    # The runtime import mirrors `wyrd.data`.
    from wyrd._native.cards.data import (
        ArrowInterface,
        DataCard,
        DataInterface,
        DataSchema,
        DataStats,
        FieldSpec,
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

__all__ = [
    "ArrowInterface",
    "DataCard",
    "DataInterface",
    "DataSchema",
    "DataStats",
    "FieldSpec",
    "HuggingfaceInterface",
    "ImageInterface",
    "JsonlInterface",
    "NumpyInterface",
    "PandasInterface",
    "ParquetInterface",
    "PolarsInterface",
    "Split",
    "SqlInterface",
    "TextInterface",
    "TorchInterface",
    "WyrdError",
]
