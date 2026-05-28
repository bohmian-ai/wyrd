#### begin imports ####

from collections.abc import Mapping, Sequence
from typing import Any, overload

from .data import FieldSpec
from .error import WyrdError
from .header import CardRefLike, JsonDict, PathLike, StringMap

#### end of imports ####

class ModelInterface:
    """Base class for Python model interfaces.

    Subclass this when Wyrd does not ship a built-in interface for your model
    framework. Custom interfaces are ordinary Python subclasses; there is no
    separate custom interface class. Override `save` and `load`, and override
    `from_metadata` if a no-argument constructor is not enough to rebuild the
    interface before loading local bytes.
    """

    kind: str

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        """Initialize a custom model interface.

        The base class accepts arbitrary arguments so subclasses can call
        `super().__init__()` without mirroring a Wyrd constructor. The base
        implementation records the interface kind as `Custom`.

        Args:
            *args (Any): Positional arguments accepted for subclass
                compatibility.
            **kwargs (Any): Keyword arguments accepted for subclass
                compatibility.
        """
        ...

    @classmethod
    def from_metadata(cls, metadata: ModelCardMetadata) -> ModelInterface:
        """Build an interface instance from serialized ModelCard metadata.

        `ModelCard.model_validate_json(..., interface=YourInterface)` calls
        this hook for custom subclasses. The default implementation calls the
        class with no arguments; override it when local loader configuration
        lives in `metadata`.

        Args:
            metadata (ModelCardMetadata): Metadata parsed from the serialized
                ModelCard envelope.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write model bytes into a local ModelCard directory.

        Built-in interfaces use their Wyrd convention path under `path`.
        Custom subclasses choose their own local layout but should keep it
        stable so `load` can rehydrate the same bytes later.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Optional interface-specific
                save options.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load model bytes from a local ModelCard directory.

        This method hydrates local bytes only. JSON parsing and metadata
        validation happen in `ModelCard.model_validate_json`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Optional interface-specific
                load options.
        """
        ...

class SklearnInterface(ModelInterface):
    """Model interface for scikit-learn estimators saved with joblib."""

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """Create a scikit-learn interface.

        Args:
            model (Any): Optional estimator to save or hold in memory.
            preprocessor (Any): Optional preprocessing object kept with the
                interface for caller-side workflows.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the estimator and optional preprocessor with joblib.

        Wyrd writes `model.joblib` under `path`. If `preprocessor` is present,
        it also writes `preprocessor.joblib`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future
                sklearn-specific save options.

        Raises:
            WyrdError: If `model` is not attached, joblib or scikit-learn is
                unavailable, or local serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the estimator and optional preprocessor from joblib files.

        Wyrd reads `model.joblib` under `path`. If `preprocessor.joblib`
        exists, it is loaded into the interface as well.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future
                sklearn-specific load options.

        Raises:
            WyrdError: If joblib or scikit-learn is unavailable, required local
                files are missing, or local deserialization fails.
        """
        ...

class XgboostInterface(ModelInterface):
    """Model interface for XGBoost sklearn-style models saved with joblib."""

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """Create an XGBoost interface.

        Args:
            model (Any): Optional XGBoost model to save or hold in memory.
            preprocessor (Any): Optional preprocessing object kept with the
                interface for caller-side workflows.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the XGBoost model and optional preprocessor with joblib.

        Wyrd writes `model.joblib` under `path`. If `preprocessor` is present,
        it also writes `preprocessor.joblib`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future
                XGBoost-specific save options.

        Raises:
            WyrdError: If `model` is not attached, joblib or XGBoost is
                unavailable, or local serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the XGBoost model and optional preprocessor from joblib files.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future
                XGBoost-specific load options.

        Raises:
            WyrdError: If joblib or XGBoost is unavailable, required local files
                are missing, or local deserialization fails.
        """
        ...

class LightgbmInterface(ModelInterface):
    """Model interface for LightGBM sklearn-style models saved with joblib."""

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """Create a LightGBM interface.

        Args:
            model (Any): Optional LightGBM model to save or hold in memory.
            preprocessor (Any): Optional preprocessing object kept with the
                interface for caller-side workflows.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the LightGBM model and optional preprocessor with joblib.

        Wyrd writes `model.joblib` under `path`. If `preprocessor` is present,
        it also writes `preprocessor.joblib`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future
                LightGBM-specific save options.

        Raises:
            WyrdError: If `model` is not attached, joblib or LightGBM is
                unavailable, or local serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the LightGBM model and optional preprocessor from joblib files.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future
                LightGBM-specific load options.

        Raises:
            WyrdError: If joblib or LightGBM is unavailable, required local
                files are missing, or local deserialization fails.
        """
        ...

