"""DataCard authoring, split declaration, and local materialization."""

from __future__ import annotations

import datetime as dt
import gzip
import hashlib
import importlib
import json
import os
import shutil
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


class WyrdError(Exception):
    """Python-facing Wyrd error with a stable code and remediation text."""

    def __init__(
        self,
        code: str,
        message: str,
        *,
        details: dict[str, Any] | None = None,
        remediation: str = "Fix the DataCard input and retry.",
    ) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.details = details
        self.remediation = remediation


def _validation(message: str, *, code: str = "WYRD_DATA_400_VALIDATION") -> WyrdError:
    return WyrdError(code, message)


@dataclass(slots=True)
class FieldSpec:
    """One field in a DataCard schema.

    `shape` stores optional dimension metadata for array-like values. `extra`
    is reserved for small string metadata that belongs on the field itself.
    """

    name: str
    dtype: str
    shape: list[dict[str, Any]] = field(default_factory=list)
    nullable: bool = False
    extra: dict[str, str] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "name": self.name,
            "dtype": self.dtype,
            "nullable": self.nullable,
        }
        if self.shape:
            result["shape"] = self.shape
        if self.extra:
            result["extra"] = dict(self.extra)
        return result


@dataclass(slots=True)
class DataSchema:
    """Ordered schema inferred from or supplied for a DataCard."""

    columns: list[FieldSpec] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {"columns": [column.to_dict() for column in self.columns]}


@dataclass(slots=True)
class DataStats:
    """Local byte and shape statistics recorded in serialized DataCard data."""

    byte_count: int
    sha256: str
    row_count: int | None = None
    col_count: int | None = None

    @classmethod
    def from_bytes(cls, payload: bytes, schema: DataSchema | None = None) -> DataStats:
        return cls(
            byte_count=max(len(payload), 1),
            sha256=hashlib.sha256(payload or b"wyrd-empty-local-data").hexdigest(),
            col_count=len(schema.columns) if schema is not None else None,
        )

    @classmethod
    def from_file(cls, path: str | os.PathLike[str], schema: DataSchema | None = None) -> DataStats:
        """Read a local file and compute DataCard byte statistics."""
        payload = Path(path).read_bytes()
        return cls.from_bytes(payload, schema)

    def to_dict(self) -> dict[str, Any]:
        result: dict[str, Any] = {"byte_count": self.byte_count, "sha256": self.sha256}
        if self.row_count is not None:
            result["row_count"] = self.row_count
        if self.col_count is not None:
            result["col_count"] = self.col_count
        return result


class DataInterface:
    """Base class for local DataCard interface adapters.

    Interface objects hold optional live local data, describe how that data is
    serialized, and know how to save or load only local filesystem material.
    """

    kind = "DataInterface"

    def __init__(self) -> None:
        if type(self) is DataInterface:
            raise _validation("DataInterface is abstract; use a concrete data interface class.")

    @property
    def has_source(self) -> bool:
        return getattr(self, "data", None) is not None

    def to_dict(self) -> dict[str, Any]:
        return {"kind": self.kind, "meta": self.meta()}

    def meta(self) -> dict[str, Any]:
        raise NotImplementedError

    def infer_schema(self) -> DataSchema:
        return infer_schema(self.kind, getattr(self, "data", None))

    def save(
        self, path: str | os.PathLike[str], save_kwargs: dict[str, Any] | None = None
    ) -> dict[str, Any]:
        """Write this interface's local data under `path` and return save metadata."""
        return save_interface(self, Path(path), save_kwargs or {})

    def load(
        self,
        path: str | os.PathLike[str],
        metadata: dict[str, Any],
        load_kwargs: dict[str, Any] | None = None,
    ) -> None:
        """Read local materialized data from `path` and replace `self.data`."""
        self.data = load_interface(self, Path(path), metadata, load_kwargs or {})


class PandasInterface(DataInterface):
    """DataCard interface for pandas DataFrame values serialized as parquet."""

    kind = "Pandas"

    def __init__(self, *, data: Any = None, compression: str = "snappy") -> None:
        self.data = data
        self.compression = _parse_option(
            "compression", compression, {"none", "snappy", "gzip", "zstd", "lz4"}
        )

    def meta(self) -> dict[str, Any]:
        return {
            "framework_version": _module_version("pandas"),
            "compression": _title(self.compression),
        }


