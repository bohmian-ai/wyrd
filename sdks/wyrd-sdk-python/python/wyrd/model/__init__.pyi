# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####

from collections.abc import Mapping, Sequence
from typing import Any, overload

from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError
from ..cards import CardRef, JsonValue, ModelLoadArgs
from ..data import FieldSpec

#### end of imports ####

class ModelInterface:
    """Base class for ModelCard model interfaces.

    Subclass it for model frameworks no built-in interface handles. A
    subclass must override ``save`` and ``load``; the base methods raise so a
    missing override cannot record an empty artifact. ``kind`` is the
    interface kind stored in the card: the built-in name, or ``Custom`` for
    subclasses.

    Built-in interfaces also expose ``has_model``, ``framework_version`` (the
    installed framework package version, or ``"unknown"``), and
    ``model_subtype`` (the held model's class name, when known). Their
    ``save_kwargs`` and ``load_kwargs`` are accepted and ignored.
    """

    kind: str

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        """Initialize a custom interface; all arguments are ignored.

        Accepting anything lets a subclass call ``super().__init__(...)``
        with its own arguments.
        """
        ...

    @classmethod
    def from_metadata(cls, metadata: ModelCardMetadata) -> ModelInterface:
        """Build an interface instance from a stored card's metadata.

        ``ModelCard.model_validate_json(..., interface=MyInterface)`` and
        card retrieval call this when given an interface class. The default
        calls the class with no arguments; override it when reconstruction
        needs values from ``metadata``.

        Args:
            metadata: metadata parsed from the stored ModelCard.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` if the default
                implementation cannot construct the class with no arguments.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write model bytes under a local card directory.

        A subclass override chooses its own layout under ``path`` and must
        keep it stable so ``load`` can read it back.

        Args:
            path: local card directory.
            save_kwargs: interface-specific options; built-ins ignore them.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` for the base class or a
                built-in with no ``model``,
                ``WYRD_MODEL_501_SERIALIZER_UNAVAILABLE`` if a required
                package is not installed, or another ``WyrdError`` if writing
                fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Read model bytes from a local card directory and hold them.

        Args:
            path: local card directory.
            load_kwargs: interface-specific options; built-ins ignore them.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` for the base class,
                ``WYRD_MODEL_501_SERIALIZER_UNAVAILABLE`` if a required
                package is not installed, or another ``WyrdError`` if the
                artifact is missing or cannot be read.
        """
        ...

class SklearnInterface(ModelInterface):
    """scikit-learn estimator saved with joblib.

    ``save`` writes ``model.joblib`` and, when a preprocessor is held,
    ``preprocessor.joblib``; ``load`` reads both, the preprocessor only if
    its file exists.
    """

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """Create a scikit-learn interface.

        Args:
            model: the estimator. Omitted, the interface has no model until
                ``load``.
            preprocessor: an optional object saved and loaded beside the
                model.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.save()``."""
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.load()``."""
        ...

class XgboostInterface(ModelInterface):
    """XGBoost sklearn-API model saved with joblib.

    Same file layout as ``SklearnInterface``.
    """

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """As ``SklearnInterface.__init__()``."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.save()``."""
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.load()``."""
        ...

class LightgbmInterface(ModelInterface):
    """LightGBM sklearn-API model saved with joblib.

    Same file layout as ``SklearnInterface``.
    """

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """As ``SklearnInterface.__init__()``."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.save()``."""
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.load()``."""
        ...

class CatboostInterface(ModelInterface):
    """CatBoost model saved with joblib.

    Same file layout as ``SklearnInterface``.
    """

    has_model: bool
    framework_version: str
    model_subtype: str | None

    def __init__(self, *, model: Any = ..., preprocessor: Any = ...) -> None:
        """As ``SklearnInterface.__init__()``."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.save()``."""
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """As ``ModelInterface.load()``."""
        ...