class CatboostInterface(ModelInterface):
    """Model interface for CatBoost models saved with joblib."""

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """Create a CatBoost interface.

        Args:
            model (Any): Optional CatBoost model to save or hold in memory.
            preprocessor (Any): Optional preprocessing object kept with the
                interface for caller-side workflows.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the CatBoost model and optional preprocessor with joblib.

        Wyrd writes `model.joblib` under `path`. If `preprocessor` is present,
        it also writes `preprocessor.joblib`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future
                CatBoost-specific save options.

        Raises:
            WyrdError: If `model` is not attached, joblib or CatBoost is
                unavailable, or local serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the CatBoost model and optional preprocessor from joblib files.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future
                CatBoost-specific load options.

        Raises:
            WyrdError: If joblib or CatBoost is unavailable, required local
                files are missing, or local deserialization fails.
        """
        ...

class TorchInterface(ModelInterface):
    """Model interface for PyTorch modules."""

    has_model: bool
    framework_version: str
    model_subtype: str | None
    save_format: str

    def __init__(
        self,
        *,
        model: Any = ...,
        preprocessor: Any = ...,
        save_format: str = ...,
    ) -> None:
        """Create a PyTorch interface.

        Args:
            model (Any): Optional `torch.nn.Module` to save or hold in memory.
            preprocessor (Any): Optional preprocessing object kept with the
                interface for caller-side workflows.
            save_format (str): `safetensors` or `pickle`.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the PyTorch module and optional preprocessor.

        With `save_format="safetensors"`, Wyrd writes `model.safetensors` from
        the module state dict. With `save_format="pickle"`, it writes
        `model.pt` through `torch.save`. If `preprocessor` is present, Wyrd
        writes `preprocessor.joblib`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future
                PyTorch-specific save options.

        Raises:
            WyrdError: If `model` is not attached, required packages are
                unavailable, or local serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the PyTorch module from the local artifact layout.

        Safetensors loading requires an attached module instance so Wyrd can
        call `load_state_dict`. Pickle loading reads `model.pt` and replaces the
        held model object. If `preprocessor.joblib` exists, it is loaded too.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future
                PyTorch-specific load options.

        Raises:
            WyrdError: If required packages are unavailable, safetensors loading
                has no attached module instance, required files are missing, or
                local deserialization fails.
        """
        ...

class LightningInterface(ModelInterface):
    """Model interface for PyTorch Lightning modules."""

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(
        self,
        *,
        model: Any = ...,
        trainer: Any = ...,
        preprocessor: Any = ...,
    ) -> None:
        """Create a Lightning interface.

        Args:
            model (Any): Optional Lightning module to save or hold in memory.
            trainer (Any): Optional trainer used by the local checkpoint path.
            preprocessor (Any): Optional preprocessing object kept with the
                interface for caller-side workflows.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the Lightning module checkpoint and optional preprocessor.

        Wyrd writes `model.ckpt`. If a trainer is attached, Wyrd calls
        `trainer.save_checkpoint`; otherwise it saves the module state dict
        with torch. If `preprocessor` is present, it also writes
        `preprocessor.joblib`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future
                Lightning-specific save options.

        Raises:
            WyrdError: If `model` is not attached, required packages are
                unavailable, or checkpoint serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the Lightning checkpoint from the local artifact layout.

        If the attached `model` is a LightningModule class, Wyrd calls
        `load_from_checkpoint`. If it is an instance, Wyrd loads the checkpoint
        state dict into that instance. If `preprocessor.joblib` exists, it is
        loaded too.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future
                Lightning-specific load options.

        Raises:
            WyrdError: If no module class or instance is attached, required
                packages are unavailable, required files are missing, or
                checkpoint loading fails.
        """
        ...

class TensorflowInterface(ModelInterface):
    """Model interface for TensorFlow or Keras models."""

    has_model: bool
    framework_version: str
    model_subtype: str | None
    save_format: str

    def __init__(
        self,
        *,
        model: Any = ...,
        preprocessor: Any = ...,
        save_format: str = ...,
    ) -> None:
        """Create a TensorFlow interface.

        Args:
            model (Any): Optional TensorFlow or Keras model to save or hold in
                memory.
            preprocessor (Any): Optional preprocessing object kept with the
                interface for caller-side workflows.
            save_format (str): `keras` or `savedmodel`.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the TensorFlow/Keras model and optional preprocessor.

        With `save_format="keras"`, Wyrd writes `model.keras`. With
        `save_format="savedmodel"`, Wyrd exports a `savedmodel` directory. If
        `preprocessor` is present, Wyrd writes `preprocessor.joblib`.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future
                TensorFlow-specific save options.

        Raises:
            WyrdError: If `model` is not attached, TensorFlow is unavailable,
                or local serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the TensorFlow/Keras model from the local artifact layout.

        Keras format reads `model.keras`. SavedModel format reads the
        `savedmodel` directory. If `preprocessor.joblib` exists, it is loaded
        too.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future
                TensorFlow-specific load options.

        Raises:
            WyrdError: If TensorFlow is unavailable, required local files are
                missing, or local deserialization fails.
        """
        ...