class PolarsInterface(DataInterface):
    """DataCard interface for polars DataFrame values serialized as parquet."""

    kind = "Polars"

    def __init__(self, *, data: Any = None, compression: str = "snappy") -> None:
        self.data = data
        self.compression = _parse_option(
            "compression", compression, {"none", "snappy", "gzip", "zstd", "lz4"}
        )

    def meta(self) -> dict[str, Any]:
        return {
            "framework_version": _module_version("polars"),
            "compression": _title(self.compression),
        }


class ArrowInterface(DataInterface):
    """DataCard interface for pyarrow Tables serialized as parquet or IPC."""

    kind = "Arrow"

    def __init__(self, *, data: Any = None, format: str = "parquet") -> None:
        self.data = data
        self.format = _parse_option("format", format, {"parquet", "ipc"})

    def meta(self) -> dict[str, Any]:
        return {"framework_version": _module_version("pyarrow"), "format": _title(self.format)}


class ParquetInterface(DataInterface):
    """DataCard interface for parquet files or table-like parquet sources."""

    kind = "Parquet"

    def __init__(
        self,
        *,
        data: Any = None,
        compression: str = "snappy",
        row_group_size: int | None = None,
    ) -> None:
        self.data = data
        self.compression = _parse_option(
            "compression", compression, {"none", "snappy", "gzip", "zstd", "lz4"}
        )
        self.row_group_size = row_group_size

    def meta(self) -> dict[str, Any]:
        result: dict[str, Any] = {"compression": _title(self.compression)}
        if self.row_group_size is not None:
            result["row_group_size"] = self.row_group_size
        return result


class NumpyInterface(DataInterface):
    """DataCard interface for numpy arrays serialized as `.npy` or `.npz` files."""

    kind = "Numpy"

    def __init__(
        self,
        *,
        data: Any = None,
        dtype: str | None = None,
        shape: list[int] | None = None,
        format: str = "npy",
    ) -> None:
        self.data = data
        self.dtype = dtype
        self.shape = shape
        self.format = _parse_option("format", format, {"npy", "npz"})

    def meta(self) -> dict[str, Any]:
        dtype = self.dtype or _numpy_dtype(self.data) or "unknown"
        shape = self.shape or _shape(self.data)
        return {"dtype": dtype, "shape": shape, "format": _title(self.format)}


class TorchInterface(DataInterface):
    """DataCard interface for torch tensors or tensor maps."""

    kind = "Torch"

    def __init__(self, *, data: Any = None, save_format: str = "safetensors") -> None:
        self.data = data
        self.save_format = _parse_option("save_format", save_format, {"safetensors", "pickle"})

    def meta(self) -> dict[str, Any]:
        return {
            "framework_version": _module_version("torch"),
            "save_format": "Safetensors" if self.save_format == "safetensors" else "Pickle",
        }


class SqlInterface(DataInterface):
    """DataCard interface for local SQL query bundles."""

    kind = "Sql"

    def __init__(
        self,
        *,
        data: Any = None,
        dialect: str,
        connection_hint: str | None = None,
    ) -> None:
        self.data = data
        self.dialect = dialect
        self.connection_hint = connection_hint

    def meta(self) -> dict[str, Any]:
        result: dict[str, Any] = {"dialect": self.dialect}
        if self.connection_hint is not None:
            result["connection_hint"] = self.connection_hint
        return result


class JsonlInterface(DataInterface):
    """DataCard interface for JSON Lines rows or JSONL files."""

    kind = "Jsonl"

    def __init__(
        self,
        *,
        data: Any = None,
        compression: str = "none",
        lines_per_file: int | None = None,
    ) -> None:
        self.data = data
        self.compression = _parse_option("compression", compression, {"none", "gzip", "zstd"})
        self.lines_per_file = lines_per_file

    def meta(self) -> dict[str, Any]:
        result: dict[str, Any] = {"compression": _title(self.compression)}
        if self.lines_per_file is not None:
            result["lines_per_file"] = self.lines_per_file
        return result


