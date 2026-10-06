# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####

from collections.abc import Mapping, Sequence
from typing import Any, Literal, overload

from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError
from ..cards import CardRef, DataLoadArgs, JsonValue

#### end of imports ####

class Dim:
    """One tensor dimension: a fixed length or a named dynamic axis."""

    @staticmethod
    def fixed(length: int) -> Dim:
        """Declare a dimension with a fixed length."""
        ...

    @staticmethod
    def dynamic(name: str | None = ...) -> Dim:
        """Declare a variable-length dimension, optionally named (e.g. `batch`)."""
        ...

    @property
    def kind(self) -> Literal["Fixed", "Dynamic"]:
        """Whether this dimension is fixed or dynamic."""
        ...

    @property
    def length(self) -> int | None:
        """The fixed length, or `None` for a dynamic dimension."""
        ...

    @property
    def name(self) -> str | None:
        """The dynamic axis name, or `None` when fixed or unnamed."""
        ...

class FieldSpec:
    """One column or tensor field in a DataCard schema or model signature.

    Construction validates only the name; it does not inspect data or touch
    the filesystem. ``shape`` and its alias ``dims`` return serialized
    dimensions: ``{"kind": "Fixed", "value": n}`` or
    ``{"kind": "Dynamic", "value": name_or_None}``.
    """

    name: str
    dtype: str
    shape: list[Dim]
    dims: list[Dim]
    nullable: bool
    extra: dict[str, str]

    def __init__(
        self,
        name: str,
        dtype: str,
        shape: Sequence[Dim] | Sequence[Mapping[str, Any]] | None = ...,
        nullable: bool = ...,
        extra: StringMap | None = ...,
    ) -> None:
        """Declare a field.

        Args:
            name (str): Column or field name stored in the Wyrd spec.
            dtype (str): Normalized dtype label, for example `int64` or
                `string`.
            shape (Sequence[Dim] | Sequence[Mapping[str, Any]] | None):
                Optional dimensions for tensor-like data, as `Dim` values or
                their serialized form.
            nullable (bool): Whether the field may contain null values.
            extra (StringMap | None): Small string metadata copied into the
                field spec.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if ``name`` is not a
                valid column name, or a ``WyrdError`` if ``shape`` cannot be
                parsed as dimensions.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this field as a JSON-compatible spec dictionary."""
        ...

class DataSchema:
    """Ordered fields describing a DataCard's data.

    ``fields`` is an alias of ``columns``. A DataCard infers its schema from
    live built-in interface data; custom interfaces and sourceless
    interfaces produce an empty schema.
    """

    columns: list[FieldSpec]
    fields: list[FieldSpec]

    def __init__(self, columns: Sequence[FieldSpec | Mapping[str, Any]] | None = ...) -> None:
        """Create a schema from ordered fields.

        Args:
            columns: ``FieldSpec`` objects or serialized field dictionaries,
                in order. Omitted, the schema is empty.

        Raises:
            WyrdError: If a serialized field cannot be parsed.
        """
        ...

    def is_empty(self) -> bool:
        """Return ``True`` when the schema has no fields."""
        ...

    def contains_column(self, name: str) -> bool:
        """Return whether a field named ``name`` exists.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if ``name`` is not a
                valid column name.
        """
        ...

    def column(self, name: str) -> FieldSpec | None:
        """Return the field named ``name``, or ``None`` when it is absent.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if ``name`` is not a
                valid column name.
        """
        ...

    def column_names(self) -> list[str]:
        """Return field names in schema order."""
        ...

    def to_dict(self) -> JsonDict:
        """Return this schema as a JSON-compatible spec dictionary."""
        ...

class DataStats:
    """Size, digest, and shape statistics for a saved DataCard artifact.

    Interfaces return these from ``save``. A card that has never been saved
    holds placeholder statistics: ``byte_count`` 0 and an all-zero digest.
    """

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
        """Create artifact statistics, typically from a custom ``save``.

        Args:
            byte_count: number of artifact bytes written.
            sha256: lowercase hex SHA-256 digest of those bytes.
            row_count: row count, or ``None`` (the default) when unknown.
            col_count: column count, or ``None`` (the default) when unknown.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return these statistics as a JSON-compatible spec dictionary."""
        ...