class HuggingfaceInterface(ModelInterface):
    """Model interface for Hugging Face transformers models."""

    has_model: bool
    framework_version: str
    model_subtype: str | None
    hf_task: str
    repo_id: str | None
    revision: str | None

    def __init__(
        self,
        *,
        model: Any = ...,
        hf_task: str = ...,
        processor: Any = ...,
        repo_id: str | None = ...,
        revision: str | None = ...,
    ) -> None:
        """Create a Hugging Face interface.

        Args:
            model (Any): Optional `transformers.PreTrainedModel` to save or
                hold in memory.
            hf_task (str): Hugging Face task token, such as
                `text-classification` or `text-generation`.
            processor (Any): Optional tokenizer, processor, or feature
                extractor.
            repo_id (str | None): Optional Hub repository id used for reload
                metadata.
            revision (str | None): Optional pinned Hub revision.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the Hugging Face model and optional processor.

        Wyrd writes a `model/` directory under `path` using
        `save_pretrained`. If `processor` is attached, Wyrd writes it into the
        same directory so tokenizer or processor state travels with the model.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            save_kwargs (dict[str, Any] | None): Reserved for future Hugging
                Face-specific save options.

        Raises:
            WyrdError: If `model` is not attached, transformers is unavailable,
                or local serialization fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the Hugging Face model and optional processor from `path/model`.

        Wyrd chooses the `AutoModel*` class from `hf_task` and calls
        `from_pretrained` on the local `model/` directory. It then tries
        `AutoProcessor.from_pretrained` and `AutoTokenizer.from_pretrained` for
        processor rehydration.

        Args:
            path (PathLike): Local ModelCard materialization directory.
            load_kwargs (dict[str, Any] | None): Reserved for future Hugging
                Face-specific load options.

        Raises:
            WyrdError: If transformers is unavailable, the selected auto model
                class is unavailable, required local files are missing, or
                local deserialization fails.
        """
        ...

class ModelSignature:
    """Typed input/output signature for a ModelCard."""

    inputs: list[FieldSpec]
    outputs: list[FieldSpec]

    def __init__(
        self,
        inputs: Sequence[FieldSpec | Mapping[str, Any]],
        outputs: Sequence[FieldSpec | Mapping[str, Any]],
    ) -> None:
        """Create a model signature from explicit fields.

        Args:
            inputs (Sequence[FieldSpec | Mapping[str, Any]]): Ordered input
                fields.
            outputs (Sequence[FieldSpec | Mapping[str, Any]]): Ordered output
                fields.
        """
        ...

    @classmethod
    def from_data(cls, *, inputs: Any, outputs: Any) -> ModelSignature:
        """Infer a signature from sample input and output objects.

        Wyrd supports common dataframe, tensor, ndarray, text, and mapping
        shapes. Unsupported objects raise `WyrdError` with the missing
        signature code.

        Args:
            inputs (Any): Sample model input object.
            outputs (Any): Sample model output object.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this signature as a JSON-compatible Wyrd spec dictionary."""
        ...

class SampleInput:
    """Sample input descriptor for local model materialization."""

    kind_token: str
    has_value: bool

    def __init__(self, *, kind: str = ..., value: Any = ...) -> None:
        """Create a sample input shell.

        Args:
            kind (str): Sample kind token, such as `numpy`, `torch`, `dict`,
                `str`, or `none`.
            value (Any): Optional live Python sample value retained for local
                save.
        """
        ...

    @classmethod
    def from_python_object(cls, value: Any) -> SampleInput:
        """Classify a live Python object as a sample input.

        Args:
            value (Any): Python object to classify.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this sample input as a JSON-compatible Wyrd spec dictionary."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write the held sample input beside the local model artifact.

        Args:
            path (PathLike): Local model materialization directory.
            save_kwargs (dict[str, Any] | None): Optional sample-specific save
                options.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Read the kind-derived sample input artifact and retain it.

        Args:
            path (PathLike): Local model materialization directory.
            load_kwargs (dict[str, Any] | None): Optional sample-specific load
                options.
        """
        ...

