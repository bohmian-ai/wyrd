# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# ruff: noqa: F811
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
### header.pyi ###
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value, missing-final-newline
# ruff: noqa: F401

from __future__ import annotations

import datetime
import os
import pathlib
from collections.abc import Mapping, Sequence
from typing import Any, Protocol, TypeAlias, overload

PathLike: TypeAlias = str | os.PathLike[str] | pathlib.Path
JsonDict: TypeAlias = dict[str, Any]
StringMap: TypeAlias = Mapping[str, str]

class CardRefLike(Protocol):
    """Object that can be represented as a Wyrd card reference."""

    def to_dict(self) -> JsonDict:
        """Return a JSON-compatible card reference dictionary."""

### error.pyi ###
class WyrdError(Exception):
    """Python-facing Wyrd error with stable metadata.

    Wyrd raises this exception for validation and boundary failures that have a
    durable Wyrd error code. The attributes are intended for both humans and
    agents: `code` is stable, `message` explains the failure, `details` carries
    structured context, and `remediation` tells the caller what to change next.
    """

    code: str
    message: str
    detail: str
    details: dict[str, Any] | None
    remediation: str

    def __init__(
        self,
        code: str,
        message: str,
        *,
        details: dict[str, Any] | None = None,
        remediation: str = ...,
    ) -> None:
        """Create a Wyrd error.

        Users normally receive this from Wyrd rather than constructing it
        directly. `code` is the machine-stable identifier; `message` is the
        short human-readable failure; `details` is JSON-compatible context; and
        `remediation` is the actionable recovery hint.

        Args:
            code (str): Stable Wyrd error code.
            message (str): Human-readable failure message.
            details (dict[str, Any] | None): Optional structured context for
                the failure.
            remediation (str): Actionable recovery guidance.
        """
        ...

### agent.pyi ###
class Agent:
    """Declarative and runnable Wyrd Agent."""

    def __init__(
        self,
        *,
        prompt: Prompt | Mapping[str, Any],
        name: str | None = ...,
        version: str | None = ...,
        space: str | None = ...,
        id: str | None = ...,
        tools: Sequence[Any] | None = ...,
        providers: Any | None = ...,
        run_config: Any | None = ...,
        before_agent_callback: Any | None = ...,
        after_agent_callback: Any | None = ...,
        before_model_callback: Any | None = ...,
        after_model_callback: Any | None = ...,
        before_tool_callback: Any | None = ...,
        after_tool_callback: Any | None = ...,
        session: Any | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
    ) -> None: ...

    @property
    def name(self) -> str | None: ...
    @property
    def version(self) -> str | None: ...
    @property
    def space(self) -> str | None: ...
    @property
    def id(self) -> str: ...
    @property
    def prompt(self) -> Prompt: ...
    @property
    def provider(self) -> str: ...
    @property
    def model(self) -> str: ...
    @property
    def tool_names(self) -> list[str]: ...
    def save(self, path: PathLike) -> None: ...
    @staticmethod
    def from_yaml(path: PathLike) -> Agent: ...
    def to_yaml_string(self) -> str: ...
    def to_card(self) -> dict[str, Any]: ...
    def model_dump_json(self) -> str: ...
    @staticmethod
    def model_validate_json(data: str) -> Agent: ...
    def validate_registrable(self) -> None: ...
    def run(self, input: str | Mapping[str, Any], *, session_id: str | None = ...) -> Any: ...
    def as_tool(self, *, description: str | None = ...) -> Any: ...

### data.pyi ###
class FieldSpec:
    """One column or tensor field in a DataCard schema.

    A field records the durable name, normalized dtype, optional shape
    dimensions, nullability, and small string metadata. Constructing a
    `FieldSpec` validates the field name but does not inspect data or touch the
    filesystem.
    """

    name: str
    dtype: str
    shape: list[dict[str, Any]]
    dims: list[dict[str, Any]]
    nullable: bool
    extra: dict[str, str]

    def __init__(
        self,
        name: str,
        dtype: str,
        shape: Sequence[Mapping[str, Any]] | None = ...,
        nullable: bool = ...,
        extra: StringMap | None = ...,
    ) -> None:
        """Declare a schema field.

        Args:
            name (str): Column or field name stored in the Wyrd spec.
            dtype (str): Normalized dtype label, for example `int64` or
                `string`.
            shape (Sequence[Mapping[str, Any]] | None): Optional serialized
                dimensions for tensor-like data.
            nullable (bool): Whether the field may contain null values.
            extra (StringMap | None): Small string metadata copied into the
                field spec.

        Raises:
            WyrdError: If `name` is not a valid Wyrd column name or the shape
                cannot be parsed as serialized dimensions.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this field as a JSON-compatible Wyrd spec dictionary."""
        ...

class DataSchema:
    """Ordered schema for a DataCard.

    `DataSchema` is local metadata. It can be supplied by a caller or inferred
    from a live interface when the backing library exposes enough type
    information.
    """

    columns: list[FieldSpec]
    fields: list[FieldSpec]

    def __init__(self, columns: Sequence[FieldSpec | Mapping[str, Any]] | None = ...) -> None:
        """Create a schema from ordered fields.

        Args:
            columns (Sequence[FieldSpec | Mapping[str, Any]] | None): Existing
                `FieldSpec` objects or serialized field dictionaries. Omit
                this for an empty schema.

        Raises:
            WyrdError: If any serialized field is invalid.
        """
        ...

    def is_empty(self) -> bool:
        """Return `True` when the schema has no fields."""
        ...

    def contains_column(self, name: str) -> bool:
        """Return whether `name` is present in this schema.

        Args:
            name (str): Column name to look up.

        Raises:
            WyrdError: If `name` is not a valid Wyrd column name.
        """
        ...

    def column(self, name: str) -> FieldSpec | None:
        """Return the field named `name`, or `None` when it is absent.

        Args:
            name (str): Column name to return.

        Raises:
            WyrdError: If `name` is not a valid Wyrd column name.
        """
        ...

    def column_names(self) -> list[str]:
        """Return schema field names in order."""
        ...

    def to_dict(self) -> JsonDict:
        """Return this schema as a JSON-compatible Wyrd spec dictionary."""
        ...

class DataStats:
    """Byte and shape statistics for a local DataCard artifact."""

    byte_count: int
    sha256: str
    row_count: int | None
    col_count: int | None

    def __init__(
        self,
        byte_count: int,
        sha256: str,
        row_count: int | None = ...,
        col_count: int | None = ...,
    ) -> None:
        """Create artifact statistics.

        Args:
            byte_count (int): Number of bytes materialized for the local
                artifact.
            sha256 (str): Hex SHA-256 digest for the materialized artifact
                bytes.
            row_count (int | None): Optional row count when the interface can
                infer it.
            col_count (int | None): Optional column count when the interface
                can infer it.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return these statistics as a JSON-compatible Wyrd spec dictionary."""
        ...