class ImageInterface(DataInterface):
    """DataCard interface for image file manifests."""

    kind = "Image"

    def __init__(
        self,
        *,
        data: Any = None,
        format: str = "mixed",
        color_mode: str = "rgb",
        manifest_ref: Mapping[str, Any] | None = None,
    ) -> None:
        self.data = data
        self.format = _parse_option("format", format, {"png", "jpeg", "webp", "mixed"})
        self.color_mode = _parse_option("color_mode", color_mode, {"rgb", "rgba", "grayscale"})
        self.manifest_ref = dict(manifest_ref) if manifest_ref is not None else None

    def meta(self) -> dict[str, Any]:
        result: dict[str, Any] = {
            "format": _title(self.format),
            "color_mode": _color(self.color_mode),
        }
        if self.manifest_ref is not None:
            result["manifest_ref"] = self.manifest_ref
        return result


class TextInterface(DataInterface):
    """DataCard interface for text file manifests."""

    kind = "Text"

    def __init__(
        self,
        *,
        data: Any = None,
        encoding: str = "utf-8",
        manifest_ref: Mapping[str, Any] | None = None,
    ) -> None:
        self.data = data
        self.encoding = encoding
        self.manifest_ref = dict(manifest_ref) if manifest_ref is not None else None

    def meta(self) -> dict[str, Any]:
        result: dict[str, Any] = {"encoding": self.encoding}
        if self.manifest_ref is not None:
            result["manifest_ref"] = self.manifest_ref
        return result


class HuggingfaceInterface(DataInterface):
    """DataCard interface for local Hugging Face datasets or dataset pointers."""

    kind = "Huggingface"

    def __init__(
        self,
        *,
        data: Any = None,
        dataset_id: str,
        revision: str | None = None,
        split: str | None = None,
        config: str | None = None,
    ) -> None:
        if revision is not None and not _is_hex_revision(revision):
            raise _validation("HuggingFace revision must be 7 to 40 lowercase hex characters.")
        self.data = data
        self.dataset_id = dataset_id
        self.revision = revision
        self.split = split
        self.config = config

    def meta(self) -> dict[str, Any]:
        result: dict[str, Any] = {"dataset_id": self.dataset_id}
        for key in ("revision", "split", "config"):
            value = getattr(self, key)
            if value is not None:
                result[key] = value
        return result


class CustomDataInterface(DataInterface):
    """DataCard interface delegated to a user-provided loader class."""

    kind = "Custom"

    def __init__(
        self,
        *,
        data: Any = None,
        loader_module: str,
        loader_class: str,
        extra: Mapping[str, str] | None = None,
    ) -> None:
        self.data = data
        self.loader_module = loader_module
        self.loader_class = loader_class
        self.extra = dict(extra or {})

    def meta(self) -> dict[str, Any]:
        return {
            "loader_module": self.loader_module,
            "loader_class": self.loader_class,
            "extra": dict(self.extra),
        }


class Split:
    """Declared DataCard split strategy.

    Splits are serialized declarations only. They do not execute filtering or
    materialize data when attached to a DataCard.
    """

    def __init__(self, strategy: dict[str, Any]) -> None:
        self.strategy = strategy

    @staticmethod
    def column(col: str, op: str, value: Any) -> Split:
        op_map = {"==": "Eq", "!=": "Ne", ">": "Gt", ">=": "Ge", "<": "Lt", "<=": "Le", "in": "In"}
        if op not in op_map:
            raise _validation(
                f"invalid split operator {op!r}",
                code="WYRD_DATA_400_INVALID_SPLIT_RULE",
            )
        return Split({"kind": "Column", "value": {"name": col, "op": op_map[op], "value": value}})

    @staticmethod
    def materialized(artifact_ref: Mapping[str, Any]) -> Split:
        if artifact_ref.get("kind") != "Artifact":
            raise _validation(
                "materialized split references must target Artifact cards",
                code="WYRD_DATA_400_INVALID_SPLIT_RULE",
            )
        return Split({"kind": "Materialized", "value": dict(artifact_ref)})

    @staticmethod
    def index_range(start: int, stop: int) -> Split:
        if start < 0 or stop < 0 or start > stop:
            raise _validation(
                "index range splits require non-negative start <= stop",
                code="WYRD_DATA_400_INVALID_SPLIT_RULE",
            )
        return Split({"kind": "IndexRange", "value": {"start": start, "stop": stop}})

    @staticmethod
    def indices(values: list[int]) -> Split:
        if any(value < 0 for value in values) or len(set(values)) != len(values):
            raise _validation(
                "indices splits require unique non-negative values",
                code="WYRD_DATA_400_INVALID_SPLIT_RULE",
            )
        return Split({"kind": "Indices", "value": list(values)})

    def to_dict(self) -> dict[str, Any]:
        return dict(self.strategy)


