from __future__ import annotations

import os
import pathlib
from collections.abc import Mapping
from typing import Any, overload

class WyrdError(Exception):
    """Python-facing Wyrd error with a stable code and remediation text."""

    code: str
    message: str
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
        """Create a WyrdError.

        `code` is the stable Wyrd error code. `message` is the human-readable
        failure. `details` defaults to `None` and is stored as a mutable dict
        when supplied. `remediation` defaults to DataCard input guidance.
        """
        ...

class FieldSpec:
    """One field in a DataCard schema."""

    name: str
    dtype: str
    shape: list[dict[str, Any]]
    nullable: bool
    extra: dict[str, str]

    def __init__(
        self,
        name: str,
        dtype: str,
        shape: list[dict[str, Any]] = ...,
        nullable: bool = ...,
        extra: dict[str, str] = ...,
    ) -> None:
        """Declare one schema field.

        `name` and `dtype` are required. `shape` defaults to an empty list and
        stores dimension dictionaries. `nullable` defaults to `False`. `extra`
        defaults to an empty dict for small string metadata. This constructor
        does not perform filesystem IO.
        """
        ...

    def to_dict(self) -> dict[str, Any]:
        """Return this field as serialized Wyrd schema data."""
        ...

class DataSchema:
    """Ordered schema inferred from or supplied for a DataCard."""

    columns: list[FieldSpec]

    def __init__(self, columns: list[FieldSpec] = ...) -> None:
        """Create a schema from an ordered list of fields.

        `columns` defaults to an empty list. The schema is local metadata only
        and does not perform filesystem IO.
        """
        ...

    def to_dict(self) -> dict[str, Any]:
        """Return this schema as serialized Wyrd card data."""
        ...

class DataStats:
    """Local byte and shape statistics recorded in serialized DataCard data."""

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
        """Create local artifact statistics.

        `byte_count` and `sha256` are required. `row_count` and `col_count`
        default to `None` when Wyrd cannot infer them. No filesystem IO occurs.
        """
        ...

    @classmethod
    def from_bytes(cls, payload: bytes, schema: DataSchema | None = ...) -> DataStats:
        """Hash local bytes and return DataCard statistics.

        `schema` defaults to `None`; when supplied, its column count is copied.
        This method performs no filesystem IO.
        """
        ...

    @classmethod
    def from_file(
        cls,
        path: str | os.PathLike[str] | pathlib.Path,
        schema: DataSchema | None = ...,
    ) -> DataStats:
        """Read a local file, hash its bytes, and return DataCard statistics.

        `path` may be a string, `os.PathLike[str]`, or `pathlib.Path`. The
        method performs local filesystem IO and lets normal file read errors
        propagate.
        """
        ...

    def to_dict(self) -> dict[str, Any]:
        """Return these statistics as serialized Wyrd card data."""
        ...

class DataInterface:
    """Base class for local DataCard interface adapters."""

    kind: str
    data: Any | None

    def __init__(self) -> None:
        """Reject direct construction.

        Use a concrete interface class such as `PandasInterface` or
        `SqlInterface`. Direct construction raises `WyrdError`.
        """
        ...

    @property
    def has_source(self) -> bool:
        """Return whether this interface currently holds live local data."""
        ...

    def to_dict(self) -> dict[str, Any]:
        """Return the serialized interface kind and metadata."""
        ...

    def meta(self) -> dict[str, Any]:
        """Return interface-specific metadata.

        Concrete interfaces implement this. The base method is not meant to be
        called directly.
        """
        ...

    def infer_schema(self) -> DataSchema:
        """Infer a DataSchema from the live local data without filesystem IO."""
        ...

    def save(
        self,
        path: str | os.PathLike[str] | pathlib.Path,
        save_kwargs: dict[str, Any] | None = ...,
    ) -> dict[str, Any]:
        """Write this interface's local data under `path`.

        The method performs local filesystem IO and returns mutable save
        metadata for the serialized card. It raises `WyrdError` when the
        interface lacks required live data or the interface kind is unsupported.
        """
        ...

    def load(
        self,
        path: str | os.PathLike[str] | pathlib.Path,
        metadata: dict[str, Any],
        load_kwargs: dict[str, Any] | None = ...,
    ) -> None:
        """Load local materialized data and replace `self.data`.

        `metadata` must be save metadata produced by `save()`. The method
        performs local filesystem IO, mutates the interface, and raises
        `WyrdError` for unsupported interface kinds.
        """
        ...

