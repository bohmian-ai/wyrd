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
from ._hydrated import HydratedCard

class CardEnvelope:
    """Complete read-only projection of one declared Card.

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

class WyrdState:
    """Runtime view of one complete Service Card graph.

    ``WyrdState.declare`` declares an authored Card file and returns its
    state; ``WyrdState.from_path`` loads a bundle produced by ``wyrd pull``
    with local files only. Either eagerly creates Python
    Agent/Prompt/Model/Data holders and shares one holder across every alias
    for the same exact Card.

    Example:
        >>> state = WyrdState.declare("support-reply.yaml")
        >>> state.service.kind
        CardKind.Service
        >>> offline = WyrdState.from_path("./service-bundle")
        >>> offline.model("primary_model")
        <ModelCard ...>
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
                ``Cards.pull``.
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

    @staticmethod
    def declare(
        path: str | Path,
        client: WyrdClient | None = None,
        *,
        bundle_dir: str | Path | None = None,
        interfaces: Mapping[str, object] | None = None,
        load_kwargs: Mapping[str, ModelLoadArgs | DataLoadArgs | Mapping[str, object]]
        | None = None,
        trusted_artifact_hashes: Mapping[str, str] | None = None,
    ) -> WyrdState:
        """Declare a Card file and return its eagerly hydrated state.

        The file and the files its ``./path`` references name are declared
        under the registry's usual version rules, so an unchanged rerun
        declares nothing new. The declared graph is pulled, validated, and
        hydrated as ``from_path`` hydrates a bundle. Every call runs as one
        client. Without ``bundle_dir`` the graph is held in memory and no
        files are kept, so Model and Data payloads that must load need
        ``bundle_dir``.

        Args:
            path: The root Service Card file.
            client: the ``WyrdClient`` every call runs as. Omitted, the
                ambient client resolves once.
            bundle_dir: A directory to publish the complete bundle to. It
                persists, loads offline with ``from_path``, and replaces a
                bundle already there only once the new one is complete.
            interfaces: As for ``from_path``.
            load_kwargs: As for ``from_path``.
            trusted_artifact_hashes: As for ``from_path``.

        Returns:
            A fully hydrated runtime state.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` for an unreadable
                or invalid Card file, the registry's code when it refuses the
                declaration (such as ``WYRD_REGISTRY_409_SPEC_DRIFT`` or
                ``WYRD_PERMISSION_403_DENIED_RBAC``), or a ``from_path`` code.
                A refusal after the registry accepted the declaration leaves
                it declared.
        """
        ...

    def start_bifrost(
        self,
        table: TableConfig | None = None,
        client_byte_limit_bytes: int | None = None,
    ) -> None:
        """Connect this state's one Bifrost writer and describe the fixed table.

        The writer acts as the state's client: the one given to ``from_path``,
        else the ambient client. Startup describes ``vala.drift.observations``
        before succeeding, so a run can never enqueue against a missing,
        unauthorized, or incompatible system table.

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

    def start_telemetry(self) -> None:
        """Install the global tracer provider exporting spans as this state's client.

        Spans export through the stock OTLP/HTTP exporter, which asks the
        client for a fresh access token on every export; spans started inside
        ``with state.run(...)`` carry that Run and Card. A second call is
        idempotent, and ``shutdown`` flushes only the provider installed here.
        Needs the ``otel`` extra.

        Raises:
            WyrdError: ``WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`` when the
                application already installed its own tracer provider; add
                ``wyrd.otel.span_exporter`` to it instead.
            ImportError: naming the ``otel`` extra when it is not installed.
        """
        ...

    def configure(
        self,
        *,
        service_name: str | None = None,
        service_version: str | None = None,
        deployment_environment: str | None = None,
    ) -> None:
        """Fix this state's Service release and deployment environment before first use.

        Local, with no network IO. Omitted name and version default to the
        loaded Service; an omitted environment defaults to a non-empty
        ``WYRD_DEPLOYMENT_ENVIRONMENT``, else stays unknown. Without a call,
        the first Run, client, Bifrost, telemetry, or shutdown use applies the
        same defaults. Another loaded state keeps its own configuration.

        Args:
            service_name: the loaded Service's name; must match it.
            service_version: the loaded Service's exact version; must match it.
            deployment_environment: the workload's deployment environment.

        Raises:
            WyrdError: ``WYRD_SDK_409_STATE_CONFIG_FROZEN`` once configuration
                is fixed, even after shutdown; ``WYRD_CLIENT_400_CONFIG_INVALID``
                for an empty field or a mismatched name or version.
        """
        ...

    @property
    def deployment_environment(self) -> str | None:
        """The configured deployment environment, or ``None`` when unknown; reading it fixes defaults."""
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
                declared in this bundle. No network IO occurs.
        """
        ...

    def flush(self) -> None:
        """Drain every producer of this state's writer without closing it.

        The explicit durability barrier for a test or a finite job. A tracer
        provider ``start_telemetry`` installed is force-flushed first; the
        writer drains even when that flush fails.

        Raises:
            WyrdError: ``WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT`` or
                ``WYRD_SDK_503_TELEMETRY_FLUSH_FAILED`` when the telemetry
                flush timed out or failed (``details["native_code"]`` names any
                writer failure too); otherwise
                ``WYRD_SDK_400_BIFROST_NOT_STARTED`` before startup,
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
        create a new ``WyrdState`` to start again. A tracer provider
        ``start_telemetry`` installed is force-flushed first, and the writer
        drains even when that flush fails. An application-owned provider is
        never flushed here; shut it down yourself.

        Raises:
            WyrdError: ``WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT`` or
                ``WYRD_SDK_503_TELEMETRY_FLUSH_FAILED`` when the telemetry
                flush timed out or failed (``details["native_code"]`` names any
                drain failure too); otherwise the first producer or sink
                failure from the drain.
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

    def get(self, alias: str) -> HydratedCard:
        """Return the hydrated Card selected by ``alias`` in its kind's holder.

        Agents, Prompts, Models and Data return the holders their typed
        accessors return; every other kind returns its ``CardEnvelope``. The
        lookup is strict and performs no IO.

        Args:
            alias: A friendly alias from this bundle.

        Returns:
            The hydrated Card; narrow it with ``isinstance``.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when no Card has the alias.
        """
        ...

    def __getitem__(self, alias: str) -> HydratedCard:
        """Return the hydrated Card selected by ``alias``, as ``get`` does.

        Args:
            alias: A friendly alias from this bundle.

        Returns:
            The hydrated Card; narrow it with ``isinstance``.

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
        """Return the Agent holder, navigable to its Prompt Card by ``prompt()``.

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

        Its typed inputs, steps, and requirements live under ``spec``.

        Args:
            alias: A friendly alias of a Verifier Card.

        Returns:
            The Verifier Card envelope.

        Raises:
            WyrdError: For an unknown alias or non-Verifier Card.
        """
        ...

__all__ = ["CardEnvelope", "HydratedCard", "WyrdState"]