class DataInterface:
    """Base class for DataCard data interfaces.

    Subclass it for data no built-in interface handles. A subclass must
    override ``save`` and ``load``; the base methods raise so a missing
    override cannot record an empty artifact. ``kind`` is the interface kind
    stored in the card: the built-in name, or ``Custom`` for subclasses.
    """

    kind: str

    def __init__(self, *args: Any, **kwargs: Any) -> None:
        """Initialize a custom interface; all arguments are ignored.

        Accepting anything lets a subclass call ``super().__init__(...)``
        with its own arguments.
        """
        ...

    @property
    def has_source(self) -> bool:
        """Whether this built-in interface currently holds live data.

        ``False`` for an interface rebuilt from metadata until ``load``
        attaches data. Only built-in interfaces define it.
        """
        ...

    @classmethod
    def from_metadata(cls, metadata: DataCardMetadata) -> DataInterface:
        """Build an interface instance from a stored card's metadata.

        Card retrieval calls this when given an interface class, as in
        ``cards.data.get(..., interface=MyInterface)``. The default calls
        the class with no arguments; override it when reconstruction needs
        values from ``metadata``.

        Args:
            metadata: metadata parsed from the stored DataCard.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if the default
                implementation cannot construct the class with no arguments.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return the interface metadata stored in the DataCard spec.

        Only built-in interfaces define it. The result never contains live
        Python objects.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` if an
                option value is invalid.
        """
        ...

    def save(self, path: PathLike, save_kwargs: dict[str, Any] | None = ...) -> DataStats:
        """Write this interface's data under a local card directory.

        Built-in interfaces write to a fixed path under ``path``, such as
        ``data/data.parquet`` or ``data/manifest.json``. A subclass override
        must write under ``path`` and return ``DataStats`` for the bytes it
        wrote.

        Args:
            path: local card directory.
            save_kwargs: interface-specific options. The image and text
                interfaces read ``copy_bytes`` (default ``False``) to copy
                referenced files into the card; other built-ins ignore it.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if the interface holds no
                data (always, for the base class) or the data is the wrong
                type, ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                invalid option, or another ``WyrdError`` if writing fails.
        """
        ...

    def load(self, path: PathLike, load_kwargs: dict[str, Any] | None = ...) -> None:
        """Read this interface's data from a local card directory and hold it.

        The artifact location is derived from ``path`` and the interface
        options; the card JSON stores no local path.

        Args:
            path: local card directory.
            load_kwargs: interface-specific options. ``HuggingfaceInterface``
                requires ``allow_remote=True`` to load a remote dataset
                pointer; other built-ins ignore it.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` for the base class or a
                remote Hugging Face load without ``allow_remote``, or another
                ``WyrdError`` if the artifact is missing or cannot be read.
        """
        ...

class PandasInterface(DataInterface):
    """pandas ``DataFrame`` data, saved to ``data/data.parquet``."""

    compression: str

    def __init__(self, *, data: Any = ..., compression: str = ...) -> None:
        """Create a pandas interface.

        Args:
            data: the ``DataFrame`` to save. Omitted, the interface has no
                data until ``load``.
            compression: parquet codec: ``"none"``, ``"snappy"`` (default),
                ``"gzip"``, ``"zstd"``, or ``"lz4"``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``compression``.
        """
        ...

class PolarsInterface(DataInterface):
    """polars ``DataFrame`` data, saved to ``data/data.parquet``."""

    compression: str

    def __init__(self, *, data: Any = ..., compression: str = ...) -> None:
        """Create a polars interface.

        Args:
            data: the ``DataFrame`` to save. Omitted, the interface has no
                data until ``load``.
            compression: parquet codec: ``"none"``, ``"snappy"`` (default),
                ``"gzip"``, ``"zstd"``, or ``"lz4"``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``compression``.
        """
        ...

class ArrowInterface(DataInterface):
    """``pyarrow.Table`` data, saved to ``data/data.parquet`` or ``data/data.arrow``."""

    format: str

    def __init__(self, *, data: Any = ..., format: str = ...) -> None:
        """Create an Arrow interface.

        Args:
            data: the table to save. Omitted, the interface has no data until
                ``load``.
            format: ``"parquet"`` (default) or ``"ipc"`` (Arrow IPC file).

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``format``.
        """
        ...

