import os
from collections.abc import Mapping
from types import TracebackType
from typing import Any, Literal

from ..cards import CardRef
from ..client import WyrdClient
from ._telemetry import record

class Run:
    """One invocation, and one Card-scoped view of it.

    Reached through ``WyrdState.run()``; never constructed directly. A run is
    local: opening it performs no network IO and creates no server-side
    resource. Views are immutable — ``for_card`` returns a sibling rather than
    retargeting this one — so concurrent emits cannot observe a moved subject.
    """

    @property
    def run_id(self) -> str:
        """The UUIDv7 invocation identity this run and every view share."""
        ...

    @property
    def alias(self) -> str:
        """The alias this view was opened with.

        ``state.card_ref(alias)`` returns the exact typed ``CardRef``.
        """
        ...

    @property
    def observe(self) -> Observe:
        """The emit surface for this view."""
        ...

    def for_card(self, alias: str) -> Run:
        """Return an immutable sibling view scoped to a registered alias.

        Args:
            alias: a Card alias registered in the state's bundle.

        Returns:
            A view sharing this run's ``run_id``, scoped to ``alias``.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when the bundle does not
                register ``alias``. No network IO occurs.
        """
        ...

    def invoke(
        self, variables: dict[str, str] | None = None, *, route: dict[str, str] | None = None
    ) -> str:
        """Invoke this view's tool-free Agent once on its execution route.

        A ``{"kind": "wyrd_gateway", "deployment": <name>}`` route sends each
        model call to ``<deployment>/<Prompt model>`` through the gateway as
        the state's client, carrying this run and the Agent Card as
        correlation. A ``{"kind": "native"}`` route calls the Prompt's
        provider with this host's configuration.

        Args:
            variables: the Agent Prompt's string variables.
            route: replaces the Agent's declared ``execution_route`` whole for
                this invocation only; the Card is unchanged. Omitted, the
                declared route is used.

        Returns:
            The Agent's final text.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a malformed ``route``;
                ``WYRD_SDK_400_CARD_KIND_MISMATCH`` for a view that is not an
                Agent; ``WYRD_AGENT_422_VALIDATION`` for an Agent with tools,
                a non-gateway Prompt model, or a gateway route to a Vertex
                Prompt, all before any IO; otherwise the gateway's own
                refusal, such as ``WYRD_PERMISSION_403_DENIED_RBAC``.
        """
        ...

    def __enter__(self) -> Run:
        """Enter this view's ambient OpenTelemetry span correlation.

        Best-effort and execution-local: attaches this view's exact Card
        UID and
        ``run_id`` as ``wyrd.card_uid`` / ``wyrd.run_id`` to the active
        recording span and to every span started inside the block on a
        provider holding the Wyrd processor (the global provider is installed
        automatically; see ``wyrd.otel.install_run_correlation``). Never raises
        for missing or failing OpenTelemetry, never starts a span, flushes, or
        calls the server. Explicit ``observe`` calls do not need ``with``.
        """
        ...

    def __exit__(
        self,
        exc_type: type[BaseException] | None = None,
        exc_value: BaseException | None = None,
        traceback: TracebackType | None = None,
    ) -> Literal[False]:
        """Restore the correlation that was ambient before the matching entry.

        Always returns ``False`` so an exception from the block propagates.
        Exiting is not a flush, shutdown, or durability acknowledgement.
        """
        ...

class File:
    """A local file to verify, read only when ``verify`` or ``submit`` uses it.

    Constructing a ``File`` performs no IO and registers nothing. During the
    call the client hashes it with SHA-256, asks the server which digests it
    lacks, and uploads only those straight to tenant storage. An uploaded file
    is evidence pinned by its digest, never an Artifact Card.
    """

    def __init__(
        self,
        source: str | os.PathLike[str] | bytes,
        *,
        format: str | None = None,
        filename: str | None = None,
    ) -> None:
        """Name a file at a path, or in-memory bytes.

        Args:
            source: a path, opened only during the call, or the file's bytes.
            format: the canonical format, such as ``"pdf"`` or ``"pptx"``.
                Omitted, it is inferred from the filename extension, else the
                slot's single accepted format.
            filename: a display name; defaults to the path's file name.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a ``source`` of
                another type or an unknown ``format``.
        """
        ...

class ArtifactRef:
    """A registered Artifact's file, verified with its normal authorization."""

    def __init__(self, artifact: CardRef, path: str | None = None) -> None:
        """Select ``artifact``'s only file, or ``path`` within it.

        Args:
            artifact: the exact Artifact Card reference.
            path: the file inside the Artifact, when it holds several.
        """
        ...

class Recorded:
    """``submit``'s answer for scheduled-window samples.

    The samples were recorded; the Verifier's schedule judges the window
    later, so there is no execution to wait for.
    """

class VerificationResult:
    """The attributable result of one Verifier execution.

    A ``failed`` verdict is an ordinary return value, not an exception.
    """

    @property
    def execution_id(self) -> str:
        """Server-issued execution ID."""
        ...

    @property
    def run_id(self) -> str | None:
        """The caller's application Run, when there is one."""
        ...

    @property
    def verifier(self) -> CardRef:
        """The exact Verifier executed."""
        ...

    @property
    def subject(self) -> CardRef:
        """The exact subject judged."""
        ...

    @property
    def verdict(self) -> Literal["passed", "failed", "inconclusive"]:
        """The ``decide`` rule applied to every applicable requirement."""
        ...

    @property
    def requirements(self) -> list[dict[str, Any]]:
        """Every requirement's judgment as its wire mapping, in plan order."""
        ...

    @property
    def steps(self) -> list[dict[str, Any]]:
        """Every step's result as its wire mapping, in plan order."""
        ...

    @property
    def evidence(self) -> dict[str, dict[str, Any]]:
        """The pinned original evidence by slot, as wire declarations."""
        ...