class TorchInterface(ModelInterface):
    """PyTorch module saved as its state dict.

    A held preprocessor is saved to ``preprocessor.joblib`` and loaded back
    when that file exists.
    """

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
            model: the ``torch.nn.Module``. Omitted, the interface has no
                model; a safetensors ``load`` then fails.
            preprocessor: an optional object saved and loaded beside the
                model.
            save_format: ``"safetensors"`` (default) or ``"pickle"``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``save_format``.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Save the module's state dict.

        ``"safetensors"`` writes ``model.safetensors``; ``"pickle"`` writes
        ``model.pt`` with ``torch.save``.

        Args:
            path: local card directory.
            save_kwargs: ignored.

        Raises:
            WyrdError: As ``ModelInterface.save()``.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the saved state dict.

        ``"safetensors"`` loads ``model.safetensors`` into the held module
        with ``load_state_dict``. ``"pickle"`` reads ``model.pt`` with
        ``torch.load(weights_only=True, map_location="cpu")`` and holds the
        result, a state dict, as ``model``.

        Args:
            path: local card directory.
            load_kwargs: ignored.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` for a safetensors load
                with no held module, or as ``ModelInterface.load()``.
        """
        ...

class LightningInterface(ModelInterface):
    """PyTorch Lightning module saved as a ``model.ckpt`` checkpoint.

    A held preprocessor is saved to ``preprocessor.joblib`` and loaded back
    when that file exists.
    """

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
            model: a ``LightningModule`` instance, or for ``load`` its class.
            trainer: trainer used to write the checkpoint. Omitted, ``save``
                writes ``{"state_dict": ...}`` with ``torch.save``.
            preprocessor: an optional object saved and loaded beside the
                model.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write ``model.ckpt``, with ``trainer.save_checkpoint`` when a trainer is held.

        Args:
            path: local card directory.
            save_kwargs: ignored.

        Raises:
            WyrdError: As ``ModelInterface.save()``.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load ``model.ckpt``.

        When ``model`` is a class, its ``load_from_checkpoint`` result
        replaces it; when it is an instance, the checkpoint's ``state_dict``
        is loaded into it.

        Args:
            path: local card directory.
            load_kwargs: ignored.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` if no module class or
                instance is held, or as ``ModelInterface.load()``.
        """
        ...

class TensorflowInterface(ModelInterface):
    """TensorFlow or Keras model.

    A held preprocessor is saved to ``preprocessor.joblib`` and loaded back
    when that file exists.
    """

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
            model: the model. Omitted, the interface has no model until
                ``load``.
            preprocessor: an optional object saved and loaded beside the
                model.
            save_format: ``"keras"`` (default) or ``"savedmodel"`` (also
                spelled ``"saved_model"``).

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``save_format``.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write ``model.keras``, or export a ``savedmodel`` directory.

        Args:
            path: local card directory.
            save_kwargs: ignored.

        Raises:
            WyrdError: As ``ModelInterface.save()``.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Read ``model.keras`` or the ``savedmodel`` directory.

        Args:
            path: local card directory.
            load_kwargs: ignored.

        Raises:
            WyrdError: As ``ModelInterface.load()``.
        """
        ...

class HuggingfaceInterface(ModelInterface):
    """Hugging Face ``transformers`` model saved with ``save_pretrained``.

    The model and any held processor are written to ``model/`` under the
    card directory.
    """

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
        hf_task: str,
        processor: Any = ...,
        repo_id: str | None = ...,
        revision: str | None = ...,
    ) -> None:
        """Create a Hugging Face interface.

        Args:
            model: the pretrained model. Omitted, the interface has no model
                until ``load``.
            hf_task: required task token, such as ``"text_classification"``,
                ``"text_generation"``, ``"feature_extraction"``, or
                ``"other"``; hyphens are accepted in place of underscores.
                It selects the ``AutoModel*`` class ``load`` uses.
            processor: tokenizer, processor, or feature extractor saved with
                the model.
            repo_id: Hub repository id recorded in the card; must not be
                empty.
            revision: pinned Hub commit recorded in the card.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``hf_task``. ``repo_id`` and ``revision`` are checked
                when the ModelCard is validated: an empty ``repo_id`` raises
                ``WYRD_MODEL_400_VALIDATION`` and a malformed ``revision``
                ``WYRD_MODEL_400_HF_REVISION_INVALID``.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write the model and processor to ``model/`` with ``save_pretrained``.

        Args:
            path: local card directory.
            save_kwargs: ignored.

        Raises:
            WyrdError: As ``ModelInterface.save()``.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load the model from ``model/`` with the task's ``AutoModel*`` class.

        The processor is restored with ``AutoProcessor``, falling back to
        ``AutoTokenizer``; it is ``None`` when neither loads.

        Args:
            path: local card directory.
            load_kwargs: ignored.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` if the installed
                ``transformers`` lacks the task's auto class, or as
                ``ModelInterface.load()``.
        """
        ...

