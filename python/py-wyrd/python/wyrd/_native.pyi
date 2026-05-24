# AUTO-GENERATED STUB FILE. DO NOT EDIT.
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

### GLOBAL EXPORTS ###
__all__ = [
    "ArrowInterface",
    "ArtifactCard",
    "DataCard",
    "DataCardMetadata",
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