class VerificationExecution:
    """One queued Verifier execution, addressed by its server-issued ID.

    Returned by ``Observe.submit`` for Run evidence, or rebuilt in any process
    with ``connect``. The queue, retries, and results stay server-owned.
    """

    @staticmethod
    def connect(id: str, client: WyrdClient | None = None) -> VerificationExecution:
        """Rebuild the handle of execution ``id``.

        Args:
            id: the execution ID ``submit`` returned.
            client: the client every status read is sent as. Omitted, the
                ambient chain resolves as ``WyrdClient`` does.

        Returns:
            The connected handle.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an ``id`` that is not
                an execution ID, and the client's configuration or credential
                error.
        """
        ...

    @property
    def id(self) -> str:
        """The execution ID."""
        ...

    def status(self) -> dict[str, Any]:
        """Read the execution's status, terminal error, and visible result.

        Returns:
            The wire mapping with ``execution_id``, ``status``, and, when
            present, ``error`` and ``result``.

        Raises:
            WyrdError: the server's not-found error before the execution is
                visible, and its authorization or transport error.
        """
        ...

    def wait(self, timeout: float) -> VerificationResult:
        """Poll until the execution yields a result, fails, or ``timeout`` passes.

        Polls back off from 100 ms to at most 2 s. A not-yet-visible
        execution and a completion whose result is not visible yet keep
        being polled; a result is never invented from a status.

        Args:
            timeout: seconds to keep polling.

        Returns:
            The execution's result.

        Raises:
            WyrdError: the execution's own error when it settled as errored,
                cancelled, or timed out, including a delayed
                ``WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT``;
                ``WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`` when ``timeout``
                passes first; ``WYRD_SPEC_400_VALIDATION`` for a negative
                ``timeout``; and any other status-read error.
        """
        ...

class Observe:
    """Verification and caller-table records on one scoped run.

    Returning from ``record`` is queue admission, not a durable
    acknowledgement. ``WyrdState.shutdown()`` is the durability barrier.
    """

    def verify(self, verifier: str, /, *value: Any, **inputs: Any) -> VerificationResult:
        """Execute the bound Verifier ``verifier`` on this view's subject now.

        Pass one positional value for a single-slot Verifier, or one keyword
        per input slot. The slot's type decides each value's form: JSON
        values fill ``json`` slots, a 32-hex trace ID fills a ``trace`` slot,
        and a ``File`` or ``ArtifactRef`` fills a ``media`` slot. Files are
        hashed and uploaded during this call. Blocks for the execution.

        Args:
            verifier: the name of a Verifier bound to this view's subject.
            *value: the one value of a single-slot Verifier.
            **inputs: values keyed by input slot name.

        Returns:
            The execution's result.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_VERIFIER`` for an unbound
                Verifier; ``WYRD_SPEC_400_VALIDATION`` for both positional and
                keyword values or a value JSON cannot represent;
                ``WYRD_VERIFICATION_400_INPUT_INVALID`` for an unknown slot, a
                value of the wrong kind, or an unreadable or unaccepted file;
                ``WYRD_VERIFICATION_413_INPUT_TOO_LARGE`` for a file above its
                slot's ``max_bytes``; ``WYRD_VERIFICATION_422_INPUT_UNSUPPORTED``
                for a scheduled-window Verifier; otherwise the server's refusal.
        """
        ...

    def submit(
        self, verifier: str, /, *value: Any, **inputs: Any
    ) -> VerificationExecution | Recorded:
        """Submit evidence for the bound Verifier ``verifier`` without waiting.

        Run evidence is staged for one queued execution and its handle is
        returned; acceptance confirms staging only, and an idempotency
        conflict surfaces later through ``wait``. Scheduled-window samples are
        recorded for the Verifier's schedule and ``Recorded`` is returned.

        Args:
            verifier: the name of a Verifier bound to this view's subject.
            *value: the one value of a single-slot Verifier.
            **inputs: values keyed by input slot name.

        Returns:
            The queued execution, or ``Recorded`` for window samples.

        Raises:
            WyrdError: As ``verify`` for Run evidence; for window samples,
                ``WYRD_SDK_400_BIFROST_NOT_STARTED``,
                ``WYRD_SDK_409_BIFROST_CLOSED``, or
                ``WYRD_CLIENT_429_QUEUE_FULL``.
        """
        ...

    def record(self, table: str, row: Mapping[str, Any] | Any) -> None:
        """Emit one row into a registered ``vala.datasets.<name>`` table.

        The first call for a table blocks to describe it; later calls reuse
        the cached schema and producer. Row values are checked against that
        schema when the queue seals a batch, not here.

        Args:
            table: the registered ``"vala.datasets.<name>"`` table.
            row: a mapping with ``str`` keys, a dataclass instance, or a
                Pydantic model instance, with any JSON values.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a row of another
                shape, a non-``str`` key, or a non-finite float;
                ``WYRD_SDK_400_INVALID_OBSERVATION`` for a table outside
                ``vala.datasets``; ``WYRD_SDK_400_BIFROST_NOT_STARTED``,
                ``WYRD_SDK_409_BIFROST_CLOSED``, or
                ``WYRD_CLIENT_429_QUEUE_FULL``; and the server's error for a
                table that is unknown, unauthorized, or unavailable.
        """
        ...

__all__ = [
    "ArtifactRef",
    "File",
    "Observe",
    "Recorded",
    "Run",
    "VerificationExecution",
    "VerificationResult",
    "record",
]
