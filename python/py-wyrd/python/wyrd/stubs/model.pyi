#### begin imports ####

from collections.abc import Mapping, Sequence
from typing import Any, overload

from .data import FieldSpec
from .header import JsonDict, PathLike, StringMap

#### end of imports ####

class ModelInterface:
    """Base class for Python model interfaces.

    Subclass this class for custom Python-only model materialization. A
    subclass must override `save` and `load`; built-in framework interfaces are
    provided for common ML libraries.
    """

    kind: str

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        """Initialize a custom model interface base."""
        ...

    @classmethod
    def from_metadata(cls, metadata: ModelCardMetadata) -> ModelInterface:
        """Build an interface instance from serialized ModelCard metadata."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write model bytes into a local ModelCard directory."""
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load model bytes from a local ModelCard directory."""
        ...

class SklearnInterface(ModelInterface):
    """Model interface for scikit-learn estimators."""

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None: ...

class XgboostInterface(ModelInterface):
    """Model interface for XGBoost models."""

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None: ...

class LightgbmInterface(ModelInterface):
    """Model interface for LightGBM models."""

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None: ...

class CatboostInterface(ModelInterface):
    """Model interface for CatBoost models."""

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None: ...

class TorchInterface(ModelInterface):
    """Model interface for PyTorch modules."""

    def __init__(
        self,
        *,
        model: Any = ...,
        preprocessor: Any = ...,
        save_format: str = ...,
    ) -> None: ...

class LightningInterface(ModelInterface):
    """Model interface for PyTorch Lightning modules."""

    def __init__(
        self,
        *,
        model: Any = ...,
        trainer: Any = ...,
        preprocessor: Any = ...,
    ) -> None: ...

class TensorflowInterface(ModelInterface):
    """Model interface for TensorFlow or Keras models."""

    def __init__(
        self,
        *,
        model: Any = ...,
        preprocessor: Any = ...,
        save_format: str = ...,
    ) -> None: ...

class HuggingfaceInterface(ModelInterface):
    """Model interface for Hugging Face transformers models."""

    def __init__(
        self,
        *,
        hf_task: str,
        model: Any = ...,
        processor: Any = ...,
        repo_id: str | None = ...,
        revision: str | None = ...,
    ) -> None: ...

class ModelSignature:
    """Typed input/output signature for a ModelCard."""

    inputs: list[FieldSpec]
    outputs: list[FieldSpec]

    def __init__(
        self,
        inputs: Sequence[FieldSpec | Mapping[str, Any]],
        outputs: Sequence[FieldSpec | Mapping[str, Any]],
    ) -> None:
        """Create a model signature from explicit fields."""
        ...

    @classmethod
    def from_data(cls, *, inputs: Any, outputs: Any) -> ModelSignature:
        """Infer a model signature from sample input and output objects."""
        ...

    def to_dict(self) -> JsonDict:
        """Return this signature as a JSON-compatible Wyrd spec dictionary."""
        ...

class SampleInput:
    """Sample input descriptor for local model materialization."""

    kind_token: str
    has_value: bool

    def __init__(self, *, kind: str = ..., value: Any = ...) -> None:
        """Create a sample input shell from an explicit kind token."""
        ...

    @classmethod
    def from_python_object(cls, value: Any) -> SampleInput:
        """Classify a live Python object as a sample input."""
        ...

    def to_dict(self) -> JsonDict:
        """Return this sample input as a JSON-compatible Wyrd spec dictionary."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write the held sample input beside the local model artifact."""
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Read the kind-derived sample input artifact and retain it."""
        ...

class ModelCardMetadata:
    """Python holder metadata used to build a durable ModelCard spec."""

    def to_dict(self) -> JsonDict:
        """Return this metadata as a JSON-compatible Wyrd spec dictionary."""
        ...

class ModelCard:
    """Local ModelCard holder and spec builder."""

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    metadata: ModelCardMetadata
    interface: ModelInterface | None
    signature: ModelSignature
    sample_input: SampleInput | None

    @overload
    def __init__(
        self,
        model_or_interface: ModelInterface,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: ModelCardMetadata | None = ...,
    ) -> None:
        """Create a ModelCard from an explicit model interface."""
        ...

    @overload
    def __init__(
        self,
        model_or_interface: Any,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: ModelCardMetadata | None = ...,
    ) -> None:
        """Create a ModelCard from a raw supported framework model object."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Materialize local model artifacts and write `card.json`."""
        ...

    def load(self, path: PathLike | None = ..., load_kwargs: dict[str, Any] | None = ...) -> None:
        """Hydrate local model artifacts through the held interface."""
        ...

    def model_dump_json(self) -> str:
        """Return this ModelCard envelope as JSON without filesystem IO."""
        ...

    @staticmethod
    def model_validate_json(json_string: str, interface: Any = ...) -> ModelCard:
        """Build a ModelCard from serialized Wyrd card JSON."""
        ...

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
    "XgboostInterface",
]