class DataCard:
    """Local DataCard holder and spec builder.

    A DataCard can serialize local data to disk with `save()` and hydrate it
    back with `load()`. Registration is intentionally outside this class.
    """

    def __init__(
        self,
        data: Any,
        space: str | None = None,
        name: str | None = None,
        version: str | None = None,
        uid: str | None = None,
        tags: list[str] | None = None,
        metadata: Mapping[str, Any] | None = None,
    ) -> None:
        self.space = space or "default"
        self.name = name or "data"
        self.version = version or "0.1.0"
        self.uid = uid or _new_uid()
        self.tags = list(tags or [])
        self.created_at = dt.datetime.now(dt.timezone.utc).isoformat()
        self.is_card = True
        self._metadata = dict(metadata or {})
        self.interface = _coerce_interface(data)
        self._save_metadata: dict[str, Any] | None = None
        self.schema = self.interface.infer_schema()
        self.stats = DataStats.from_bytes(_stable_payload(self.interface.data), self.schema)

    @property
    def metadata(self) -> dict[str, Any]:
        return self._metadata

    @metadata.setter
    def metadata(self, value: Mapping[str, Any]) -> None:
        self._metadata = dict(value)

    @property
    def data(self) -> Any:
        if self.interface.data is None:
            raise _validation("DataCard has no live local data attached.")
        return self.interface.data

    def save(self, path: str | os.PathLike[str], save_kwargs: dict[str, Any] | None = None) -> None:
        """Materialize local data under `path` and write `card.json`."""
        root = Path(path)
        root.mkdir(parents=True, exist_ok=True)
        self._save_metadata = self.interface.save(root, save_kwargs)
        self.schema = _schema_from_metadata(self._save_metadata)
        self.stats = DataStats(**self._save_metadata["stats"])
        self.save_card(root)

    def load(
        self,
        path: str | os.PathLike[str] | None = None,
        load_kwargs: dict[str, Any] | None = None,
    ) -> None:
        """Hydrate local data from `path` into this card's interface."""
        if path is None:
            raise _validation("local artifact path is required for DataCard.load")
        if self._save_metadata is None:
            card_path = Path(path) / "card.json"
            loaded = json.loads(card_path.read_text(encoding="utf-8"))
            self._save_metadata = loaded["spec"].get("save_metadata")
        if self._save_metadata is None:
            raise _validation("DataCard local save metadata is required for local load")
        self.interface.load(Path(path), self._save_metadata, load_kwargs)

    def save_card(self, path: str | os.PathLike[str]) -> None:
        """Write only the serialized DataCard envelope to `path/card.json`."""
        root = Path(path)
        root.mkdir(parents=True, exist_ok=True)
        (root / "card.json").write_text(self.model_dump_json(indent=2), encoding="utf-8")

    def model_dump_json(self, *, indent: int | None = None) -> str:
        """Return serialized Wyrd card JSON without performing filesystem IO."""
        return json.dumps(self.to_dict(), indent=indent, sort_keys=True, default=str)

    @staticmethod
    def model_validate_json(json_string: str, data: Any = None) -> DataCard:
        """Build a DataCard from serialized Wyrd card JSON."""
        payload = json.loads(json_string)
        metadata = payload.get("metadata", {})
        spec = payload.get("spec", {})
        interface = data if data is not None else _interface_from_spec(spec.get("interface", {}))
        card = DataCard(
            interface,
            space=metadata.get("space"),
            name=metadata.get("name"),
            version=metadata.get("version"),
            uid=metadata.get("uid"),
            tags=metadata.get("tags", []),
            metadata=metadata.get("annotations", {}),
        )
        card._save_metadata = spec.get("save_metadata")
        return card

    def to_dict(self) -> dict[str, Any]:
        """Return the serialized Wyrd card envelope as a mutable dict."""
        spec = {
            "interface": self.interface.to_dict(),
            "schema": self.schema.to_dict(),
            "artifact_refs": [],
            "splits": {},
            "target_columns": [],
            "sql": _sql_logic(self.interface),
            "stats": self.stats.to_dict(),
        }
        if self._save_metadata is not None:
            spec["save_metadata"] = self._save_metadata
        return {
            "apiVersion": "wyrd/v1",
            "kind": "Data",
            "metadata": {
                "space": self.space,
                "name": self.name,
                "version": self.version,
                "uid": self.uid,
                "tags": list(self.tags),
                "annotations": dict(self._metadata),
            },
            "spec": spec,
        }