class ParquetInterface(DataInterface):
    """Parquet data from a local file or a table-like object.

    Saved to ``data/data.parquet``; ``load`` returns a ``pyarrow.Table``.
    """

    compression: str
    row_group_size: int | None

    def __init__(
        self,
        *,
        data: Any = ...,
        compression: str = ...,
        row_group_size: int | None = ...,
    ) -> None:
        """Create a parquet interface.

        Args:
            data: a local parquet file path, copied as-is, or a table-like
                object written with ``pyarrow.parquet``. Omitted, the
                interface has no data until ``load``.
            compression: codec for table-like writes: ``"none"``,
                ``"snappy"`` (default), ``"gzip"``, ``"zstd"``, or ``"lz4"``.
            row_group_size: row group size recorded in the card metadata.
                It is not applied when writing.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``compression``.
        """
        ...

class NumpyInterface(DataInterface):
    """NumPy ``ndarray`` data, saved to ``data/data.npy`` or ``data/data.npz``.

    Saving and loading never use pickle.
    """

    dtype: str | None
    shape: list[int] | None
    format: str

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
            data: the array to save. Omitted, the interface has no data until
                ``load``.
            dtype: declared dtype. Omitted, it is inferred from ``data``.
            shape: declared array shape. Omitted, it is inferred from
                ``data``.
            format: ``"npy"`` (default) or ``"npz"``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``format``.
        """
        ...

class TorchInterface(DataInterface):
    """Torch tensor or tensor-mapping data.

    Saved to ``data/data.safetensors`` or ``data/data.pt``.
    """

    save_format: str

    def __init__(self, *, data: Any = ..., save_format: str = ...) -> None:
        """Create a Torch data interface.

        Args:
            data: a tensor or a mapping of names to tensors. Omitted, the
                interface has no data until ``load``.
            save_format: ``"safetensors"`` (default) or ``"pickle"``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``save_format``.
        """
        ...

class SqlInterface(DataInterface):
    """Named SQL queries, saved to ``data/sql.json``.

    Wyrd records the queries; it never connects to a database.
    """

    dialect: str
    connection_hint: str | None

    def __init__(
        self,
        *,
        data: Any = ...,
        dialect: str,
        connection_hint: str | None = ...,
    ) -> None:
        """Create a SQL interface.

        Args:
            data: a mapping of query name to SQL string, or
                ``{"queries": {...}, "default_query": name}``. Omitted, an
                empty query set is saved.
            dialect: SQL dialect label, such as ``"duckdb"`` or
                ``"postgres"``. Surrounding whitespace is trimmed.
            connection_hint: human-readable hint recorded as metadata.
                Omitted, none is recorded.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` if
                ``dialect`` is blank.
        """
        ...

class JsonlInterface(DataInterface):
    """JSON Lines data from records or a local JSONL file."""

    compression: str
    lines_per_file: int | None

    def __init__(
        self,
        *,
        data: Any = ...,
        compression: str = ...,
        lines_per_file: int | None = ...,
    ) -> None:
        """Create a JSON Lines interface.

        Args:
            data: an iterable of JSON-compatible records or a JSONL file
                path. Omitted, the interface has no data until ``load``.
            compression: ``"none"`` (default), ``"gzip"``, or ``"zstd"``.
            lines_per_file: partition size recorded in the card metadata. It
                is not applied when writing.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``compression``.
        """
        ...

class ImageInterface(DataInterface):
    """Image data described by a file manifest saved to ``data/manifest.json``.

    Pass ``save_kwargs={"copy_bytes": True}`` to ``save`` to also copy the
    referenced files into ``data/images``.
    """

    format: str
    color_mode: str

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
            data: an image directory, an iterable of image paths, or a
                manifest-like value. Omitted, the interface has no data until
                ``load``.
            format: ``"png"``, ``"jpeg"``, ``"webp"``, or ``"mixed"``
                (default).
            color_mode: ``"rgb"`` (default), ``"rgba"``, or ``"grayscale"``.
            manifest_ref: serialized CardRef of an external manifest card.
                Omitted, none is recorded.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_INTERFACE_OPTION`` for an
                unknown ``format`` or ``color_mode``, or a ``WyrdError`` if
                ``manifest_ref`` is not a valid CardRef.
        """
        ...

class TextInterface(DataInterface):
    """Text data described by a file manifest saved to ``data/manifest.json``.

    Pass ``save_kwargs={"copy_bytes": True}`` to ``save`` to also copy the
    referenced files into ``data/files``.
    """

    encoding: str

    def __init__(
        self,
        *,
        data: Any = ...,
        encoding: str = ...,
        manifest_ref: Mapping[str, Any] | None = ...,
    ) -> None:
        """Create a text manifest interface.

        Args:
            data: a text directory, an iterable of text file paths, or a
                manifest-like value. Omitted, the interface has no data until
                ``load``.
            encoding: encoding label recorded in the card. Defaults to
                ``"utf-8"``.
            manifest_ref: serialized CardRef of an external manifest card.
                Omitted, none is recorded.

        Raises:
            WyrdError: If ``manifest_ref`` is not a valid CardRef.
        """
        ...

class HuggingfaceInterface(DataInterface):
    """A Hugging Face dataset saved locally or as a pinned remote pointer.

    With live data, ``save`` writes the dataset to ``data/dataset``. Without
    it, ``save`` writes ``data/dataset_pointer.json``, which requires
    ``revision``; loading that pointer requires
    ``load_kwargs={"allow_remote": True}``.
    """

    dataset_id: str
    revision: str | None
    split: str | None
    config: str | None

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
            data: the dataset object to save locally. Omitted, ``save``
                writes a remote pointer.
            dataset_id: Hub dataset identifier recorded in the card.
            revision: pinned commit: lowercase hex, 7 to 40 characters.
                Omitted, pointer-only saves fail.
            split: dataset split. Omitted, none is recorded.
            config: dataset config name. Omitted, none is recorded.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if ``revision`` is not
                lowercase hex of 7 to 40 characters.
        """
        ...

class Split:
    """One DataCard split strategy; build it with a static method.

    ``strategy`` is the serialized strategy, the same value ``to_dict``
    returns.
    """

    strategy: JsonDict

    @staticmethod
    def column(col: str, op: str, value: Any) -> Split:
        """Select rows whose column satisfies a predicate.

        Args:
            col: column name.
            op: ``"=="``, ``"!="``, ``"<"``, ``"<="``, ``">"``, ``">="``, or
                ``"in"``.
            value: a ``bool``, ``int``, ``float``, ``str``, ``datetime``, or
                a list of those (for ``"in"``).

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_SPLIT_RULE`` for an invalid
                column name, unknown operator, or unsupported value type.
        """
        ...

    @staticmethod
    def materialized(card_ref: Mapping[str, Any] | CardRefLike) -> Split:
        """Use rows already materialized in an Artifact card.

        Args:
            card_ref: a CardRef, or a mapping that serializes to one, whose
                ``kind`` is ``Artifact``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_SPLIT_RULE`` if ``card_ref``
                is not a valid CardRef or does not target an Artifact card.
        """
        ...

    @staticmethod
    def index_range(start: int, stop: int) -> Split:
        """Select the half-open row index range ``[start, stop)``.

        Args:
            start: first included index; not negative.
            stop: first excluded index; at least ``start``.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_SPLIT_RULE`` if a bound is
                negative or ``start > stop``.
        """
        ...

    @staticmethod
    def indices(values: Sequence[int]) -> Split:
        """Select explicit row indices.

        Args:
            values: non-empty, unique, non-negative row indices.

        Raises:
            WyrdError: ``WYRD_DATA_400_INVALID_SPLIT_RULE`` if ``values`` is
                empty or contains a negative or duplicate index.
        """
        ...

    def to_dict(self) -> JsonDict:
        """Return this split strategy as a JSON-compatible dictionary."""
        ...

class DataCardMetadata:
    """Interface, schema, split, SQL, artifact-reference, and statistics
    metadata a DataCard accumulates before it becomes a durable Data spec.

    Only a ``DataCard`` creates it; direct construction raises ``TypeError``."""

    def to_dict(self) -> JsonDict:
        """Return this metadata as a JSON-compatible dictionary for inspection."""
        ...

class DataCard:
    """Local DataCard holder and spec builder.

    A DataCard holds identity, labels, annotations, metadata, and an optional
    live data interface. It never registers itself; registration and
    retrieval belong to ``Cards``:

    ```python
    cards = Cards()
    card = cards.data.get(space="ml", name="training-data", eager_load=True)
    frame = card.data
    ```

    ``get`` returns the validated envelope without data bytes unless
    ``eager_load=True``, which downloads the artifacts and calls ``load``.

    Identity fields are validated when the card is serialized
    (``save``, ``model_dump``, ``as_card_ref``), not at construction.
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

    @property
    def splits(self) -> dict[str, Split]:
        """Named split strategies recorded on the DataCard spec."""
        ...

    @property
    def target_columns(self) -> list[str]:
        """Supervised target column names recorded on the DataCard spec."""
        ...

    @staticmethod
    def from_path(
        path: PathLike,
        interface: DataInterface | type[DataInterface] | CardRefLike | None = ...,
        load_kwargs: DataLoadArgs | Mapping[str, JsonValue] | None = ...,
    ) -> DataCard:
        """Load a DataCard from a saved Card directory or a Card YAML/JSON file.

        A directory is read through the `card.json` that `save` wrote, and its
        data is loaded through the interface. A file is parsed as one Card
        envelope; no data is loaded.

        Raises:
            WyrdError: If the file cannot be read, is not a Data Card envelope,
                or interface loading fails.
        """
        ...

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
        splits: Mapping[str, Split] | None = ...,
        target_columns: Sequence[str] | None = ...,
    ) -> None:
        """Create a DataCard from an initialized data interface.

        The schema is inferred from the interface's live data, if any.

        Args:
            data: a built-in interface or subclass instance. An interface
                class is rejected; pass classes to ``Cards.data.get``.
            space: card space. Omitted, the nearest ``wyrd.toml`` default
                applies, else ``"default"``.
            name: card name. Defaults to ``"data"``.
            version: semantic version. Defaults to ``"0.1.0"``.
            uid: card UID. Omitted, a new UUIDv7 is generated.
            labels: queryable labels. Defaults from ``wyrd.toml`` fill keys
                not given here.
            annotations: free-form annotations. Defaults from ``wyrd.toml``
                fill keys not given here.
            metadata: metadata to start from. Its interface, schema, and SQL
                entries are replaced from ``data``. Omitted, it starts empty.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` for an interface class or
                an invalid label or annotation, or another ``WyrdError`` if
                interface metadata or schema inference fails.
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
        splits: Mapping[str, Split] | None = ...,
        target_columns: Sequence[str] | None = ...,
    ) -> None:
        """Create a DataCard from a local path or a SQL query mapping.

        A ``.parquet`` file gets ``ParquetInterface``, a JSONL file
        ``JsonlInterface``, a directory containing images ``ImageInterface``,
        any other directory ``TextInterface``, and a ``dict``
        ``SqlInterface`` with dialect ``"sql"``.

        Args:
            data: the local path or query mapping.
            space: as for the interface overload.
            name: as for the interface overload.
            version: as for the interface overload.
            uid: as for the interface overload.
            labels: as for the interface overload.
            annotations: as for the interface overload.
            metadata: as for the interface overload.

        Raises:
            WyrdError: ``WYRD_DATA_400_UNKNOWN_DATA_TYPE`` if the path is not
                a supported file or directory, or the errors of the interface
                overload.
        """
        ...

    @overload
    def __init__(
        self,
        data: CardRef,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: DataCardMetadata | None = ...,
        splits: Mapping[str, Split] | None = ...,
        target_columns: Sequence[str] | None = ...,
    ) -> None:
        """Create a DataCard whose data is an existing Artifact card.

        The reference is appended to the metadata's card references and no
        interface is attached.

        Args:
            data (CardRef): Reference with kind `Kind.Artifact` or
                `kind="Artifact"`.
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
                seed before artifact reference attachment.
            splits (Mapping[str, Split] | None): Named split strategies
                recorded on the DataCard spec.
            target_columns (Sequence[str] | None): Supervised target column
                names; each must exist in the schema when one is known.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if the CardRef kind is not
                ``Artifact`` or a label or annotation is invalid.
        """
        ...

    @overload
    def __init__(
        self,
        data: object,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: StringMap | None = ...,
        annotations: StringMap | None = ...,
        metadata: DataCardMetadata | None = ...,
        splits: Mapping[str, Split] | None = ...,
        target_columns: Sequence[str] | None = ...,
    ) -> None:
        """Create a DataCard by detecting the interface for a data object.

        Detection covers pandas and polars DataFrames, ``pyarrow.Table``,
        NumPy arrays, Torch tensors, and Hugging Face datasets, each with the
        default options of its interface.

        Args:
            data: the data object.
            space: as for the interface overload.
            name: as for the interface overload.
            version: as for the interface overload.
            uid: as for the interface overload.
            labels: as for the interface overload.
            annotations: as for the interface overload.
            metadata: as for the interface overload.

        Raises:
            WyrdError: ``WYRD_DATA_400_UNKNOWN_DATA_TYPE`` if no interface
                matches, ``WYRD_DATA_400_INTERFACE_METADATA_REQUIRED`` if a
                Hugging Face dataset id cannot be inferred, or the errors of
                the interface overload.
        """
        ...

    @property
    def data(self) -> object:
        """The live data held by the interface.

        For a custom interface this is its ``data`` attribute.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if no interface is
                attached, it holds no data, or a custom interface has no
                ``data`` attribute.
        """
        ...

    def save(self, path: PathLike, save_kwargs: Mapping[str, JsonValue] | None = ...) -> None:
        """Write the data artifacts and ``card.json`` to a local directory.

        Updates the interface metadata and ``stats`` from the write. Local
        only: nothing is uploaded or registered, and no Artifact cards are
        created.

        Args:
            path: local card directory.
            save_kwargs: a ``dict`` of interface options forwarded to the
                interface's ``save``; see ``DataInterface.save``.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if no interface is
                attached, the errors of the interface's ``save``, or a
                ``WyrdError`` if identity is invalid or ``card.json`` cannot
                be written.
        """
        ...

    def load(
        self,
        path: PathLike | None = ...,
        load_kwargs: DataLoadArgs | Mapping[str, JsonValue] | None = ...,
    ) -> None:
        """Read data into the held interface from a local card directory.

        Args:
            path: local card directory. Omit it only to reload a card
                returned by ``Cards.data.get(eager_load=True)``, which reuses
                the artifacts that call downloaded.
            load_kwargs: options forwarded to the interface's ``load``; see
                ``DataInterface.load``.

        Raises:
            WyrdError: ``WYRD_DATA_400_VALIDATION`` if ``path`` is omitted
                without eager-loaded artifacts or no interface is attached,
                or the errors of the interface's ``load``.
        """
        ...

    def model_dump_json(self) -> str:
        """Return the DataCard envelope as a JSON string.

        Raises:
            WyrdError: If identity fields are invalid.
        """
        ...

    def model_dump(self) -> JsonDict:
        """Return the DataCard envelope as a JSON-compatible dictionary.

        Raises:
            WyrdError: If identity fields are invalid.
        """
        ...

    def _to_card_envelope_json(self) -> str:
        """Return the registry adapter's single envelope conversion."""
        ...

    def as_card_ref(self) -> CardRef:
        """Return a ``Data`` CardRef for this card's identity.

        Raises:
            WyrdError: If name, version, space, or UID is invalid.
        """
        ...

    @staticmethod
    def model_validate_json(
        json_string: str,
        interface: DataInterface | type[DataInterface] | CardRefLike | None = ...,
    ) -> DataCard:
        """Rebuild a DataCard from serialized card JSON.

        The JSON must be a complete ``Data`` envelope with a resolved
        version, space, and UID. No data artifacts are downloaded or read.

        Args:
            json_string: the serialized envelope.
            interface: an interface instance, or an interface class rebuilt
                with its ``from_metadata``. It must match the stored
                interface kind. Omitted, built-in interfaces are rebuilt from
                the stored metadata; a card with a custom interface requires
                this argument. A CardRef is rejected.

        Raises:
            WyrdError: If the JSON or envelope is invalid, the stored
                interface is custom and ``interface`` is omitted, or
                ``interface`` is a CardRef or does not match the stored
                interface.
        """
        ...

__all__ = [
    "ArrowInterface",
    "DataCard",
    "DataCardMetadata",
    "DataInterface",
    "DataSchema",
    "DataStats",
    "Dim",
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
