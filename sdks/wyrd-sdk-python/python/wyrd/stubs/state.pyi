#### begin imports ####

from collections.abc import Mapping
from pathlib import Path
from typing import Any

from ..bifrost import TableConfig
from ..cards import AgentCard, CardKind, CardRef, DataLoadArgs, ModelLoadArgs
from ..client import WyrdClient
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
    def metadata(self) -> dict[str, Any]:
        """Return a JSON-compatible copy of Card metadata."""
        ...

    @property
    def spec(self) -> dict[str, Any]:
        """Return a JSON-compatible copy of the kind-specific Card spec."""
        ...

    @property
    def relationships(self) -> dict[str, Any]:
        """Return a JSON-compatible copy of server-derived relationships."""
        ...

    @property
    def status(self) -> dict[str, Any] | None:
        """Return server-managed status, or ``None`` when no status exists."""
        ...

    def model_dump(self) -> dict[str, Any]:
        """Return the complete Card envelope as a JSON-compatible mapping.

        Returns:
            The envelope as plain Python values.

        Raises:
            WyrdError: If the retained native envelope cannot be converted.
        """
        ...

    def model_dump_json(self) -> str:
        """Serialize the complete Card envelope to canonical public JSON.

        Returns:
            The envelope as a JSON string.

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
        client: WyrdClient | None = None,
        *,
        interfaces: Mapping[str, object] | None = None,
        load_kwargs: Mapping[str, ModelLoadArgs | DataLoadArgs | Mapping[str, object]]
        | None = None,
        trusted_artifact_hashes: Mapping[str, str] | None = None,
    ) -> WyrdState:
        """Load, validate, and eagerly hydrate a complete local bundle.

        Hydration performs no network access; ``client`` is held for the
        state's later server calls, such as ``start_bifrost``.

        Args:
            path: Directory produced by complete hydration, such as
                ``Cards.hydrate``.
            client: the ``WyrdClient`` this state acts as. Omitted, the
                ambient client resolves when the state first needs one.
            interfaces: Custom Model/Data interface classes or instances keyed
                by friendly alias.
            load_kwargs: Model/Data loader arguments keyed by friendly alias.
            trusted_artifact_hashes: Externally verified canonical artifact
                manifest hashes keyed by Model/Data alias. Built-in sklearn,
                XGBoost, LightGBM, and CatBoost Models require an exact hash for
                their persisted CardRef before local deserialization can run;
                without one, construction fails.

        Returns:
            A fully hydrated offline runtime state.

        Raises:
            WyrdError: With a stable ``WYRD_SDK_*`` code for invalid bundles,
                aliases, kinds, conflicting configuration, or holder hydration.

        """
        ...

    def start_bifrost(
        self,
        table: TableConfig | None = None,
        client_byte_limit_bytes: int | None = None,
    ) -> None:
        """Connect this state's one Bifrost writer and describe the fixed tables.

        The writer acts as the state's client: the one given to ``from_path``,
        else the ambient client. Startup describes
        ``vala.drift.observations`` and ``vala.eval.observations`` before
        succeeding, so a run can never enqueue against a missing, unauthorized,
        or incompatible system table.

        Args:
            table: the Bifrost write binding, as for ``Bifrost()``. It does
                not choose a run's destination.
            client_byte_limit_bytes: the handle-wide ingestion byte budget.
                256 MiB if omitted.

        Raises:
            WyrdError: ``WYRD_SDK_409_BIFROST_ALREADY_STARTED`` when this state
                already started Bifrost, ``WYRD_SDK_409_BIFROST_CLOSED`` after a
                successful shutdown, ``WYRD_CLIENT_400_CONFIG_INVALID`` for a
                byte budget too small to seal one message, or the catalog code
                for a missing credential, an undialable ingest channel, or a
                fixed table that is absent, unauthorized, or incompatible.
        """
        ...

    def run(self, alias: str | None = None) -> Run:
        """Open one invocation whose first view observes ``alias``.

        Local only: no network IO, no server-side Run resource, and no Verifier
        execution. ``alias`` selects a registered Card before the run mints its
        ``run_id``; omitting it opens the root Service Card's view. Later
        ``for_card`` views share that id. Use ``with state.run("alias")`` to
        correlate OpenTelemetry spans created inside the block.

        Args:
            alias: A friendly alias from this bundle. Omitted, the root Service Card.

        Returns:
            A local `Run` whose first view observes the selected Card.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when ``alias`` is not
                registered in this bundle. No network IO occurs.
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
    def root_ref(self) -> CardRef:
        """Return the exact root `CardRef` of this bundle, independent of aliases."""
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

        Args:
            alias: A friendly alias from this bundle.

        Returns:
            The complete read-only Card envelope.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when no Card has the alias.
        """
        ...

    def card_ref(self, alias: str) -> CardRef:
        """Return the exact CardRef selected by ``alias``.

        Args:
            alias: A friendly alias from this bundle.

        Returns:
            The exact `CardRef` of the selected Card.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when no Card has the alias.
        """
        ...

    def model(self, alias: str) -> ModelCard:
        """Return the eagerly loaded Model holder selected by ``alias``.

        Args:
            alias: A friendly alias of a Model Card.

        Returns:
            The loaded `ModelCard` holder.

        Raises:
            WyrdError: For an unknown alias or non-Model Card.
        """
        ...

    def data(self, alias: str) -> DataCard:
        """Return the eagerly loaded Data holder selected by ``alias``.

        Args:
            alias: A friendly alias of a Data Card.

        Returns:
            The loaded `DataCard` holder.

        Raises:
            WyrdError: For an unknown alias or non-Data Card.
        """
        ...

    def agent(self, alias: str) -> AgentCard:
        """Return the Agent holder with its inline or resolved typed prompt.

        Args:
            alias: A friendly alias of an Agent Card.

        Returns:
            The `AgentCard` holder.

        Raises:
            WyrdError: For an unknown alias or non-Agent Card.
        """
        ...

    def prompt(self, alias: str) -> PromptCard:
        """Return the hydrated Prompt holder selected by ``alias``.

        Args:
            alias: A friendly alias of a Prompt Card.

        Returns:
            The hydrated `PromptCard` holder.

        Raises:
            WyrdError: For an unknown alias or non-Prompt Card.
        """
        ...

    def verifier(self, alias: str) -> CardEnvelope:
        """Return the kind-checked Verifier envelope selected by ``alias``.

        The typed Drift or Eval body lives under ``spec["implementation"]``.

        Args:
            alias: A friendly alias of a Verifier Card.

        Returns:
            The Verifier Card envelope.

        Raises:
            WyrdError: For an unknown alias or non-Verifier Card.
        """
        ...

    def workflow(self, alias: str) -> CardEnvelope:
        """Return the kind-checked Workflow envelope selected by ``alias``.

        Args:
            alias: A friendly alias of a Workflow Card.

        Returns:
            The Workflow Card envelope.

        Raises:
            WyrdError: For an unknown alias or non-Workflow Card.
        """
        ...

    def artifacts(self, alias: str) -> tuple[HydratedArtifact, ...]:
        """Return verified artifact descriptors without reading payload bytes.

        Args:
            alias: A friendly alias from this bundle.

        Returns:
            One verified descriptor per artifact, without payload bytes.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when no Card has the alias.
        """
        ...

__all__ = ["CardEnvelope", "HydratedArtifact", "WyrdState"]
