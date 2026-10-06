# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from collections.abc import Mapping, Sequence
from types import TracebackType
from typing import Any, Literal

from ..bifrost import Bifrost, Correlation
from ..cards import CardRef
from ..eval import MediaRef

#### end of imports ####

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

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_ALIAS`` when the bundle does not
                register ``alias``. No network IO occurs.
        """
        ...

    def __enter__(self) -> Run:
        """Enter this view's ambient OpenTelemetry span correlation.

        Best-effort and execution-local: attaches this view's exact Card
        reference and
        ``run_id`` as ``wyrd.card_ref`` / ``wyrd.run_id`` to the active
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

class Judgment:
    """One direct verification judgment, returned by :meth:`Observe.verify`.

    A ``failed`` verdict is an ordinary return value, not an exception.
    """

    @property
    def passed(self) -> bool:
        """Whether the expectations held: true only for a ``passed`` verdict."""
        ...

    @property
    def execution_id(self) -> str:
        """Transient identity of this execution, shared with its audit and trace."""
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
    def kind(self) -> Literal["drift", "eval"]:
        """The Verifier classification."""
        ...

    @property
    def verdict(self) -> Literal["passed", "failed", "inconclusive"]:
        """The common verdict."""
        ...

    @property
    def summary(self) -> str:
        """Bounded human-readable summary of the verdict."""
        ...

    @property
    def counts(self) -> dict[str, Any]:
        """Count-only rollup of the judgment as its wire mapping."""
        ...

    @property
    def detail(self) -> dict[str, Any]:
        """The engine report: ``{"drift": {...}}`` or ``{"eval": {...}}``."""
        ...

class Observe:
    """The emits and the direct judgment available on one scoped run.

    Returning from an emit is queue admission, not a durable acknowledgement.
    ``WyrdState.shutdown()`` is the durability barrier.
    """

    def drift(
        self,
        features: Mapping[str, Any] | Any,
        *,
        session_id: str | None = None,
    ) -> None:
        """Emit one Drift observation as one row per feature.

        ``features`` is a flat mapping, dataclass instance, or Pydantic model
        instance of string, integer, float, or boolean values. It is converted
        here, before admission.

        Raises:
            WyrdError: ``WYRD_SDK_400_BIFROST_NOT_STARTED``,
                ``WYRD_SDK_409_BIFROST_CLOSED``,
                ``WYRD_SDK_400_INVALID_OBSERVATION`` for an input that is not a
                flat object of supported scalars, ``WYRD_SPEC_400_VALIDATION``
                for a malformed ``session_id``, or
                ``WYRD_CLIENT_429_QUEUE_FULL``.
        """
        ...

    def eval(
        self,
        context: Mapping[str, Any] | Any,
        *,
        session_id: str | None = None,
        media: Sequence[MediaRef] | None = None,
        trace_id: str | None = None,
        span_id: str | None = None,
    ) -> None:
        """Emit one Eval observation carrying its context and identity.

        ``trace_id`` and ``span_id`` are lower-case hex. When both are omitted
        the active OpenTelemetry span supplies them; a ``span_id`` without its
        ``trace_id`` is refused.

        Raises:
            WyrdError: As :meth:`drift`, plus ``WYRD_SPEC_400_VALIDATION`` for a
                malformed media descriptor, trace identifier, or a ``span_id``
                supplied without ``trace_id``.
        """
        ...

    def verify(
        self,
        verifier: str,
        input: Mapping[str, Any] | Sequence[Mapping[str, Any]] | Any,
        *,
        media: Sequence[MediaRef] | None = None,
    ) -> Judgment:
        """Judge this view's subject with a bound Verifier and return its judgment.

        ``verifier`` names a Verifier bound in ``verified_by`` to this view's
        subject. An Eval Verifier takes one context mapping, dataclass instance,
        or Pydantic model plus optional ``media``; a Drift Verifier takes a
        list of flat feature rows. Blocks for the one server call. Nothing is
        observed, recorded, enqueued, or dispatched, and Bifrost need not be
        started.

        Raises:
            WyrdError: ``WYRD_SDK_404_UNKNOWN_VERIFIER`` for an unbound
                Verifier and ``WYRD_SDK_400_INVALID_OBSERVATION`` for input of
                the wrong shape, both before any network IO, and otherwise the
                server's verification refusal such as
                ``WYRD_VERIFICATION_409_BASELINE_NOT_READY``.
        """
        ...

    def record(self, table: str, row: Mapping[str, Any] | Any) -> None:
        """Emit one row into a registered ``vala.datasets.<name>`` table.

        The first call for a table describes it; later calls reuse the cached
        schema and producer, so only the first blocks on a lookup.

        Raises:
            WyrdError: As :meth:`drift`, plus
                ``WYRD_SDK_400_INVALID_OBSERVATION`` for a table outside
                ``vala.datasets``, and the server's error for a table that is
                unknown, unauthorized, or unavailable.
        """
        ...

def record(
    bifrost: Bifrost,
    table: str,
    schema: str,
    row: str,
    correlation: Correlation | None = None,
) -> None:
    """Record one telemetry observation, fire-and-forget.

    Telemetry names its own ``table`` and ``schema`` per call rather than using
    the client's active binding. ``schema`` is JSON-Schema text; ``correlation``
    optionally carries ``card_ref`` (``space/Kind/name@version``) and ``run_id``.
    Queue-full is swallowed and counted on ``bifrost.dropped``, never raised.
    """
    ...

__all__ = ["Judgment", "Observe", "Run", "record"]