class PandasInterface(DataInterface):
    """DataCard interface for pandas DataFrame values serialized as parquet."""

    def __init__(self, *, data: Any = ..., compression: str = ...) -> None:
        """Create a pandas interface.

        `data` accepts a pandas DataFrame or `None`. `compression` defaults to
        `"snappy"` and accepts `"none"`, `"snappy"`, `"gzip"`, `"zstd"`, or
        `"lz4"`. Invalid compression raises `WyrdError`. No filesystem IO
        occurs until `save()` or `load()`.
        """
        ...

class PolarsInterface(DataInterface):
    """DataCard interface for polars DataFrame values serialized as parquet."""

    def __init__(self, *, data: Any = ..., compression: str = ...) -> None:
        """Create a polars interface.

        `data` accepts a polars DataFrame or `None`. `compression` defaults to
        `"snappy"` and accepts `"none"`, `"snappy"`, `"gzip"`, `"zstd"`, or
        `"lz4"`. Invalid compression raises `WyrdError`. No filesystem IO
        occurs until `save()` or `load()`.
        """
        ...

class ArrowInterface(DataInterface):
    """DataCard interface for pyarrow Tables serialized as parquet or IPC."""

    def __init__(self, *, data: Any = ..., format: str = ...) -> None:
        """Create an Arrow interface.

        `data` accepts a pyarrow Table or `None`. `format` defaults to
        `"parquet"` and accepts `"parquet"` or `"ipc"`. Invalid formats raise
        `WyrdError`. No filesystem IO occurs until `save()` or `load()`.
        """
        ...

class ParquetInterface(DataInterface):
    """DataCard interface for parquet files or table-like parquet sources."""

    def __init__(
        self,
        *,
        data: Any = ...,
        compression: str = ...,
        row_group_size: int | None = ...,
    ) -> None:
        """Create a parquet interface.

        `data` accepts a parquet path, a table-like object, or `None`.
        `compression` defaults to `"snappy"` and accepts `"none"`, `"snappy"`,
        `"gzip"`, `"zstd"`, or `"lz4"`. `row_group_size` defaults to `None`
        and is copied into metadata. Invalid compression raises `WyrdError`.
        """
        ...

class NumpyInterface(DataInterface):
    """DataCard interface for numpy arrays serialized as `.npy` or `.npz` files."""

    def __init__(
        self,
        *,
        data: Any = ...,
        dtype: str | None = ...,
        shape: list[int] | None = ...,
        format: str = ...,
    ) -> None:
        """Create a numpy interface.

        `data` accepts a numpy array or `None`. `dtype` and `shape` default to
        values inferred from `data`. `format` defaults to `"npy"` and accepts
        `"npy"` or `"npz"`. Invalid formats raise `WyrdError`.
        """
        ...

class TorchInterface(DataInterface):
    """DataCard interface for torch tensors or tensor maps."""

    def __init__(self, *, data: Any = ..., save_format: str = ...) -> None:
        """Create a torch interface.

        `data` accepts a torch tensor, a tensor mapping, or `None`.
        `save_format` defaults to `"safetensors"` and accepts `"safetensors"`
        or `"pickle"`. Invalid formats raise `WyrdError`.
        """
        ...