class ModelCardMetadata:
    """Python holder metadata used to build a durable ModelCard spec."""

    interface: JsonDict
    task_type: str
    signature: ModelSignature | JsonDict
    sample_input: SampleInput | JsonDict | None
    artifact_refs: list[CardRefLike]

    def __init__(
        self,
        interface: ModelInterface | JsonDict | None = ...,
        task_type: str = ...,
        signature: ModelSignature | JsonDict | None = ...,
        sample_input: SampleInput | JsonDict | None = ...,
        artifact_refs: Sequence[CardRefLike] | None = ...,
    ) -> None:
        """Create ModelCard holder metadata.

        Args:
            interface (ModelInterface | JsonDict | None): Model interface
                instance or serialized model interface metadata.
            task_type (str): Task type token stored in the Model spec.
            signature (ModelSignature | JsonDict | None): Model signature
                metadata.
            sample_input (SampleInput | JsonDict | None): Optional sample input
                descriptor.
            artifact_refs (Sequence[CardRefLike] | None): Existing durable
                ArtifactCard references.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this metadata as a JSON-compatible Wyrd spec dictionary."""
        ...

class ModelCard:
    """Local ModelCard holder and spec builder.

    `save` and `load` only touch the local filesystem. Registration belongs to
    registry/client APIs, not the card object.
    """

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    metadata: ModelCardMetadata
    interface: ModelInterface | None
    task_type: str
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
        """Create a ModelCard from an explicit model interface.

        Args:
            model_or_interface (ModelInterface): Built-in interface or Python
                subclass that owns local save/load behavior.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `model`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated
                UUIDv7.
            labels (StringMap | None): Queryable user labels copied into the
                card metadata.
            annotations (StringMap | None): Free-form user annotations copied
                into the card metadata.
            metadata (ModelCardMetadata | None): Existing holder metadata to
                seed before interface conversion.

        Raises:
            WyrdError: If labels, annotations, interface metadata, or signature
                data violate the ModelCard contract.
        """
        ...

    @overload
    def __init__(
        self,
        model_or_interface: PathLike,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: ModelCardMetadata | None = ...,
    ) -> None:
        """Create a ModelCard from a local model artifact path.

        Args:
            model_or_interface (PathLike): Wyrd model materialization root or a
                convention artifact path such as `model.joblib`, `model.pt`,
                `model.keras`, `savedmodel`, or `model/`.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `model`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated
                UUIDv7.
            labels (StringMap | None): Queryable user labels copied into the
                card metadata.
            annotations (StringMap | None): Free-form user annotations copied
                into the card metadata.
            metadata (ModelCardMetadata | None): Existing holder metadata to
                seed before interface inference. A valid signature is required.
                Joblib artifacts require `metadata.interface` because the file
                layout is shared by Sklearn, XGBoost, LightGBM, and CatBoost.

        Raises:
            WyrdError: If Wyrd cannot infer a supported model interface or the
                supplied metadata violates the ModelCard contract.
        """
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
        """Create a ModelCard by inferring the interface from a runtime model.

        Args:
            model_or_interface (Any): Runtime model object such as a
                scikit-learn estimator, XGBoost model, LightGBM model, CatBoost
                model, PyTorch module, Lightning module, TensorFlow/Keras
                model, or Hugging Face pretrained model.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `model`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated
                UUIDv7.
            labels (StringMap | None): Queryable user labels copied into the
                card metadata.
            annotations (StringMap | None): Free-form user annotations copied
                into the card metadata.
            metadata (ModelCardMetadata | None): Existing holder metadata. It
                must contain a valid model signature unless Wyrd can infer one
                from the supplied objects.

        Raises:
            WyrdError: If Wyrd cannot infer a supported interface or the
                supplied metadata violates the ModelCard contract.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Materialize local model artifacts and write `card.json`.

        The held interface writes model bytes. Wyrd updates interface metadata
        and writes the ModelCard envelope; it does not create ArtifactCards or
        mutate `artifact_refs`.

        Args:
            path (PathLike): Local directory where Wyrd writes model bytes and
                `card.json`.
            save_kwargs (dict[str, Any] | None): Optional interface-specific
                save options.
        """
        ...

    def load(self, path: PathLike | None = ..., load_kwargs: dict[str, Any] | None = ...) -> None:
        """Hydrate local model artifacts through the held interface.

        Args:
            path (PathLike | None): Local materialization directory. Pass
                `None` only when another surface has already provided local
                model bytes for the interface.
            load_kwargs (dict[str, Any] | None): Optional interface-specific
                load options.
        """
        ...

    def model_dump_json(self) -> str:
        """Return this ModelCard envelope as JSON without filesystem IO."""
        ...

    @staticmethod
    def model_validate_json(json_string: str, interface: Any | None = ...) -> ModelCard:
        """Build a ModelCard from serialized Wyrd card JSON.

        Pass `interface=YourInterface` when the JSON describes a custom Python
        `ModelInterface`; Wyrd cannot rebuild custom subclasses from metadata
        alone.

        Args:
            json_string (str): Serialized ModelCard envelope.
            interface (Any | None): Optional built-in interface, Python
                subclass instance, or Python subclass type reconstructed
                through `from_metadata`.
        """
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
    "WyrdError",
    "XgboostInterface",
]
