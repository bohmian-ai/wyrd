from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ._wyrd import (
        CatboostInterface,
        HuggingfaceInterface,
        LightgbmInterface,
        LightningInterface,
        ModelCard,
        ModelCardMetadata,
        ModelInterface,
        ModelSignature,
        SampleInput,
        SklearnInterface,
        TensorflowInterface,
        TorchInterface,
        WyrdError,
        XgboostInterface,
    )
else:
    from ._wyrd.cards.model import (
        CatboostInterface,
        HuggingfaceInterface,
        LightgbmInterface,
        LightningInterface,
        ModelCard,
        ModelCardMetadata,
        ModelInterface,
        ModelSignature,
        SampleInput,
        SklearnInterface,
        TensorflowInterface,
        TorchInterface,
        WyrdError,
        XgboostInterface,
    )

__all__ = [
    "CatboostInterface",
    "HuggingfaceInterface",
    "LightgbmInterface",
    "LightningInterface",
    "ModelCard",
    "ModelCardMetadata",
    "ModelInterface",
    "ModelSignature",
    "SampleInput",
    "SklearnInterface",
    "TensorflowInterface",
    "TorchInterface",
    "WyrdError",
    "XgboostInterface",
]