class SqlInterface(DataInterface):
    """DataCard interface for local SQL query bundles."""

    def __init__(
        self,
        *,
        data: Any = ...,
        dialect: str,
        connection_hint: str | None = ...,
    ) -> None:
        """Create a SQL interface.

        `dialect` is required. `data` accepts a mapping with queries or `None`.
        `connection_hint` defaults to `None` and is metadata only. This
        interface writes query JSON during local save; it does not connect to a
        database.
        """
        ...

class JsonlInterface(DataInterface):
    """DataCard interface for JSON Lines rows or JSONL files."""

    def __init__(
        self,
        *,
        data: Any = ...,
        compression: str = ...,
        lines_per_file: int | None = ...,
    ) -> None:
        """Create a JSON Lines interface.

        `data` accepts an iterable of JSON-serializable rows, a JSONL path, or
        `None`. `compression` defaults to `"none"` and accepts `"none"`,
        `"gzip"`, or `"zstd"`. `lines_per_file` defaults to `None` and is
        metadata only. Invalid compression raises `WyrdError`.
        """
        ...

class ImageInterface(DataInterface):
    """DataCard interface for image file manifests."""

    def __init__(
        self,
        *,
        data: Any = ...,
        format: str = ...,
        color_mode: str = ...,
        manifest_ref: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create an image manifest interface.

        `data` accepts a local image file, image directory, manifest-like data,
        or `None`. `format` defaults to `"mixed"` and accepts `"png"`,
        `"jpeg"`, `"webp"`, or `"mixed"`. `color_mode` defaults to `"rgb"` and
        accepts `"rgb"`, `"rgba"`, or `"grayscale"`. `manifest_ref` defaults
        to `None` and is copied from a read-only mapping. Invalid options raise
        `WyrdError`.
        """
        ...

class TextInterface(DataInterface):
    """DataCard interface for text file manifests."""

    def __init__(
        self,
        *,
        data: Any = ...,
        encoding: str = ...,
        manifest_ref: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create a text manifest interface.

        `data` accepts a local text file, text directory, manifest-like data, or
        `None`. `encoding` defaults to `"utf-8"`. `manifest_ref` defaults to
        `None` and is copied from a read-only mapping. No filesystem IO occurs
        until `save()` or `load()`.
        """
        ...

class HuggingfaceInterface(DataInterface):
    """DataCard interface for local Hugging Face datasets or dataset pointers."""

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

        `dataset_id` is required. `data` accepts a datasets object, pointer
        data, or `None`. `revision`, `split`, and `config` default to `None`.
        When provided, `revision` must be 7 to 40 lowercase hex characters or
        this constructor raises `WyrdError`.
        """
        ...

class CustomDataInterface(DataInterface):
    """DataCard interface delegated to a user-provided loader class."""

    def __init__(
        self,
        *,
        data: Any = ...,
        loader_module: str,
        loader_class: str,
        extra: Mapping[str, str] | None = ...,
    ) -> None:
        """Create a custom data interface.

        `loader_module` and `loader_class` are required and are imported during
        local save/load. `data` accepts any object the loader understands.
        `extra` defaults to `None` and is copied from a read-only mapping. Bad
        loader imports surface as normal import or attribute errors.
        """
        ...

class Split:
    """Serialized DataCard split declaration."""

    strategy: dict[str, Any]

    def __init__(self, strategy: dict[str, Any]) -> None:
        """Create a split from a mutable serialized strategy dict.

        No validation beyond storing the strategy occurs here. Prefer the
        static builders for validated split declarations.
        """
        ...

    @staticmethod
    def column(col: str, op: str, value: Any) -> Split:
        """Declare a column predicate split.

        `op` accepts `"=="`, `"!="`, `">"`, `">="`, `"<"`, `"<="`, or
        `"in"`. The method performs no filesystem IO and raises `WyrdError`
        when `op` is invalid.
        """
        ...

    @staticmethod
    def materialized(artifact_ref: Mapping[str, Any]) -> Split:
        """Declare a split backed by an Artifact card reference.

        `artifact_ref` is read as a mapping and copied into serialized split
        data. It must contain `{"kind": "Artifact"}` or `WyrdError` is raised.
        """
        ...

    @staticmethod
    def index_range(start: int, stop: int) -> Split:
        """Declare a non-negative index range split.

        `start` and `stop` must satisfy `0 <= start <= stop`. The method
        performs no filesystem IO and raises `WyrdError` for invalid ranges.
        """
        ...

    @staticmethod
    def indices(values: list[int]) -> Split:
        """Declare an explicit index split.

        `values` must contain unique non-negative integers. The values are
        copied into serialized split data. Invalid values raise `WyrdError`.
        """
        ...

    def to_dict(self) -> dict[str, Any]:
        """Return the serialized split declaration as a mutable dict."""
        ...

class DataCard:
    """Local DataCard holder and spec builder."""

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    created_at: str
    is_card: bool
    interface: DataInterface
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
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
        metadata: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create a DataCard from an explicit DataInterface.

        `space`, `name`, and `version` default to `"default"`, `"data"`, and
        `"0.1.0"`. `uid` defaults to a generated local identifier. `labels`
        and `annotations` default to empty mappings. Construction performs no
        filesystem IO and raises `WyrdError` when the interface cannot infer
        its schema.
        """
        ...

    @overload
    def __init__(
        self,
        data: str | os.PathLike[str] | pathlib.Path | Mapping[str, Any],
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
        metadata: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create a DataCard from a direct local path or SQL query mapping.

        Paths infer text directories, parquet files, or JSONL files. Mappings
        with `queries` infer `SqlInterface`. Construction does not read or
        write local files, but unsupported inputs raise `WyrdError`.
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
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
        metadata: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create a DataCard by inferring the interface from runtime data.

        `data` intentionally accepts pandas, polars, pyarrow, numpy, torch, and
        other supported runtime objects without importing those libraries for
        type checking. Unsupported inputs raise `WyrdError`.
        """
        ...

    @property
    def metadata(self) -> dict[str, Any]:
        """Return mutable DataCard annotation metadata."""
        ...

    @metadata.setter
    def metadata(self, value: Mapping[str, Any]) -> None:
        """Replace annotation metadata by copying `value`."""
        ...

    @property
    def data(self) -> Any:
        """Return live local data or raise `WyrdError` when none is attached."""
        ...

    def save(
        self,
        path: str | os.PathLike[str] | pathlib.Path,
        save_kwargs: dict[str, Any] | None = ...,
    ) -> None:
        """Materialize this DataCard under a local directory.

        The method creates directories as needed, writes local data, writes
        `card.json`, and mutates save metadata, schema, and stats. It raises
        `WyrdError` when live local data is missing or unsupported.
        """
        ...

    def load(
        self,
        path: str | os.PathLike[str] | pathlib.Path | None = ...,
        load_kwargs: dict[str, Any] | None = ...,
    ) -> None:
        """Hydrate local data from a previously saved DataCard directory.

        `path` is required unless a registry/client has already provided local
        materialized data. This method reads local files, mutates the interface
        data, and raises `WyrdError` when save metadata is missing.
        """
        ...

    def save_card(self, path: str | os.PathLike[str] | pathlib.Path) -> None:
        """Write only the serialized Wyrd card envelope to `path/card.json`."""
        ...

    def model_dump_json(self, *, indent: int | None = ...) -> str:
        """Return serialized Wyrd card JSON without filesystem IO."""
        ...

    @staticmethod
    def model_validate_json(json_string: str, data: Any = ...) -> DataCard:
        """Build a DataCard from serialized Wyrd card JSON.

        The method parses JSON and rebuilds local DataCard metadata. It does
        not perform filesystem IO. Pass `data` to attach live local data instead
        of rebuilding an empty interface from serialized metadata.
        """
        ...

    def to_dict(self) -> dict[str, Any]:
        """Return the serialized Wyrd card envelope as a mutable dict."""
        ...

__all__: list[str]
