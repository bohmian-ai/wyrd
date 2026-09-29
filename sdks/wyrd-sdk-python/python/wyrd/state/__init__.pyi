# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####

from collections.abc import Mapping
from pathlib import Path

from ..bifrost import TableConfig
from ..cards import AgentCard, CardKind, CardRef, DataLoadArgs, ModelLoadArgs
from ..data import DataCard
from ..model import ModelCard
from ..observe import Run
from ..prompt import PromptCard

#### end of imports ####

class CardEnvelope:
    """Complete read-only projection of one registered Card.

    The envelope is retained in native typed form by ``WyrdState``. Mapping
    properties are converted to ordinary JSON-compatible Python values when
    accessed. Instances cannot be constructed directly and property access
    performs no network IO.
    """

    @property
    def card_ref(self) -> CardRef:
        """Return the exact versioned and UID-pinned Card identity."""
        ...

    @property
    def aliases(self) -> tuple[str, ...]:
        """Return all friendly aliases for this exact Card in stable order."""
        ...

    @property
    def kind(self) -> CardKind:
        """Return the Card kind from the retained envelope."""
        ...

    @property
    def metadata(self) -> dict[str, object]:
        """Return a JSON-compatible copy of Card metadata."""
        ...

    @property
    def spec(self) -> dict[str, object]:
        """Return a JSON-compatible copy of the kind-specific Card spec."""
        ...

    @property
    def relationships(self) -> dict[str, object]:
        """Return a JSON-compatible copy of server-derived relationships."""
        ...

    @property
    def status(self) -> dict[str, object] | None:
        """Return server-managed status, or ``None`` when no status exists."""
        ...

    def model_dump(self) -> dict[str, object]:
        """Return the complete Card envelope as a JSON-compatible mapping."""
        ...

    def model_dump_json(self) -> str:
        """Serialize the complete Card envelope to canonical public JSON.

        Raises:
            WyrdError: If the retained native envelope cannot be serialized.
        """
        ...

class HydratedArtifact:
    """Read-only path and integrity metadata for one verified local artifact.

    Instances come from ``WyrdState.artifacts`` and do not contain payload
    bytes. Opening ``local_path`` is always an explicit caller action.
    """

    @property
    def relative_path(self) -> str:
        """Return the manifest-relative artifact path."""
        ...

    @property
    def local_path(self) -> Path:
        """Return the absolute path confined beneath the hydrated bundle."""
        ...

    @property
    def sha256(self) -> str:
        """Return the SHA-256 digest verified while loading the bundle."""
        ...

    @property
    def size_bytes(self) -> int:
        """Return the verified payload length in bytes."""
        ...

    @property
    def content_type(self) -> str | None:
        """Return the declared media type, if present."""
        ...

