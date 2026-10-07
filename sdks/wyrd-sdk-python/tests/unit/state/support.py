"""Custom Model and Data interfaces that hydrate the checked-in test bundles."""

from __future__ import annotations

import hashlib
from pathlib import Path
from typing import Any

from wyrd.data import DataInterface, DataStats
from wyrd.model import ModelInterface


class TinyModelInterface(ModelInterface):
    """Test-only Model interface for proving offline loader delegation.

    The fixture writes deterministic bytes below a caller-owned bundle path,
    records the artifact directory and kwargs passed by Rust, and exposes
    identity callables as loaded model and preprocessor values. It performs no
    network access; the ``model`` and ``preprocessor`` attributes are deliberate
    test invariants rather than production inference implementations.
    """

    def __init__(self) -> None:
        """Initialize empty holder slots and loader telemetry for assertions.

        The superclass is initialized first so the object remains a valid
        ``ModelInterface``; all additional fields are test-only observations.
        """
        super().__init__()
        self.model: Any = None
        self.preprocessor: Any = None
        self.processor: Any = None
        self.loaded_path: Path | None = None
        self.loaded_kwargs: dict[str, Any] | None = None
        # Lets a GC test hold a state -> interface -> state reference cycle.
        self.state: object | None = None

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> None:
        """Write deterministic fixture bytes below ``path``.

        Args:
            path: Artifact-root directory supplied by the hydration runtime.
            save_kwargs: Ignored test-only save options.
        Side effects:
            Creates ``path/model/tiny.bin`` on the local filesystem.
        """
        del save_kwargs
        output = path / "model" / "tiny.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(b"tiny-model")

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        """Record local load inputs and expose deterministic callable values.

        Args:
            path: Verified bundle-local artifact directory.
            load_kwargs: Alias-specific options forwarded by ``WyrdState``.
        Side effects:
            Reads no bytes and performs no network access; it only records the
            path/options and sets the test holder attributes.
        """
        self.loaded_path = path
        self.loaded_kwargs = dict(load_kwargs or {})
        self.model = lambda value: value
        self.preprocessor = lambda value: value
        self.processor = None


class TinyDataInterface(DataInterface):
    """Test-only Data interface for proving local dataset hydration.

    Loading records the verified artifact directory and publishes a tiny
    deterministic mapping. The mapping is intentionally a test-only invariant
    used to prove the public DataCard holder remains alive after hydration.
    """

    def __init__(self) -> None:
        """Initialize empty data state and test-only loader telemetry.

        The constructor performs no filesystem or network work. Its fields are
        mutable observations used to verify that Rust hydration forwards the
        confined artifact path and preserves the loaded DataCard holder.
        """
        super().__init__()
        self.data: Any = None
        self.loaded_path: Path | None = None
        self.loaded_kwargs: dict[str, Any] | None = None

    def save(self, path: Path, save_kwargs: dict[str, Any] | None = None) -> DataStats:
        """Write deterministic fixture bytes below ``path`` and report them.

        Args:
            path: Artifact-root directory supplied by the hydration runtime.
            save_kwargs: Ignored test-only save options.
        Returns:
            Byte count and SHA-256 digest for the created payload.
        Side effects:
            Creates ``path/data/tiny.bin`` on the local filesystem.
        """
        del save_kwargs
        output = path / "data" / "tiny.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        payload = b"tiny-data"
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=hashlib.sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, Any] | None = None) -> None:
        """Record local load inputs and expose a deterministic data mapping.

        Args:
            path: Verified bundle-local artifact directory.
            load_kwargs: Alias-specific options forwarded by ``WyrdState``.
        Side effects:
            Performs no network access and does not read payload bytes; it only
            records arguments and sets the test dataset invariant.
        """
        self.loaded_path = path
        self.loaded_kwargs = dict(load_kwargs or {})
        self.data = {"rows": [{"value": 1}]}