class DataInterface:
    """Base class for Python data interfaces.

    Subclass this class for custom Python-only materialization. A subclass must
    override `save` and `load`; the base methods raise `WyrdError` so missing
    implementations fail before a card silently records unusable artifacts.
    """

    kind: str

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        """Initialize a custom interface base.

        Positional and keyword arguments are accepted so custom subclasses can
        call `super().__init__(...)` without matching a built-in constructor.
        The base class records the interface kind as `Custom`.

        Args:
            *args (Any): Positional arguments accepted for subclass
                compatibility.
            **kwargs (Any): Keyword arguments accepted for subclass
                compatibility.
        """
        ...

    @property
    def has_source(self) -> bool:
        """Return whether this interface currently holds live Python data."""
        ...

    @classmethod
    def from_metadata(cls, metadata: DataCardMetadata) -> DataInterface:
        """Build an interface instance from serialized DataCard metadata.

        Registry and client retrieval surfaces call this hook when a user
        passes a custom interface class, such as
        `wyrd.cards.get(..., interface=MyInterface)`. The default
        implementation constructs the subclass with no arguments. Override
        this method when an interface needs metadata values to reconstruct
        local configuration before `DataCard.load(...)` hydrates data.

        Args:
            metadata (DataCardMetadata): Metadata parsed from the serialized
                DataCard envelope.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return interface metadata as a JSON-compatible dictionary.

        The dictionary is the metadata stored in the DataCard spec. It does not
        include live Python objects.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> DataStats:
        """Write this interface's data into a local DataCard directory.

        Built-in interfaces write to their convention path under `path`, such
        as `data/data.parquet` or `data/manifest.json`. Custom subclasses must
        implement the same contract and return `DataStats` for the bytes they
        wrote.

        Args:
            path (PathLike): Local DataCard materialization directory.
            save_kwargs (dict[str, Any] | None): Optional interface-specific
                save options.

        Raises:
            WyrdError: If the interface has no live source data, the source
                object is not supported, an option is invalid, or local IO
                fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Load this interface's data from a local DataCard directory.

        The interface reconstructs its convention path from `path`; Wyrd does
        not store a local absolute path in the card JSON.

        Args:
            path (PathLike): Local DataCard materialization directory.
            load_kwargs (dict[str, Any] | None): Optional interface-specific
                load options.

        Raises:
            WyrdError: If the expected artifact is missing, deserialization
                fails, remote loading is not explicitly allowed, or local IO
                fails.
        """
        ...

class PandasInterface(DataInterface):
    """Data interface for pandas DataFrames saved as parquet."""

    def __init__(self, *, data: Any = ..., compression: str = ...) -> None:
        """Create a pandas interface.

        Args:
            data (Any): Optional pandas `DataFrame` to materialize during
                `save`.
            compression (str): Parquet codec: `none`, `snappy`, `gzip`,
                `zstd`, or `lz4`.
        """
        ...

class PolarsInterface(DataInterface):
    """Data interface for polars DataFrames saved as parquet."""

    def __init__(self, *, data: Any = ..., compression: str = ...) -> None:
        """Create a polars interface.

        Args:
            data (Any): Optional polars `DataFrame` to materialize during
                `save`.
            compression (str): Parquet codec: `none`, `snappy`, `gzip`,
                `zstd`, or `lz4`.
        """
        ...

class ArrowInterface(DataInterface):
    """Data interface for PyArrow tables saved as parquet or IPC."""

    def __init__(self, *, data: Any = ..., format: str = ...) -> None:
        """Create an Arrow interface.

        Args:
            data (Any): Optional `pyarrow.Table` to materialize during `save`.
            format (str): Serialization format: `parquet` or `ipc`.
        """
        ...

class ParquetInterface(DataInterface):
    """Data interface for parquet paths or table-like parquet sources."""

    def __init__(
        self,
        *,
        data: Any = ...,
        compression: str = ...,
        row_group_size: int | None = ...,
    ) -> None:
        """Create a parquet interface.

        Args:
            data (Any): Optional parquet path, table-like object, or `None`.
            compression (str): Parquet codec for table-like writes.
            row_group_size (int | None): Optional declared row group size
                metadata.
        """
        ...

class NumpyInterface(DataInterface):
    """Data interface for NumPy arrays saved as `.npy` or `.npz`."""

    def __init__(
        self,
        *,
        data: Any = ...,
        dtype: str | None = ...,
        shape: Sequence[int] | None = ...,
        format: str = ...,
    ) -> None:
        """Create a NumPy interface.

        Args:
            data (Any): Optional NumPy ndarray to materialize during `save`.
            dtype (str | None): Optional dtype. If omitted, Wyrd infers it
                from `data`.
            shape (Sequence[int] | None): Optional array shape. If omitted,
                Wyrd infers it from `data`.
            format (str): Serialization format: `npy` or `npz`.
        """
        ...

class TorchInterface(DataInterface):
    """Data interface for Torch tensors or mappings of tensors."""

    def __init__(self, *, data: Any = ..., save_format: str = ...) -> None:
        """Create a Torch interface.

        Args:
            data (Any): Optional Torch tensor or tensor mapping.
            save_format (str): Serialization format: `safetensors` or
                `pickle`.
        """
        ...

class SqlInterface(DataInterface):
    """Data interface for SQL query bundles."""

    def __init__(
        self,
        *,
        data: Any = ...,
        dialect: str,
        connection_hint: str | None = ...,
    ) -> None:
        """Create a SQL interface.

        Args:
            data (Any): Optional query mapping or JSON-compatible SQL logic.
            dialect (str): SQL dialect label recorded in the DataCard spec.
            connection_hint (str | None): Optional human-readable connection
                hint. Wyrd records it as metadata and does not connect to a
                database.
        """
        ...

class JsonlInterface(DataInterface):
    """Data interface for JSON Lines rows or JSONL files."""

    def __init__(
        self,
        *,
        data: Any = ...,
        compression: str = ...,
        lines_per_file: int | None = ...,
    ) -> None:
        """Create a JSON Lines interface.

        Args:
            data (Any): Optional iterable of JSON-compatible rows, JSONL path,
                or `None`.
            compression (str): JSONL compression mode: `none`, `gzip`, or
                `zstd`.
            lines_per_file (int | None): Optional declared line-count
                partition size.
        """
        ...

class ImageInterface(DataInterface):
    """Data interface for image manifests."""

    def __init__(
        self,
        *,
        data: Any = ...,
        format: str = ...,
        color_mode: str = ...,
        manifest_ref: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create an image manifest interface.

        Args:
            data (Any): Optional image file, image directory, iterable
                manifest, or serialized manifest-like value.
            format (str): Image format family: `png`, `jpeg`, `webp`, or
                `mixed`.
            color_mode (str): Declared color mode: `rgb`, `rgba`, or
                `grayscale`.
            manifest_ref (Mapping[str, Any] | None): Optional CardRef for an
                external manifest card.
        """
        ...