def infer_schema(kind: str, data: Any) -> DataSchema:
    if data is None:
        return DataSchema([])
    if kind == "Pandas":
        return _pandas_schema(data)
    if kind == "Polars":
        return _polars_schema(data)
    if kind in {"Arrow", "Parquet"}:
        return _arrow_schema(data)
    if kind == "Numpy":
        return DataSchema([FieldSpec("value", _numpy_dtype(data) or "unknown", _shape_dims(data))])
    if kind == "Torch":
        return DataSchema([FieldSpec("value", _torch_dtype(data) or "unknown", _shape_dims(data))])
    return DataSchema([])


def save_interface(
    interface: DataInterface,
    path: Path,
    save_kwargs: dict[str, Any],
) -> dict[str, Any]:
    data_dir = path / "data"
    data_dir.mkdir(parents=True, exist_ok=True)
    kind = interface.kind
    if kind == "Pandas":
        rel = Path("data/data.parquet")
        _require_source(interface, "PandasInterface.save requires a pandas.DataFrame")
        interface.data.to_parquet(path / rel, engine="pyarrow", compression=interface.compression)
        return _metadata(interface, rel, "parquet", interface.compression)
    if kind == "Polars":
        rel = Path("data/data.parquet")
        _require_source(interface, "PolarsInterface.save requires a polars.DataFrame")
        interface.data.write_parquet(path / rel, compression=interface.compression)
        return _metadata(interface, rel, "parquet", interface.compression)
    if kind == "Arrow":
        _require_source(interface, "ArrowInterface.save requires a pyarrow.Table")
        if interface.format == "ipc":
            rel = Path("data/data.arrow")
            import pyarrow as pa
            import pyarrow.ipc as ipc

            with pa.OSFile(str(path / rel), "wb") as sink:
                with ipc.new_file(sink, interface.data.schema) as writer:
                    writer.write_table(interface.data)
            return _metadata(interface, rel, "ipc", None)
        rel = Path("data/data.parquet")
        import pyarrow.parquet as pq

        pq.write_table(interface.data, path / rel)
        return _metadata(interface, rel, "parquet", None)
    if kind == "Parquet":
        rel = Path("data/data.parquet")
        _require_source(
            interface,
            "ParquetInterface.save requires a parquet path or table-like source",
        )
        if isinstance(interface.data, str | os.PathLike):
            source = Path(interface.data)
            if save_kwargs.get("reference_only"):
                return _metadata(interface, source, "parquet", interface.compression)
            shutil.copyfile(source, path / rel)
        else:
            import pyarrow.parquet as pq

            pq.write_table(interface.data, path / rel, compression=interface.compression)
        return _metadata(interface, rel, "parquet", interface.compression)
    if kind == "Numpy":
        rel = Path("data/data.npy" if interface.format == "npy" else "data/data.npz")
        _require_source(interface, "NumpyInterface.save requires a numpy.ndarray")
        import numpy as np

        if interface.format == "npz":
            np.savez(path / rel, value=interface.data)
        else:
            np.save(path / rel, interface.data, allow_pickle=False)
        return _metadata(interface, rel, interface.format, None)
    if kind == "Torch":
        _require_source(interface, "TorchInterface.save requires torch data")
        if interface.save_format == "safetensors":
            rel = Path("data/data.safetensors")
            from safetensors.torch import save_file

            save_file(_torch_map(interface.data), path / rel)
            return _metadata(interface, rel, "safetensors", None)
        rel = Path("data/data.pt")
        import torch

        torch.save(interface.data, path / rel)
        return _metadata(interface, rel, "pickle", None)
    if kind == "Sql":
        rel = Path("data/queries.json")
        bundle = _sql_logic(interface) or {"queries": {}, "default_query": None}
        (path / rel).write_text(json.dumps(bundle, sort_keys=True), encoding="utf-8")
        return _metadata(interface, rel, "sql-json", None)
    if kind == "Jsonl":
        rel = Path("data/data.jsonl.gz" if interface.compression == "gzip" else "data/data.jsonl")
        _write_jsonl(interface.data, path / rel, interface.compression)
        return _metadata(interface, rel, "jsonl", interface.compression)
    if kind in {"Image", "Text"}:
        rel = Path("data/manifest.json")
        manifest = _manifest(interface.data)
        (path / rel).write_text(json.dumps(manifest, sort_keys=True), encoding="utf-8")
        return _metadata(interface, rel, f"{kind.lower()}-manifest", None)
    if kind == "Huggingface":
        if interface.data is not None and hasattr(interface.data, "save_to_disk"):
            rel = Path("data/dataset")
            interface.data.save_to_disk(path / rel)
            return _metadata(interface, rel, "huggingface-disk", None)
        rel = Path("data/dataset.json")
        (path / rel).write_text(json.dumps(interface.meta(), sort_keys=True), encoding="utf-8")
        return _metadata(interface, rel, "huggingface-pointer", None)
    if kind == "Custom":
        rel = Path("data/custom")
        loader = _custom_loader(interface)
        loader.save(interface.data, path / rel, **interface.extra)
        return _metadata(interface, rel, "custom", None)
    raise _validation(f"unsupported interface kind {kind!r}")