class ModelSignature:
    """Ordered input and output fields of a ModelCard."""

    inputs: list[FieldSpec]
    outputs: list[FieldSpec]

    def __init__(
        self,
        inputs: Sequence[FieldSpec | Mapping[str, Any]],
        outputs: Sequence[FieldSpec | Mapping[str, Any]],
    ) -> None:
        """Create and validate a signature from explicit fields.

        Args:
            inputs: input fields, as ``FieldSpec`` objects or serialized
                field dictionaries.
            outputs: output fields, in the same forms.

        Raises:
            WyrdError: ``WYRD_MODEL_400_MISSING_SIGNATURE`` if either side is
                empty, ``WYRD_MODEL_400_VALIDATION`` for duplicate field
                names, ``WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED`` for a
                non-canonical dtype, or ``WYRD_MODEL_400_SHAPE_INVALID`` for a
                non-positive fixed dimension.
        """
        ...

    @classmethod
    def from_data(cls, *, inputs: Any, outputs: Any) -> ModelSignature:
        """Infer a signature from sample input and output objects.

        Each side may be a pandas or polars DataFrame, a ``pyarrow.Table``, a
        NumPy array or Torch tensor (first dimension treated as the dynamic
        batch axis), a ``dict`` of names to NumPy arrays, a ``list[str]``, or
        a ``str``.

        Args:
            inputs: sample model input.
            outputs: sample model output.

        Raises:
            WyrdError: ``WYRD_MODEL_400_MISSING_SIGNATURE`` for an
                unsupported object, or the errors of ``ModelSignature()``.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this signature as a JSON-compatible spec dictionary."""
        ...

