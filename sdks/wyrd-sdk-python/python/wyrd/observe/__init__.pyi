from collections.abc import Mapping
from types import TracebackType
from typing import Any, Literal

from typing_extensions import TypeVar

from ..verify import DataModel, Execution, Recorded, VerificationResult
from ._telemetry import record

_Model = TypeVar("_Model", bound=DataModel)

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

class Observe:
    """Verification and caller-table records on one scoped run.

    Returning from ``record`` is queue admission, not a durable
    acknowledgement. ``WyrdState.shutdown()`` is the durability barrier.
    """

    def verify(
        self, verifier: str, /, *value: Any, group_id: str | None = None, **inputs: Any
    ) -> VerificationResult:
        """Execute the bound Verifier ``verifier`` on this view's subject now.

        Pass one positional value for a single-slot Verifier, or one keyword
        per input slot. The slot's type decides each value's form: JSON
        values fill ``json`` slots, a 32-hex trace ID fills a ``trace`` slot,
        and a ``wyrd.verify.File`` or ``ArtifactRef`` fills a ``media`` slot. Files are
        hashed and uploaded during this call. Blocks for the execution.

        Args:
            verifier: the name of a Verifier bound to this view's subject.
            *value: the one value of a single-slot Verifier.
            group_id: the caller's group, such as a conversation or order ID,
                that grouped Verifiers judge together; never an input slot.
            **inputs: values keyed by input slot name.

        Returns:
            The execution's result.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_VERIFIER`` for an unbound
                Verifier; ``WYRD_SPEC_400_VALIDATION`` for both positional and
                keyword values, a value JSON cannot represent, or an invalid
                ``group_id``;
                ``WYRD_VERIFICATION_400_INPUT_INVALID`` for an unknown slot, a
                value of the wrong kind, or an unreadable or unaccepted file;
                ``WYRD_VERIFICATION_413_INPUT_TOO_LARGE`` for a file above its
                slot's ``max_bytes``; ``WYRD_VERIFICATION_422_INPUT_UNSUPPORTED``
                for a scheduled-window Verifier; otherwise the server's refusal.
        """
        ...

    def verify_typed(
        self,
        verifier: str,
        data_type: type[_Model],
        /,
        *value: Any,
        group_id: str | None = None,
        **inputs: Any,
    ) -> VerificationResult[_Model]:
        """``verify``, with every requirement's ``data`` validated as ``data_type``.

        The execution is the ordinary one. Every present ``data`` object,
        failed requirements included, passes through
        ``data_type.model_validate`` before the result returns; absent data
        stays absent. A refusal changes no stored result and reruns nothing.
        ``verifier`` and ``data_type`` are positional-only, so an input slot
        named ``data_type`` still passes as a keyword.

        Args:
            verifier: the name of a Verifier bound to this view's subject.
            data_type: a model exposing ``model_validate``, such as a
                Pydantic model class or a ``RootModel`` union.
            *value: the one value of a single-slot Verifier.
            group_id: the caller's group, as for ``verify``.
            **inputs: values keyed by input slot name.

        Returns:
            The execution's result, its ``data`` typed.

        Raises:
            WyrdError: As ``verify``;
                ``WYRD_SDK_422_VERIFICATION_DATA_INVALID`` naming the first
                requirement and field path ``data_type`` refuses, never the
                data; ``WYRD_SPEC_400_VALIDATION`` when ``data_type`` has no
                ``model_validate``.
        """
        ...

    def submit(
        self, verifier: str, /, *value: Any, group_id: str | None = None, **inputs: Any
    ) -> Execution | Recorded:
        """Submit evidence for the bound Verifier ``verifier`` without waiting.

        Run evidence is staged for one queued ``wyrd.verify.Execution`` and
        its handle is returned; acceptance confirms staging only, and an idempotency
        conflict surfaces later through ``wait``. Scheduled-window samples are
        recorded for the Verifier's schedule and ``Recorded`` is returned.

        Args:
            verifier: the name of a Verifier bound to this view's subject.
            *value: the one value of a single-slot Verifier.
            group_id: the caller's group, as for ``verify``.
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
    "Observe",
    "Run",
    "record",
]