def load_interface(
    interface: DataInterface,
    path: Path,
    metadata: dict[str, Any],
    load_kwargs: dict[str, Any],
) -> Any:
    rel = Path(metadata["relative_path"])
    absolute = path / rel
    if interface.kind == "Pandas":
        import pandas as pd

        return pd.read_parquet(absolute, engine="pyarrow")
    if interface.kind == "Polars":
        import polars as pl

        return pl.read_parquet(absolute)
    if interface.kind == "Arrow":
        if metadata["save_format"] == "ipc":
            import pyarrow.ipc as ipc

            with ipc.open_file(absolute) as reader:
                return reader.read_all()
        import pyarrow.parquet as pq

        return pq.read_table(absolute)
    if interface.kind == "Parquet":
        import pyarrow.parquet as pq

        return pq.read_table(absolute)
    if interface.kind == "Numpy":
        import numpy as np

        loaded = np.load(absolute, allow_pickle=False)
        return loaded["value"] if metadata["save_format"] == "npz" else loaded
    if interface.kind == "Torch":
        if metadata["save_format"] == "safetensors":
            from safetensors.torch import load_file

            return load_file(absolute)
        import torch

        return torch.load(absolute, weights_only=True)
    if interface.kind == "Sql":
        return json.loads(absolute.read_text(encoding="utf-8"))
    if interface.kind == "Jsonl":
        return _read_jsonl(absolute, metadata.get("compression"))
    if interface.kind == "Huggingface":
        if metadata["save_format"] == "huggingface-disk":
            from datasets import load_from_disk

            return load_from_disk(absolute)
        if absolute.is_dir():
            return absolute
        return json.loads(absolute.read_text(encoding="utf-8"))
    if interface.kind in {"Image", "Text"}:
        if absolute.is_dir():
            return absolute
        return json.loads(absolute.read_text(encoding="utf-8"))
    if interface.kind == "Custom":
        return _custom_loader(interface).load(absolute, **interface.extra)
    raise _validation(f"unsupported interface kind {interface.kind!r}")


def _metadata(
    interface: DataInterface,
    relative_path: Path,
    save_format: str,
    compression: str | None,
) -> dict[str, Any]:
    path = relative_path if relative_path.is_absolute() else Path(relative_path)
    absolute = path if path.is_absolute() else None
    schema = interface.infer_schema()
    stats = (
        DataStats.from_file(absolute, schema)
        if absolute is not None and absolute.is_file()
        else DataStats.from_bytes(_stable_payload(interface.data), schema)
    )
    return {
        "interface": interface.to_dict(),
        "relative_path": str(relative_path),
        "schema": schema.to_dict(),
        "stats": stats.to_dict(),
        "save_format": save_format,
        "compression": compression,
        "row_count": stats.row_count,
        "column_count": stats.col_count,
        "extra": {},
    }