class TextInterface(DataInterface):
    """Data interface for text manifests."""

    def __init__(
        self,
        *,
        data: Any = ...,
        encoding: str = ...,
        manifest_ref: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create a text manifest interface.

        Args:
            data (Any): Optional text file, text directory, iterable manifest,
                or serialized manifest-like value.
            encoding (str): Text encoding label recorded in the DataCard spec.
            manifest_ref (Mapping[str, Any] | None): Optional CardRef for an
                external manifest card.
        """
        ...

class HuggingfaceInterface(DataInterface):
    """Data interface for local Hugging Face datasets or pinned dataset pointers."""

    def __init__(
        self,
        *,
        data: Any = ...,
        dataset_id: str,
        revision: str | None = ...,
        split: str | None = ...,
        config: str | None = ...,
    ) -> None:
        """Create a Hugging Face dataset interface.

        Args:
            data (Any): Optional Hugging Face dataset object to materialize
                locally.
            dataset_id (str): Dataset identifier recorded in the DataCard
                spec.
            revision (str | None): Optional pinned dataset revision.
                Pointer-only saves require a revision.
            split (str | None): Optional dataset split.
            config (str | None): Optional dataset config name.
        """
        ...

class Split:
    """Builder for a DataCard split strategy."""

    strategy: JsonDict

    @staticmethod
    def column(col: str, op: str, value: Any) -> Split:
        """Declare a split using a column predicate.

        Args:
            col (str): Column name to evaluate.
            op (str): Predicate operator: `==`, `!=`, `<`, `<=`, `>`, `>=`,
                or `in`.
            value (Any): Predicate value. Lists are accepted only for `in`.

        Raises:
            WyrdError: If the column name, operator, or value cannot be encoded
                as a Wyrd split strategy.
        """
        ...

    @staticmethod
    def materialized(artifact_ref: Mapping[str, Any] | CardRefLike) -> Split:
        """Declare a split backed by an Artifact card reference.

        Args:
            artifact_ref (Mapping[str, Any] | CardRefLike): Mapping or object
                that serializes to a CardRef with `kind` set to `Artifact`.
        """
        ...

    @staticmethod
    def index_range(start: int, stop: int) -> Split:
        """Declare a non-negative half-open index range split.

        Args:
            start (int): Inclusive starting index.
            stop (int): Exclusive stopping index. Must be greater than or
                equal to `start`.
        """
        ...

    @staticmethod
    def indices(values: Sequence[int]) -> Split:
        """Declare a split from explicit row indices.

        Args:
            values (Sequence[int]): Non-empty sequence of non-negative row
                indices. Duplicate values are preserved for spec validation.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this split strategy as a JSON-compatible dictionary."""
        ...

class ArtifactCard:
    """Minimal Artifact card reference holder accepted by DataCard.

    This PR1 holder exists so a DataCard can point at bytes that are already
    durable without creating or uploading a new artifact during local save.
    """

    space: str
    name: str
    version: str
    uid: str

    def __init__(
        self,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
    ) -> None:
        """Create an Artifact card reference holder.

        Args:
            space (str | None): Optional artifact space. Defaults to
                `default`.
            name (str | None): Optional artifact name. Defaults to `artifact`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional artifact UID. Defaults to a generated
                UID.
        """
        ...

class DataCardMetadata:
    """Python holder metadata used to build a durable DataCard spec."""

class DataCard:
    """Local DataCard holder and spec builder.

    A DataCard owns local identity, labels, annotations, schema metadata, and an
    optional live data interface. `save` and `load` only operate on the local
    filesystem. Registration belongs to registry/client APIs, not the card.
    """

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    metadata: DataCardMetadata
    interface: DataInterface | None
    schema: DataSchema
    stats: DataStats

    @overload
    def __init__(
        self,
        data: DataInterface,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: DataCardMetadata | None = ...,
    ) -> None:
        """Create a DataCard from an explicit data interface.

        Args:
            data (DataInterface): Built-in interface or Python subclass that
                owns local save/load behavior.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `data`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated UID.
            labels (StringMap | None): Queryable user labels copied into the
                card metadata.
            annotations (StringMap | None): Free-form user annotations copied
                into the card metadata.
            metadata (DataCardMetadata | None): Existing holder metadata to
                seed before interface inference.

        Raises:
            WyrdError: If labels, annotations, interface metadata, or inferred
                schema data violate the DataCard contract.
        """
        ...

    @overload
    def __init__(
        self,
        data: PathLike | Mapping[str, Any],
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: DataCardMetadata | None = ...,
    ) -> None:
        """Create a DataCard from a local path or SQL query mapping.

        Args:
            data (PathLike | Mapping[str, Any]): Local path to infer as a data
                interface, or a SQL query mapping.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `data`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated UID.
            labels (StringMap | None): Queryable user labels copied into the
                card metadata.
            annotations (StringMap | None): Free-form user annotations copied
                into the card metadata.
            metadata (DataCardMetadata | None): Existing holder metadata to
                seed before interface inference.

        Raises:
            WyrdError: If Wyrd cannot infer a supported data interface or the
                supplied metadata violates the DataCard contract.
        """
        ...

    @overload
    def __init__(
        self,
        data: Any,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: DataCardMetadata | None = ...,
    ) -> None:
        """Create a DataCard by inferring the interface from runtime data.

        Args:
            data (Any): Runtime object such as a pandas DataFrame, polars
                DataFrame, PyArrow table, NumPy array, Torch tensor, SQL
                mapping, or supported local path.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `data`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated UID.
            labels (StringMap | None): Queryable user labels copied into the
                card metadata.
            annotations (StringMap | None): Free-form user annotations copied
                into the card metadata.
            metadata (DataCardMetadata | None): Existing holder metadata to
                seed before interface inference.

        Raises:
            WyrdError: If Wyrd cannot infer a supported interface or the
                inferred schema data violates the DataCard contract.
        """
        ...

    @property
    def data(self) -> Any:
        """Return live local data from the held interface.

        Raises:
            WyrdError: If no interface is attached or the interface has no live
                Python source data.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> None:
        """Materialize local data artifacts and write `card.json`.

        This is a local filesystem operation only. It updates interface
        metadata and byte statistics, then writes the DataCard envelope. It
        does not create ArtifactCards, upload bytes, or register the card.

        Args:
            path (PathLike): Local directory where Wyrd writes artifact bytes
                and `card.json`.
            save_kwargs (dict[str, Any] | None): Optional interface-specific
                save options.
        """
        ...

    def load(self, path: PathLike | None = ..., load_kwargs: dict[str, Any] | None = ...) -> None:
        """Hydrate local data through the held interface.

        Pass `path` when loading from a saved local directory. The interface
        reconstructs its convention path under that directory.

        Args:
            path (PathLike | None): Local materialization directory. Pass
                `None` only when another surface has already provided local
                data for the interface.
            load_kwargs (dict[str, Any] | None): Optional interface-specific
                load options.
        """
        ...

    def model_dump_json(self) -> str:
        """Return this DataCard envelope as JSON without filesystem IO."""
        ...

    @staticmethod
    def model_validate_json(json_string: str, interface: Any = ...) -> DataCard:
        """Build a DataCard from serialized Wyrd card JSON.

        Args:
            json_string (str): Serialized DataCard envelope.
            interface (Any): Optional built-in interface, Python subclass
                instance, Python subclass type reconstructed through
                `from_metadata`, or ArtifactCard to attach after parsing.
        """
        ...

### model.pyi ###
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
            task_type (str): model task type token stored in the Model spec.
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

### prompt.pyi ###
class MediaRef:
    """Reference to media content used by `Prompt.bind_media`.

    Create a media reference with one of the static constructors and bind it
    to a prompt placeholder written as `${media:name}`. Text placeholders
    written as `{{name}}` are unaffected by media binding; the two token
    namespaces are disjoint.

    Provider support depends on media kind and source. OpenAI accepts image
    URLs, image base64 data, image files, document base64 data, and document
    files, but rejects document URLs. Anthropic accepts image and document
    URLs, base64 data, and file ids. Gemini and Vertex accept inline base64
    data and file data; URL sources must be `gs://` or Gemini file API URIs
    with an explicit MIME type.

    Examples:
        >>> from wyrd import MediaRef, Prompt
        >>> prompt = Prompt("see: ${media:logo}", "gpt-4o", provider="openai")
        >>> bound = prompt.bind_media("logo", MediaRef.image_url("https://x/logo.png"))
        >>> assert bound.media_variables == []
    """

    kind: str
    source_type: str

    @staticmethod
    def image_url(url: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct an image reference from a provider-accessible URL.

        Args:
            url (str): Remote image URL passed to the provider.
            mime_type (str | None): Optional MIME type. Required when `url`
                is a Gemini or Vertex `gs://` reference or Gemini file API URI.

        Returns:
            MediaRef: Image reference with `source_type == "url"`.
        """
        ...

    @staticmethod
    def image_bytes(mime_type: str, data: bytes) -> MediaRef:
        """Construct an image reference from raw bytes.

        The bytes are base64-encoded eagerly and the input bytes are not
        retained.

        Args:
            mime_type (str): Image MIME type, such as `image/png`.
            data (bytes): Raw image bytes.

        Returns:
            MediaRef: Image reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def image_base64(mime_type: str, data: str) -> MediaRef:
        """Construct an image reference from existing base64 data.

        Args:
            mime_type (str): Image MIME type, such as `image/png`.
            data (str): Base64 payload without a `data:` prefix.

        Returns:
            MediaRef: Image reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def image_file(uri: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct an image reference from a provider file id or URI.

        Args:
            uri (str): OpenAI file id, Anthropic file id, Gemini file API URI,
                or Vertex file URI.
            mime_type (str | None): Optional MIME type. Required for Gemini
                and Vertex.

        Returns:
            MediaRef: Image reference with `source_type == "file"`.
        """
        ...

    @staticmethod
    def image_path(path: PathLike) -> MediaRef:
        """Construct an image reference by eagerly reading a local file.

        MIME type is inferred from the extension. Supported image extensions
        are `png`, `jpg`, `jpeg`, `gif`, and `webp`.

        Args:
            path (PathLike): Regular local file no larger than 20 MiB.

        Returns:
            MediaRef: Image reference with `source_type == "base64"`.

        Raises:
            WyrdError: If the path is not a regular file, exceeds 20 MiB, has
                an unsupported extension, or cannot be read.
        """
        ...

    @staticmethod
    def document_url(url: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct a document reference from a provider-accessible URL.

        OpenAI rejects document URLs at bind time because its chat file part
        has no document-URL primitive.

        Args:
            url (str): Remote document URL, `gs://` URI, or Gemini file API URI.
            mime_type (str | None): Optional MIME type. Required for Gemini
                and Vertex URL sources.

        Returns:
            MediaRef: Document reference with `source_type == "url"`.

        Raises:
            WyrdError: Later binding to OpenAI raises
                `WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER`.
        """
        ...

    @staticmethod
    def document_bytes(mime_type: str, data: bytes) -> MediaRef:
        """Construct a document reference from raw bytes.

        The bytes are base64-encoded eagerly and accepted by all supported
        prompt providers.

        Args:
            mime_type (str): Document MIME type, such as `application/pdf`.
            data (bytes): Raw document bytes.

        Returns:
            MediaRef: Document reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def document_base64(mime_type: str, data: str) -> MediaRef:
        """Construct a document reference from existing base64 data.

        Args:
            mime_type (str): Document MIME type, such as `application/pdf`.
            data (str): Base64 payload without a `data:` prefix.

        Returns:
            MediaRef: Document reference with `source_type == "base64"`.
        """
        ...

    @staticmethod
    def document_file(uri: str, *, mime_type: str | None = ...) -> MediaRef:
        """Construct a document reference from a provider file id or URI.

        Args:
            uri (str): OpenAI file id, Anthropic file id, Gemini file API URI,
                or Vertex file URI.
            mime_type (str | None): Optional MIME type. Required for Gemini
                and Vertex.

        Returns:
            MediaRef: Document reference with `source_type == "file"`.
        """
        ...

    @staticmethod
    def document_path(path: PathLike) -> MediaRef:
        """Construct a document reference by eagerly reading a local file.

        MIME type is inferred from the extension. Supported document
        extensions include `pdf`, `txt`, `md`, `json`, `csv`, `html`, and
        `htm`.

        Args:
            path (PathLike): Regular local file no larger than 20 MiB.

        Returns:
            MediaRef: Document reference with `source_type == "base64"`.

        Raises:
            WyrdError: If the path is not a regular file, exceeds 20 MiB, has
                an unsupported extension, or cannot be read.
        """
        ...

    def __repr__(self) -> str:
        """Return a concise representation without payload bytes or URLs.

        Returns:
            str: Redacted media reference representation.
        """
        ...

class ProviderRequest:
    """Opaque provider-native request returned by prompt rendering."""

    provider: str
    messages: list[JsonDict]
    message: JsonDict | None
    system: Any

    def model_dump(self) -> JsonDict:
        """Return the native provider request as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible provider request.
        """
        ...

    def model_dump_json(self) -> str:
        """Return the native provider request as JSON.

        Returns:
            str: Serialized provider request JSON.
        """
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the provider request.
        """
        ...

class ResponseFormat:
    """Provider-independent response-format authoring helper."""

    @staticmethod
    def text() -> ResponseFormat:
        """Request plain-text output.

        Returns:
            ResponseFormat: Response-format helper for plain text.
        """
        ...

    @staticmethod
    def json_object() -> ResponseFormat:
        """Request provider-native JSON object output.

        Returns:
            ResponseFormat: Response-format helper for provider JSON object
                mode.
        """
        ...

    @staticmethod
    def json_schema(name: str, schema: JsonDict) -> ResponseFormat:
        """Request structured JSON output matching `schema`.

        Args:
            name (str): Provider-visible schema name.
            schema (JsonDict): JSON Schema object that the response must
                satisfy.

        Returns:
            ResponseFormat: Response-format helper for provider JSON Schema
                mode.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return the response format as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible response format configuration.
        """
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the response format.
        """
        ...

class OpenAISettings:
    """OpenAI Chat generation settings.

    These fields serialize at the top level of the native Chat Completions
    request. Unknown keyword arguments are preserved and emitted at the same
    top-level location.

    Args:
        temperature (float | None): Sampling temperature.
        top_p (float | None): Nucleus sampling probability.
        max_tokens (int | None): Legacy Chat output-token cap.
        max_completion_tokens (int | None): Chat completion-token cap.
        n (int | None): Number of completions to generate.
        stop (str | list[str] | None): Stop sequence or sequences.
        presence_penalty (float | None): Presence penalty.
        frequency_penalty (float | None): Frequency penalty.
        seed (int | None): Provider best-effort deterministic seed.
        logit_bias (Mapping[str, Any] | None): Token-bias map keyed by token id.
        user (str | None): Provider-visible end-user identifier.
        reasoning_effort (str | None): Reasoning effort label.
        modalities (list[str] | None): Requested output modalities.
        audio (JsonDict | None): Audio output settings.
        prediction (JsonDict | None): Prediction content hint.
        prompt_cache_key (str | None): OpenAI prompt cache key.
        service_tier (str | None): OpenAI service tier.
        safety_identifier (str | None): Safety identifier.
        store (bool | None): Whether the provider may store the response.
        metadata (Mapping[str, Any] | None): Provider metadata object.
        logprobs (bool | None): Whether to return token log probabilities.
        top_logprobs (int | None): Number of top token log probabilities.
        **extra (Any): Unmodeled OpenAI Chat fields.
    """

    def __init__(
        self,
        *,
        temperature: float | None = ...,
        top_p: float | None = ...,
        max_tokens: int | None = ...,
        max_completion_tokens: int | None = ...,
        n: int | None = ...,
        stop: str | list[str] | None = ...,
        presence_penalty: float | None = ...,
        frequency_penalty: float | None = ...,
        seed: int | None = ...,
        logit_bias: Mapping[str, Any] | None = ...,
        user: str | None = ...,
        reasoning_effort: str | None = ...,
        modalities: list[str] | None = ...,
        audio: JsonDict | None = ...,
        prediction: JsonDict | None = ...,
        prompt_cache_key: str | None = ...,
        service_tier: str | None = ...,
        safety_identifier: str | None = ...,
        store: bool | None = ...,
        metadata: Mapping[str, Any] | None = ...,
        logprobs: bool | None = ...,
        top_logprobs: int | None = ...,
        **extra: Any,
    ) -> None:
        """Create OpenAI Chat settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> OpenAISettings:
        """Create OpenAI Chat settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class OpenAIResponsesSettings:
    """OpenAI Responses generation settings.

    These fields serialize at the top level of the native Responses request.
    Unknown keyword arguments are preserved and emitted at the same top-level
    location.

    Args:
        temperature (float | None): Sampling temperature.
        top_p (float | None): Nucleus sampling probability.
        max_output_tokens (int | None): Responses output-token cap.
        reasoning (JsonDict | None): Responses reasoning configuration.
        store (bool | None): Whether the provider may store the response.
        include (list[str] | None): Additional response fields to include.
        metadata (Mapping[str, Any] | None): Provider metadata object.
        **extra (Any): Unmodeled OpenAI Responses fields.
    """

    def __init__(
        self,
        *,
        temperature: float | None = ...,
        top_p: float | None = ...,
        max_output_tokens: int | None = ...,
        reasoning: JsonDict | None = ...,
        store: bool | None = ...,
        include: list[str] | None = ...,
        metadata: Mapping[str, Any] | None = ...,
        **extra: Any,
    ) -> None:
        """Create OpenAI Responses settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> OpenAIResponsesSettings:
        """Create OpenAI Responses settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class AnthropicSettings:
    """Anthropic Messages generation settings.

    These fields serialize at the top level of the native Messages request.
    `max_tokens` defaults to `4096`. Unknown keyword arguments are preserved
    and emitted at the same top-level location.

    Args:
        max_tokens (int): Maximum output tokens.
        temperature (float | None): Sampling temperature.
        top_p (float | None): Nucleus sampling probability.
        top_k (int | None): Top-k sampling limit.
        stop_sequences (list[str] | None): Stop sequences.
        metadata (Mapping[str, Any] | None): Provider metadata object.
        thinking (JsonDict | None): Anthropic thinking configuration.
        **extra (Any): Unmodeled Anthropic Messages fields.
    """

    def __init__(
        self,
        *,
        max_tokens: int = ...,
        temperature: float | None = ...,
        top_p: float | None = ...,
        top_k: int | None = ...,
        stop_sequences: list[str] | None = ...,
        metadata: Mapping[str, Any] | None = ...,
        thinking: JsonDict | None = ...,
        **extra: Any,
    ) -> None:
        """Create Anthropic settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> AnthropicSettings:
        """Create Anthropic settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class GeminiSettings:
    """Gemini and Vertex GenerateContent settings.

    These fields serialize at the top level of the native GenerateContent
    request. Generation knobs such as `temperature`, `top_p`, `top_k`,
    `max_output_tokens`, and `thinking_config` live inside
    `generation_config`. Unknown keyword arguments are preserved and emitted at
    the request top level.

    Args:
        generation_config (JsonDict | None): Native Google generationConfig object.
        safety_settings (list[JsonDict] | None): Native Google safetySettings list.
        cached_content (str | None): Cached content resource name.
        labels (Mapping[str, Any] | None): Provider labels object.
        **extra (Any): Unmodeled Gemini or Vertex request fields.
    """

    def __init__(
        self,
        *,
        generation_config: JsonDict | None = ...,
        safety_settings: list[JsonDict] | None = ...,
        cached_content: str | None = ...,
        labels: Mapping[str, Any] | None = ...,
        **extra: Any,
    ) -> None:
        """Create Gemini or Vertex settings from keyword arguments."""
        ...

    @staticmethod
    def from_dict(value: Mapping[str, Any]) -> GeminiSettings:
        """Create Gemini or Vertex settings from a mapping."""
        ...

    def to_dict(self) -> JsonDict:
        """Return settings as a JSON-compatible dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return settings as JSON."""
        ...

    def __repr__(self) -> str:
        """Return a concise settings representation."""
        ...

class Prompt:
    """Client-safe native prompt builder.

    Prompt text variables and media variables use separate placeholder
    namespaces. Text variables use `{{name}}` and are bound with `bind` or
    `bind_mut`. Media variables use `${media:name}` and are bound with
    `bind_media` or `bind_media_mut`.

    Media placeholders are split into isolated provider-native text parts
    during prompt construction. Binding media replaces those sentinel parts
    with provider-native content blocks while preserving the original provider
    request shape.

    Examples:
        >>> from wyrd import MediaRef, Prompt
        >>> prompt = Prompt("Hi {{name}}, see ${media:logo}", "gpt-4o", provider="openai")
        >>> assert prompt.variables == ["name"]
        >>> assert prompt.media_variables == ["logo"]
        >>> bound = prompt.bind("name", "Ada").bind_media(
        ...     "logo",
        ...     MediaRef.image_url("https://example.com/logo.png"),
        ... )
        >>> assert bound.variables == []
        >>> assert bound.media_variables == []
    """

    provider: str
    request: ProviderRequest
    messages: list[JsonDict]
    message: JsonDict | None
    system_messages: Any
    model: str
    version: str | None
    variables: list[str]
    media_variables: list[str]
    model_settings: (
        OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | None
    )

    def __init__(
        self,
        messages: Any,
        model: str,
        *,
        provider: str,
        system: str | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        operation: str | None = ...,
        cache: str | Mapping[str, Any] | None = ...,
        model_settings: OpenAISettings
        | OpenAIResponsesSettings
        | AnthropicSettings
        | GeminiSettings
        | Mapping[str, Any]
        | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> None:
        """Build a provider-native prompt from dynamic authoring inputs.

        Args:
            messages (Any): Provider-shaped message input. Strings are treated
                as user text; sequences and mappings are coerced into the
                target provider's native request shape.
            model (str): Provider model identifier to store on the native
                prompt and request.
            provider (str): Provider name. Accepted values are `openai`,
                `anthropic`, `gemini`, `google`, `vertex`, or a custom provider
                accepted by the raw passthrough path.
            system (str | None): Optional system instruction when supported by
                the selected provider.
            response_format (ResponseFormat | JsonDict | None): Optional
                structured-output helper or schema dictionary.
            operation (str | None): Optional provider operation selector used
                by provider families with more than one request shape.
            cache (str | Mapping[str, Any] | None): Optional cache sugar for
                providers with a native cache key.
            model_settings (OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | Mapping[str, Any] | None): Native provider generation settings object or mapping. Explicit settings take precedence over cache sugar.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `{{name}}` placeholders from the request.
            version (str | None): Optional prompt version string stored on the
                native prompt.
        """
        ...

    @staticmethod
    def openai_chat(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        cache: str | Mapping[str, Any] | None = ...,
        model_settings: OpenAISettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an OpenAI Chat prompt.

        Args:
            model (str): OpenAI model identifier.
            system (str | None): Optional system message prepended to the chat.
            messages (Any | None): Optional user-authored chat messages.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            cache (str | Mapping[str, Any] | None): Optional prompt cache key.
            model_settings (OpenAISettings | Mapping[str, Any] | None): OpenAI settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping an OpenAI Chat Completions request.
        """
        ...

    @staticmethod
    def openai_responses(
        model: str,
        *,
        instructions: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        model_settings: OpenAIResponsesSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an OpenAI Responses prompt.

        Args:
            model (str): OpenAI model identifier.
            instructions (str | None): Optional Responses API instructions.
            messages (Any | None): Optional Responses API input items.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            model_settings (OpenAIResponsesSettings | Mapping[str, Any] | None): OpenAI Responses settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping an OpenAI Responses request.
        """
        ...

    @staticmethod
    def anthropic(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        model_settings: AnthropicSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build an Anthropic Messages prompt.

        Args:
            model (str): Anthropic model identifier.
            system (str | None): Optional system instruction.
            messages (Any | None): Optional Anthropic message list.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            model_settings (AnthropicSettings | Mapping[str, Any] | None): Anthropic settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping an Anthropic Messages request.
        """
        ...

    @staticmethod
    def gemini(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        model_settings: GeminiSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build a Gemini GenerateContent prompt.

        Args:
            model (str): Gemini model identifier.
            system (str | None): Optional system instruction.
            messages (Any | None): Optional Gemini content turns.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            model_settings (GeminiSettings | Mapping[str, Any] | None): Gemini settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping a Gemini GenerateContent request.
        """
        ...

    @staticmethod
    def vertex(
        model: str,
        *,
        system: str | None = ...,
        messages: Any | None = ...,
        response_format: ResponseFormat | JsonDict | None = ...,
        model_settings: GeminiSettings | Mapping[str, Any] | None = ...,
        variables: list[str] | None = ...,
        version: str | None = ...,
    ) -> Prompt:
        """Build a Vertex GenerateContent prompt.

        Args:
            model (str): Vertex model identifier.
            system (str | None): Optional system instruction.
            messages (Any | None): Optional Vertex content turns.
            response_format (ResponseFormat | JsonDict | None): Optional
                response-format helper or schema dictionary.
            model_settings (GeminiSettings | Mapping[str, Any] | None): Gemini settings object or mapping.
            variables (list[str] | None): Declared text variables. When
                omitted, Wyrd infers `{{name}}` placeholders.
            version (str | None): Optional prompt version string.

        Returns:
            Prompt: Prompt wrapping a Vertex GenerateContent request.
        """
        ...

    @staticmethod
    def raw(provider: str, model: str, body: bytes) -> Prompt:
        """Build a raw JSON passthrough prompt.

        Args:
            provider (str): Provider dispatch target for the raw body.
            model (str): Model identifier used for indexing and selection.
            body (bytes): Raw provider request JSON bytes.

        Returns:
            Prompt: Prompt wrapping a raw provider request.
        """
        ...

    def system(self, text: str) -> Prompt:
        """Return a copy with a system message applied.

        Args:
            text (str): System instruction text.

        Returns:
            Prompt: New prompt with the system instruction applied.
        """
        ...

    def user(self, content: Any) -> Prompt:
        """Return a copy with a user message appended.

        Args:
            content (Any): Provider-compatible user message content.

        Returns:
            Prompt: New prompt with the user message appended.
        """
        ...

    def assistant(self, content: Any) -> Prompt:
        """Return a copy with an assistant message appended.

        Args:
            content (Any): Provider-compatible assistant message content.

        Returns:
            Prompt: New prompt with the assistant message appended.
        """
        ...

    def tool_result(self, tool_use_id: str, content: str, is_error: bool = ...) -> Prompt:
        """Return a copy with a native tool-result message appended.

        Args:
            tool_use_id (str): Provider tool-call identifier being answered.
            content (str): Tool result text.
            is_error (bool): Whether the tool result represents an error.

        Returns:
            Prompt: New prompt with the tool-result message appended.
        """
        ...

    def render(self, **kwargs: str) -> ProviderRequest:
        """Render declared variables and return a provider request.

        Args:
            **kwargs (str): Text variable bindings keyed by variable name.

        Returns:
            ProviderRequest: Rendered provider-native request.
        """
        ...

    def bind(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> Prompt:
        """Return a copy with one or more variables bound.

        Args:
            name (str | None): Optional single text variable name.
            value (Any | None): Optional value for `name`.
            **kwargs (Any): Additional text variable bindings.

        Returns:
            Prompt: New prompt with bound text variables removed from
                `variables`.
        """
        ...

    def bind_mut(self, name: str | None = ..., value: Any | None = ..., **kwargs: Any) -> None:
        """Bind one or more variables in place.

        Args:
            name (str | None): Optional single text variable name.
            value (Any | None): Optional value for `name`.
            **kwargs (Any): Additional text variable bindings.
        """
        ...

    def bind_media(self, name: str, media: MediaRef) -> Prompt:
        """Return a copy with a media placeholder bound.

        The matching `${media:name}` text part is replaced with the typed
        provider-native content variant for this prompt's provider. The
        original prompt is unchanged.

        Args:
            name (str): Media parameter name, without the `${media:...}`
                wrapper.
            media (MediaRef): Media payload to insert.

        Returns:
            Prompt: New prompt with `name` removed from `media_variables`.

        Raises:
            WyrdError: If the placeholder is missing, not isolated, unsupported
                by the provider, or missing a required MIME type.
        """
        ...

    def bind_media_mut(self, name: str, media: MediaRef) -> None:
        """Bind a media placeholder in place.

        Args:
            name (str): Media parameter name, without the `${media:...}`
                wrapper.
            media (MediaRef): Media payload to insert.

        Raises:
            WyrdError: Same failures as `bind_media`.
        """
        ...

    def model_dump(self) -> JsonDict:
        """Return the native prompt as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible native prompt representation.
        """
        ...

    def model_dump_json(self) -> str:
        """Return the native prompt as JSON.

        Returns:
            str: Serialized native prompt JSON.
        """
        ...

    @staticmethod
    def model_validate_json(data: str) -> Prompt:
        """Build a prompt from serialized native prompt JSON.

        Args:
            data (str): Serialized native prompt JSON.

        Returns:
            Prompt: Prompt rebuilt from the serialized native prompt.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> Prompt:
        """Load a bare native prompt from JSON or YAML.

        Args:
            path (PathLike): Source `.json`, `.yaml`, or `.yml` prompt file.

        Returns:
            Prompt: Prompt loaded from the bare prompt spec file.
        """
        ...

    def dump(self, path: PathLike) -> None:
        """Dump a bare native prompt to JSON or YAML.

        Args:
            path (PathLike): Target `.json`, `.yaml`, or `.yml` prompt file.
        """
        ...

    @staticmethod
    def openai_image_url(url: str, detail: str | None = ...) -> JsonDict:
        """Return an OpenAI image URL content part.

        Args:
            url (str): Image URL.
            detail (str | None): Optional OpenAI image detail hint.

        Returns:
            JsonDict: OpenAI image URL content part.
        """
        ...

    @staticmethod
    def anthropic_image_base64(media_type: str, data: str) -> JsonDict:
        """Return an Anthropic base64 image content block.

        Args:
            media_type (str): Image MIME type.
            data (str): Base64 image payload.

        Returns:
            JsonDict: Anthropic image content block.
        """
        ...

    @staticmethod
    def google_file_data(mime_type: str, file_uri: str) -> JsonDict:
        """Return a Google file-data content part.

        Args:
            mime_type (str): File MIME type.
            file_uri (str): Google file URI.

        Returns:
            JsonDict: Google file-data content part.
        """
        ...

    @staticmethod
    def image_url(url: str, *, detail: str | None = ..., provider: str = ...) -> JsonDict:
        """Return an image URL content helper.

        Args:
            url (str): Image URL.
            detail (str | None): Optional image detail hint.
            provider (str): Provider-specific helper target.

        Returns:
            JsonDict: Provider-specific image URL content value.
        """
        ...

    @staticmethod
    def image_base64(media_type: str, data: str) -> JsonDict:
        """Return a base64 image content helper.

        Args:
            media_type (str): Image MIME type.
            data (str): Base64 image payload.

        Returns:
            JsonDict: Provider-specific base64 image content value.
        """
        ...

    @staticmethod
    def file_uri(mime_type: str, file_uri: str) -> JsonDict:
        """Return a file URI content helper.

        Args:
            mime_type (str): File MIME type.
            file_uri (str): Provider file URI.

        Returns:
            JsonDict: Provider-specific file URI content value.
        """
        ...

    @staticmethod
    def file_id(file_id: str) -> JsonDict:
        """Return a file-id content helper.

        Args:
            file_id (str): Provider file identifier.

        Returns:
            JsonDict: Provider-specific file-id content value.
        """
        ...

    @staticmethod
    def document_text(media_type: str, data: str, title: str | None = ...) -> JsonDict:
        """Return a text document content helper.

        Args:
            media_type (str): Document MIME type.
            data (str): Document text payload.
            title (str | None): Optional document title.

        Returns:
            JsonDict: Provider-specific text document content value.
        """
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the native prompt.
        """
        ...

class PromptRef:
    """Python-facing Wyrd prompt reference.

    A prompt reference points at either a registered Prompt Card or an inline
    Prompt spec.
    """

    kind: str

    @staticmethod
    def card(
        name: str, version: str, *, space: str | None = ..., uid: str | None = ...
    ) -> PromptRef:
        """Create a reference to a registered Prompt Card."""
        ...

    @staticmethod
    def inline(prompt: Prompt) -> PromptRef:
        """Create an inline prompt reference from a Prompt."""
        ...

    def model_dump(self) -> JsonDict:
        """Return this prompt reference as a Python dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return this prompt reference as JSON."""
        ...

    @staticmethod
    def model_validate_json(data: str) -> PromptRef:
        """Build a prompt reference from serialized JSON."""
        ...

class PromptCardMetadata:
    """Local holder metadata used when serializing a `PromptCard`.

    The metadata stores the native prompt body that becomes
    `PromptSpec.prompt` in the Wyrd card envelope.
    """

    prompt: Prompt

    def __init__(
        self,
        prompt: Prompt | None = ...,
        *,
        model_settings: OpenAISettings
        | OpenAIResponsesSettings
        | AnthropicSettings
        | GeminiSettings
        | Mapping[str, Any]
        | None = ...,
    ) -> None:
        """Create prompt card metadata from an optional `Prompt`.

        Args:
            prompt (Prompt | None): Native prompt builder to store in metadata.
                When omitted, Wyrd creates placeholder metadata that callers can
                replace before serialization.
            model_settings (OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | Mapping[str, Any] | None): Provider-native generation settings to apply to `prompt` before storing metadata.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this metadata as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible holder metadata.
        """
        ...

    def to_spec(self) -> JsonDict:
        """Return the validated PromptSpec body as a Python dictionary.

        Returns:
            JsonDict: JSON-compatible `PromptSpec` body containing the native
                prompt.

        Raises:
            WyrdError: If the stored prompt violates PromptCard validation.
        """
        ...

class PromptCard:
    """Local PromptCard holder for filesystem materialization.

    A `PromptCard` wraps a live `Prompt`, local card identity, labels,
    annotations, and metadata. It can save and load local JSON or YAML card
    envelopes, but registration belongs to registry/client surfaces.
    """

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    metadata: PromptCardMetadata
    prompt: Prompt
    model_settings: (
        OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | None
    )
    content_hash: str
    parameters: list[str]
    is_fully_bound: bool
    card_ref: str
    is_card: bool

    def __init__(
        self,
        prompt: Prompt,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
        metadata: PromptCardMetadata | None = ...,
        model_settings: OpenAISettings
        | OpenAIResponsesSettings
        | AnthropicSettings
        | GeminiSettings
        | Mapping[str, Any]
        | None = ...,
    ) -> None:
        """Create a local `PromptCard` from a `Prompt`.

        Args:
            prompt (Prompt): Live prompt builder to store in the PromptCard
                spec body.
            space (str | None): Optional card space. Defaults to `default`.
            name (str | None): Optional card name. Defaults to `prompt`.
            version (str | None): Optional semantic version. Defaults to
                `0.1.0`.
            uid (str | None): Optional card UID. Defaults to a generated UID.
            labels (Mapping[str, str] | None): Queryable user labels copied
                into card metadata.
            annotations (Mapping[str, str] | None): Free-form user annotations
                copied into card metadata.
            metadata (PromptCardMetadata | None): Existing holder metadata to
                seed before the live prompt is captured.
            model_settings (OpenAISettings | OpenAIResponsesSettings | AnthropicSettings | GeminiSettings | Mapping[str, Any] | None): Provider-native generation settings to apply to the captured prompt. Typed settings must match the prompt provider; mappings decode into the active provider shape.

        Raises:
            WyrdError: If labels, annotations, identity fields, or prompt
                metadata violate the PromptCard contract.
        """
        ...

    def save(self, path: PathLike) -> None:
        """Save this PromptCard envelope to a local JSON or YAML file.

        Args:
            path (PathLike): Target `.json`, `.yaml`, or `.yml` file path.

        Raises:
            WyrdError: If the path extension is unsupported, the card cannot be
                serialized, or filesystem IO fails.
        """
        ...

    @staticmethod
    def load(path: PathLike) -> PromptCard:
        """Load a PromptCard envelope from a local JSON or YAML file.

        Args:
            path (PathLike): Source `.json`, `.yaml`, or `.yml` file path.

        Returns:
            PromptCard: Local holder rebuilt from the serialized envelope.

        Raises:
            WyrdError: If the file cannot be read, parsed, or validated as a
                PromptCard envelope.
        """
        ...

    @staticmethod
    def from_path(path: PathLike) -> PromptCard:
        """Load a PromptCard envelope from a local JSON or YAML file.

        Accepts both the stored native format and the declarative authoring
        format (when the spec has a ``provider`` key instead of ``request``).

        Args:
            path (PathLike): Source `.json`, `.yaml`, or `.yml` file path.

        Returns:
            PromptCard: Local holder rebuilt from the serialized envelope.

        Raises:
            WyrdError: If the file cannot be read, parsed, or validated as a
                PromptCard envelope.
        """
        ...

    def model_dump_json(self) -> str:
        """Return this PromptCard as a JSON envelope string.

        Returns:
            str: Serialized Wyrd card envelope.

        Raises:
            WyrdError: If the prompt or holder identity cannot be converted
                into a valid PromptCard envelope.
        """
        ...

    @staticmethod
    def model_validate_json(json_string: str) -> PromptCard:
        """Build a PromptCard from serialized card-envelope JSON.

        Args:
            json_string (str): Serialized Wyrd PromptCard envelope.

        Returns:
            PromptCard: Local holder rebuilt from the JSON envelope.

        Raises:
            WyrdError: If the JSON is invalid or does not contain a
                `apiVersion: wyrd/v1`, `kind: Prompt` envelope.
        """
        ...

    def __str__(self) -> str:
        """Return pretty JSON for interactive inspection.

        Returns:
            str: Pretty JSON representation of the PromptCard envelope.
        """
        ...

### GLOBAL EXPORTS ###
__all__ = [
    "Agent",
    "AnthropicSettings",
    "ArrowInterface",
    "ArtifactCard",
    "CatboostInterface",
    "DataCard",
    "DataCardMetadata",
    "DataInterface",
    "DataSchema",
    "DataStats",
    "FieldSpec",
    "GeminiSettings",
    "HuggingfaceInterface",
    "ImageInterface",
    "JsonlInterface",
    "LightgbmInterface",
    "LightningInterface",
    "MediaRef",
    "ModelCard",
    "ModelCardMetadata",
    "ModelInterface",
    "ModelSignature",
    "NumpyInterface",
    "OpenAIResponsesSettings",
    "OpenAISettings",
    "PandasInterface",
    "ParquetInterface",
    "PolarsInterface",
    "Prompt",
    "PromptCard",
    "PromptCardMetadata",
    "PromptRef",
    "ProviderRequest",
    "ResponseFormat",
    "SampleInput",
    "SklearnInterface",
    "Split",
    "SqlInterface",
    "TensorflowInterface",
    "TextInterface",
    "TorchInterface",
    "WyrdError",
    "XgboostInterface",
]
