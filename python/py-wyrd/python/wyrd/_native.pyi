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

### GLOBAL EXPORTS ###
__all__ = [
    "ArrowInterface",
    "ArtifactCard",
    "CatboostInterface",
    "DataCard",
    "DataCardMetadata",
    "DataInterface",
    "DataSchema",
    "DataStats",
    "FieldSpec",
    "HuggingfaceInterface",
    "ImageInterface",
    "JsonlInterface",
    "LightgbmInterface",
    "LightningInterface",
    "ModelCard",
    "ModelCardMetadata",
    "ModelInterface",
    "ModelSignature",
    "NumpyInterface",
    "PandasInterface",
    "ParquetInterface",
    "PolarsInterface",
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