def _coerce_interface(data: Any) -> DataInterface:
    if isinstance(data, DataInterface):
        return data
    module = type(data).__module__
    name = type(data).__name__
    if module.startswith("pandas") and name == "DataFrame":
        return PandasInterface(data=data)
    if module.startswith("polars") and name == "DataFrame":
        return PolarsInterface(data=data)
    if module.startswith("pyarrow") and name == "Table":
        return ArrowInterface(data=data)
    if module.startswith("numpy") and name == "ndarray":
        return NumpyInterface(data=data)
    if module.startswith("torch") and "Tensor" in name:
        return TorchInterface(data=data)
    if isinstance(data, Mapping) and "queries" in data:
        return SqlInterface(data=data, dialect=str(data.get("dialect", "sql")))
    if isinstance(data, str | os.PathLike):
        path = Path(data)
        suffixes = "".join(path.suffixes).lower()
        if path.is_dir():
            return TextInterface(data=path)
        if suffixes.endswith(".parquet"):
            return ParquetInterface(data=path)
        if ".jsonl" in suffixes:
            compression = "gzip" if suffixes.endswith(".gz") else "none"
            return JsonlInterface(data=path, compression=compression)
    raise _validation(
        f"unsupported DataCard data input {type(data).__name__}",
        code="WYRD_DATA_400_UNKNOWN_DATA_TYPE",
    )


def _interface_from_spec(interface: dict[str, Any]) -> DataInterface:
    kind = interface.get("kind")
    meta = interface.get("meta", {})
    mapping = {
        "Pandas": lambda: PandasInterface(
            compression=str(meta.get("compression", "snappy")).lower()
        ),
        "Polars": lambda: PolarsInterface(
            compression=str(meta.get("compression", "snappy")).lower()
        ),
        "Arrow": lambda: ArrowInterface(format=str(meta.get("format", "parquet")).lower()),
        "Parquet": lambda: ParquetInterface(
            compression=str(meta.get("compression", "snappy")).lower()
        ),
        "Numpy": lambda: NumpyInterface(
            dtype=meta.get("dtype"),
            shape=meta.get("shape"),
            format=str(meta.get("format", "npy")).lower(),
        ),
        "Torch": lambda: TorchInterface(
            save_format=str(meta.get("save_format", "safetensors")).lower()
        ),
        "Sql": lambda: SqlInterface(
            dialect=meta.get("dialect", "sql"),
            connection_hint=meta.get("connection_hint"),
        ),
        "Jsonl": lambda: JsonlInterface(compression=str(meta.get("compression", "none")).lower()),
        "Image": lambda: ImageInterface(
            format=str(meta.get("format", "mixed")).lower(),
            color_mode=str(meta.get("color_mode", "rgb")).lower(),
        ),
        "Text": lambda: TextInterface(encoding=meta.get("encoding", "utf-8")),
        "Huggingface": lambda: HuggingfaceInterface(dataset_id=meta.get("dataset_id", "unknown")),
        "Custom": lambda: CustomDataInterface(
            loader_module=meta.get("loader_module", ""),
            loader_class=meta.get("loader_class", ""),
        ),
    }
    if kind not in mapping:
        raise _validation(f"unsupported serialized interface kind {kind!r}")
    return mapping[kind]()


def _schema_from_metadata(metadata: dict[str, Any]) -> DataSchema:
    return DataSchema([FieldSpec(**column) for column in metadata["schema"].get("columns", [])])


def _pandas_schema(data: Any) -> DataSchema:
    return DataSchema(
        [FieldSpec(str(name), _canonical_dtype(str(dtype))) for name, dtype in data.dtypes.items()]
    )


def _polars_schema(data: Any) -> DataSchema:
    return DataSchema(
        [FieldSpec(str(name), _canonical_dtype(str(dtype))) for name, dtype in data.schema.items()]
    )


def _arrow_schema(data: Any) -> DataSchema:
    schema = getattr(data, "schema", None)
    if schema is None:
        return DataSchema([])
    return DataSchema(
        [FieldSpec(str(field.name), _canonical_dtype(str(field.type))) for field in schema]
    )