class WyrdState:
    """Offline runtime view of one complete Service Card graph.

    ``WyrdState`` validates a bundle produced by ``wyrd get``, eagerly creates
    Python Agent/Prompt/Model/Data holders, and shares one holder across every
    alias for the same exact Card. Construction uses local files only.

    Example:
        >>> state = WyrdState.from_path("./service-bundle")
        >>> state.service.kind
        CardKind.Service
        >>> state.model("primary_model")
        <ModelCard ...>
        >>> state.artifacts("primary_model")[0].local_path
        PosixPath(...)
    """

    @staticmethod
    def from_path(
        path: str | Path,
        *,
        interfaces: Mapping[str, object] | None = ...,
        load_kwargs: Mapping[str, ModelLoadArgs | DataLoadArgs | Mapping[str, object]] | None = ...,
        trusted_artifact_hashes: Mapping[str, str] | None = ...,
    ) -> WyrdState:
        """Load, validate, and eagerly hydrate a complete local bundle.

        Args:
            path: Directory produced by complete ``wyrd get`` hydration.
            interfaces: Custom Model/Data interface classes or instances keyed
                by friendly alias.
            load_kwargs: Model/Data loader arguments keyed by friendly alias.
            trusted_artifact_hashes: Externally verified canonical artifact
                manifest hashes keyed by Model/Data alias. Joblib-backed built-in
                Models require an exact hash for their persisted CardRef before
                local deserialization can run.

        Returns:
            A fully hydrated offline runtime state.

        Raises:
            WyrdError: With a stable ``WYRD_SDK_*`` code for invalid bundles,
                aliases, kinds, conflicting configuration, or holder hydration.

        This method performs no network access.
        """
        ...

    def start_bifrost(
        self,
        table: TableConfig | None = None,
        server_url: str | None = None,
        credential: str | None = None,
        grpc_url: str | None = None,
    ) -> None:
        """Connect this state's one Bifrost writer and describe the fixed tables.

        The four arguments are ``Bifrost(...)``'s and pass straight through,
        including its environment and default resolution. Startup describes
        ``vala.drift.observations`` and ``vala.eval.observations`` before
        succeeding, so a run can never enqueue against a missing, unauthorized,
        or incompatible system table. ``table`` keeps its existing Bifrost
        meaning and does not choose a run's destination.

        Raises:
            WyrdError: ``WYRD_SDK_409_BIFROST_ALREADY_STARTED`` when this state
                already started Bifrost, ``WYRD_SDK_409_BIFROST_CLOSED`` after a
                successful shutdown, or the catalog code for a missing
                credential, an undialable ingest channel, or a fixed table that
                is absent, unauthorized, or incompatible.
        """
        ...

    def run(self) -> Run:
        """Open one invocation, targeting the root Service Card.

        Local only: no network IO, no server-side Run resource, and no Verifier
        execution.
        """
        ...

    def flush(self) -> None:
        """Drain every producer of this state's writer without closing it.

        The explicit durability barrier for a test or a finite job.

        Raises:
            WyrdError: ``WYRD_SDK_400_BIFROST_NOT_STARTED`` before startup,
                ``WYRD_SDK_409_BIFROST_CLOSED`` after shutdown, or the first
                producer or sink failure.
        """
        ...

    def shutdown(self) -> None:
        """Drain every producer of this state's writer and close it to writes.

        Call this once at graceful application shutdown, not after each
        observation: queue admission is not a durable acknowledgement, so an
        abrupt exit before this returns can lose pending rows. After an
        ambiguous failure, retry ``shutdown()`` on the same state rather than
        replacing the writer. A successfully shut-down state stays closed;
        create a new ``WyrdState`` to start again.

        Raises:
            WyrdError: The first producer or sink failure from the drain.
        """
        ...

    @property
    def service(self) -> CardEnvelope:
        """Return the exact root Service envelope, independent of aliases."""
        ...

    @property
    def aliases(self) -> tuple[str, ...]:
        """Return every friendly alias in stable sorted order."""
        ...

    def card(self, alias: str) -> CardEnvelope:
        """Return the complete Card envelope selected by ``alias``.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when no Card has the alias.
        """
        ...

    def card_ref(self, alias: str) -> CardRef:
        """Return the exact CardRef selected by ``alias``.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when no Card has the alias.
        """
        ...

    def model(self, alias: str) -> ModelCard:
        """Return the eagerly loaded Model holder selected by ``alias``.

        Raises:
            WyrdError: For an unknown alias or non-Model Card.
        """
        ...

    def data(self, alias: str) -> DataCard:
        """Return the eagerly loaded Data holder selected by ``alias``.

        Raises:
            WyrdError: For an unknown alias or non-Data Card.
        """
        ...

    def agent(self, alias: str) -> AgentCard:
        """Return the Agent holder with its inline or resolved typed prompt.

        Raises:
            WyrdError: For an unknown alias or non-Agent Card.
        """
        ...

    def prompt(self, alias: str) -> PromptCard:
        """Return the hydrated Prompt holder selected by ``alias``.

        Raises:
            WyrdError: For an unknown alias or non-Prompt Card.
        """
        ...

    def verifier(self, alias: str) -> CardEnvelope:
        """Return the kind-checked Verifier envelope selected by ``alias``.

        The typed Drift or Eval body lives under ``spec["implementation"]``.

        Raises:
            WyrdError: For an unknown alias or non-Verifier Card.
        """
        ...

    def workflow(self, alias: str) -> CardEnvelope:
        """Return the kind-checked Workflow envelope selected by ``alias``.

        Raises:
            WyrdError: For an unknown alias or non-Workflow Card.
        """
        ...

    def artifacts(self, alias: str) -> tuple[HydratedArtifact, ...]:
        """Return verified artifact descriptors without reading payload bytes.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when no Card has the alias.
        """
        ...

__all__ = ["CardEnvelope", "HydratedArtifact", "WyrdState"]