class SampleInput:
    """Example model input stored beside the model artifact.

    ``kind_token`` is the canonical kind and ``has_value`` reports whether a
    live value is held. ``save`` writes ``sample_input.parquet`` (pandas,
    polars, arrow), ``sample_input.npy`` (numpy, tf),
    ``sample_input.safetensors`` (torch), ``sample_input.json`` (dict, list,
    tuple), or ``sample_input.txt`` (str); kind ``none`` writes nothing.
    """

    kind_token: str
    has_value: bool

    def __init__(self, *, kind: str = ..., value: Any = ...) -> None:
        """Create a sample input.

        Args:
            kind: ``"pandas"``, ``"polars"``, ``"arrow"``, ``"numpy"``,
                ``"torch"``, ``"tf"``, ``"dict"``, ``"list"``, ``"tuple"``,
                ``"str"``, or ``"none"`` (default).
            value: the sample value. Omitted, nothing is held until ``load``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``kind``.
        """
        ...

    @classmethod
    def from_python_object(cls, value: Any) -> SampleInput:
        """Create a sample input whose kind is detected from ``value``.

        ``None`` gives kind ``none``.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` if ``value`` is not a
                supported sample type.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this sample input as a JSON-compatible spec dictionary."""
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Write the held value to the kind's file under ``path``.

        Args:
            path: local card directory.
            save_kwargs: ignored.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` if a kind other than
                ``none`` holds no value, or another ``WyrdError`` if writing
                fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Read the kind's file under ``path`` and hold its value.

        Args:
            path: local card directory.
            load_kwargs: ignored.

        Raises:
            WyrdError: If the file is missing or cannot be read.
        """
        ...

class ModelCardMetadata:
    """Interface, task, signature, sample-input, and artifact-reference
    metadata a ModelCard turns into a durable Model spec.

    ``task_type`` and ``interface_kind`` read back typed values; ``to_dict()``
    returns the whole serialized spec."""

    @property
    def task_type(self) -> str:
        """The task type token, as accepted by ``task_type=``, such as ``"regression"``."""
        ...
    @property
    def interface_kind(self) -> str:
        """The stored interface kind, such as ``"Sklearn"``, or ``"Custom"`` for a subclass."""
        ...

    def __init__(
        self,
        *,
        interface: ModelInterface | JsonDict | None = ...,
        task_type: str = ...,
        signature: ModelSignature | JsonDict | None = ...,
        sample_input: SampleInput | JsonDict | None = ...,
        card_refs: Sequence[CardRefLike] | None = ...,
    ) -> None:
        """Create ModelCard metadata; every argument is keyword-only.

        Values are parsed, not validated; ``ModelCard`` validates the
        resulting spec.

        Args:
            interface: an interface instance or serialized interface
                metadata. ``ModelCard`` replaces it with its own interface.
            task_type: ``"binary_classification"``,
                ``"multi_class_classification"``, ``"regression"``,
                ``"clustering"``, ``"anomaly_detection"``, ``"forecasting"``,
                ``"generation"``, or ``"other"`` (default).
            signature: a ``ModelSignature`` or its serialized form. Omitted,
                the signature is empty, which ``ModelCard`` rejects.
            sample_input: a ``SampleInput`` or its serialized form. Omitted,
                none is recorded.
            card_refs: serialized references to Artifact cards holding the
                model bytes. Omitted, none are recorded.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``task_type``, or a ``WyrdError`` if a serialized
                value cannot be parsed.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this metadata as a JSON-compatible spec dictionary."""
        ...

class ModelCard:
    """Local ModelCard holder and spec builder.

    A ModelCard holds identity, labels, annotations, metadata, and a live
    model interface. It never registers itself; registration and retrieval
    belong to ``Cards``:

    ```python
    cards = Cards()
    card = cards.model.get(
        space="ml", name="fraud-model", interface=MyModelInterface, eager_load=True
    )
    ```

    ``get`` returns the validated envelope without model bytes unless
    ``eager_load=True``, which downloads the artifacts and calls ``load``.
    ``model``, ``preprocessor``, and ``processor`` read through to the held
    interface and are ``None`` when it has none. Setting ``metadata``
    re-validates the spec.
    """

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    metadata: ModelCardMetadata
    interface: ModelInterface | None
    model: Any | None
    preprocessor: Any | None
    processor: Any | None
    task_type: str
    signature: ModelSignature
    sample_input: SampleInput | None

    @property
    def artifact_hash(self) -> str | None:
        """The registered Card's artifact manifest hash, read from its envelope.

        The registry derives it at registration; it is never computed
        locally. Pass it as ``WyrdState.from_path(trusted_artifact_hashes=...)``
        to load an executable model. ``None`` for a locally authored card.
        """
        ...

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
        """Create a ModelCard from an initialized model interface.

        The resulting spec is validated, so ``metadata`` must carry a
        non-empty signature; the constructor never infers one.

        Args:
            model_or_interface: a built-in interface or subclass instance. An
                interface class is rejected; pass classes to
                ``Cards.model.get``.
            space: card space. Omitted, the nearest ``wyrd.toml`` default
                applies, else ``"default"``.
            name: card name. Defaults to ``"model"``.
            version: semantic version. Defaults to ``"0.1.0"``.
            uid: card UID. Omitted, a new UUIDv7 is generated.
            labels: queryable labels. Defaults from ``wyrd.toml`` fill keys
                not given here.
            annotations: free-form annotations. Defaults from ``wyrd.toml``
                fill keys not given here.
            metadata: task type, signature, sample input, and card
                references. Its interface is replaced from
                ``model_or_interface``.

        Raises:
            WyrdError: ``WYRD_MODEL_400_MISSING_SIGNATURE`` if the signature is
                missing or empty, ``WYRD_MODEL_400_VALIDATION`` for an
                interface class or an invalid label, annotation, or
                interface, or the other signature errors of
                ``ModelSignature()``.
        """
        ...

    @overload
    def __init__(
        self,
        model_or_interface: object,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: ModelCardMetadata | None = ...,
    ) -> None:
        """Create a ModelCard by detecting the interface for a model object.

        Detection checks, in order, Hugging Face pretrained models (task
        ``"other"``), Lightning modules, Torch modules (``"pickle"`` save
        format), Keras models, XGBoost, LightGBM, CatBoost, and scikit-learn
        estimators.

        Args:
            model_or_interface: the model object.
            space: as for the interface overload.
            name: as for the interface overload.
            version: as for the interface overload.
            uid: as for the interface overload.
            labels: as for the interface overload.
            annotations: as for the interface overload.
            metadata: as for the interface overload; it must still carry a
                signature.

        Raises:
            WyrdError: ``WYRD_MODEL_400_UNKNOWN_MODEL_TYPE`` if no interface
                matches, or the errors of the interface overload.
        """
        ...

    def save(self, path: PathLike, save_kwargs: Mapping[str, JsonValue] | None = ...) -> None:
        """Write the model artifacts and ``card.json`` to a local directory.

        Validates the spec, has the interface write its files, and records the
        updated interface metadata. Local only: nothing is uploaded or
        registered, and ``card_refs`` is not changed.

        Args:
            path: local card directory.
            save_kwargs: a ``dict`` forwarded to the interface's ``save``.

        Raises:
            WyrdError: If the spec or identity is invalid, the errors of the
                interface's ``save``, or a ``WyrdError`` if ``card.json``
                cannot be written.
        """
        ...

    def load(
        self,
        path: PathLike | None = ...,
        load_kwargs: ModelLoadArgs | Mapping[str, JsonValue] | None = ...,
    ) -> None:
        """Read model artifacts into the held interface from a local directory.

        Args:
            path: local card directory. Omit it only to reload a card
                returned by ``Cards.model.get(eager_load=True)``, which
                reuses the artifacts that call downloaded.
            load_kwargs: options forwarded to the interface's ``load``.

        Raises:
            WyrdError: ``WYRD_MODEL_400_VALIDATION`` if ``path`` is omitted
                without eager-loaded artifacts or no interface is attached,
                or the errors of the interface's ``load``.
        """
        ...

    def model_dump_json(self) -> str:
        """Return the ModelCard envelope as a JSON string.

        Raises:
            WyrdError: If identity fields or the spec are invalid.
        """
        ...

    def model_dump(self) -> JsonDict:
        """Return the ModelCard envelope as a JSON-compatible dictionary.

        Raises:
            WyrdError: If identity fields or the spec are invalid.
        """
        ...

    def _to_card_envelope_json(self) -> str:
        """Return the registry adapter's single envelope conversion."""
        ...

    def as_card_ref(self) -> CardRef:
        """Return a ``Model`` CardRef for this card's identity.

        Raises:
            WyrdError: If name, version, space, or UID is invalid.
        """
        ...

    @staticmethod
    def model_validate_json(
        json_string: str,
        interface: ModelInterface | type[ModelInterface] | None = ...,
    ) -> ModelCard:
        """Rebuild a ModelCard from serialized card JSON.

        The JSON must be a complete ``Model`` envelope with a resolved
        version. No model artifacts are downloaded or read.

        Args:
            json_string: the serialized envelope.
            interface: an interface instance, or an interface class rebuilt
                with its ``from_metadata``. It must match the stored
                interface. Omitted, built-in interfaces are rebuilt from the
                stored metadata; a card with a custom interface requires this
                argument.

        Raises:
            WyrdError: If the JSON or envelope is invalid, the stored
                interface is custom and ``interface`` is omitted, or
                ``interface`` does not match the stored interface.
        """
        ...

    @staticmethod
    def from_path(
        path: PathLike,
        interface: ModelInterface | type[ModelInterface] | None = ...,
        load_kwargs: ModelLoadArgs | Mapping[str, JsonValue] | None = ...,
    ) -> ModelCard:
        """Load a ModelCard from a saved Card directory or a Card YAML/JSON file.

        A directory is read through the `card.json` that `save` wrote, and its
        model is loaded through the interface. A file is parsed as one Card
        envelope; no model is loaded.

        Raises:
            WyrdError: If the file cannot be read, is not a Model Card
                envelope, or interface loading fails.
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