def _canonical_dtype(value: str) -> str:
    raw = value.lower().replace("boolean", "bool").replace("string", "utf8")
    table = {
        "object": "utf8",
        "category": "dictionary<int32, utf8>",
        "categorical": "dictionary<int32, utf8>",
        "datetime64[ns]": "timestamp[ns]",
        "date": "date32",
        "large_string": "utf8",
        "large_string()": "utf8",
    }
    if raw in table:
        return table[raw]
    for prefix in ("int", "uint", "float", "bool", "utf8", "binary", "timestamp", "duration"):
        if raw.startswith(prefix):
            return raw
    if raw in {"str", "string[python]", "string[pyarrow]"}:
        return "utf8"
    raise _validation(f"unknown dtype {value!r}", code="WYRD_DATA_400_UNKNOWN_DATA_TYPE")


def _numpy_dtype(data: Any) -> str | None:
    dtype = getattr(data, "dtype", None)
    return _canonical_dtype(str(dtype)) if dtype is not None else None


def _torch_dtype(data: Any) -> str | None:
    dtype = getattr(data, "dtype", None)
    if dtype is None:
        return None
    return str(dtype).replace("torch.", "")


def _shape(data: Any) -> list[int]:
    return [int(value) for value in getattr(data, "shape", [])]


def _shape_dims(data: Any) -> list[dict[str, Any]]:
    return [{"kind": "Fixed", "value": value} for value in _shape(data)]


def _stable_payload(data: Any) -> bytes:
    try:
        return repr(data).encode("utf-8")
    except Exception:
        return type(data).__name__.encode("utf-8")


def _module_version(name: str) -> str:
    try:
        module = importlib.import_module(name)
    except ModuleNotFoundError:
        return "unknown"
    return str(getattr(module, "__version__", "unknown"))


def _parse_option(field_name: str, value: str, allowed: set[str]) -> str:
    normalized = value.lower()
    if normalized not in allowed:
        allowed_values = ", ".join(sorted(allowed))
        raise _validation(f"{field_name} must be one of: {allowed_values}")
    return normalized


def _title(value: str) -> str:
    return "None" if value == "none" else value.capitalize()


def _color(value: str) -> str:
    return {"rgb": "Rgb", "rgba": "Rgba", "grayscale": "Grayscale"}[value]


def _new_uid() -> str:
    return hashlib.sha256(os.urandom(16)).hexdigest()[:26]


def _is_hex_revision(value: str) -> bool:
    return 7 <= len(value) <= 40 and all(ch in "0123456789abcdef" for ch in value)


def _require_source(interface: DataInterface, message: str) -> None:
    if interface.data is None:
        raise _validation(message)


def _sql_logic(interface: DataInterface) -> dict[str, Any] | None:
    if not isinstance(interface, SqlInterface):
        return None
    if interface.data is None:
        return {"queries": {}, "default_query": None}
    queries = interface.data.get("queries", interface.data)
    default_query = interface.data.get("default_query")
    return {"queries": dict(queries), "default_query": default_query}


def _write_jsonl(data: Any, path: Path, compression: str) -> None:
    if isinstance(data, str | os.PathLike):
        source = Path(data)
        if compression == "gzip":
            with source.open("rb") as src, gzip.open(path, "wb") as dst:
                shutil.copyfileobj(src, dst)
        else:
            shutil.copyfile(source, path)
        return
    rows: Iterable[Any] = data if data is not None else []
    opener = gzip.open if compression == "gzip" else open
    with opener(path, "wt", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True))
            handle.write("\n")


def _read_jsonl(path: Path, compression: str | None) -> list[Any]:
    opener = gzip.open if compression == "gzip" else open
    with opener(path, "rt", encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def _manifest(data: Any) -> list[dict[str, str]]:
    root = Path(data)
    if root.is_file():
        paths = [root]
    else:
        paths = sorted(path for path in root.rglob("*") if path.is_file())
    return [{"path": str(path)} for path in paths]


def _torch_map(data: Any) -> dict[str, Any]:
    if isinstance(data, Mapping):
        return dict(data)
    return {"value": data}


def _custom_loader(interface: CustomDataInterface) -> Any:
    module = importlib.import_module(interface.loader_module)
    return getattr(module, interface.loader_class)


__all__ = [
    "ArrowInterface",
    "CustomDataInterface",
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
