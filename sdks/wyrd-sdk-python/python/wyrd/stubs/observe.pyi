#### begin imports ####
from collections.abc import Mapping, Sequence
from types import TracebackType
from typing import Any, Literal

from .bifrost import Bifrost, Correlation
from .eval import MediaRef

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
    def card_ref(self) -> str:
        """The exact ``space/Kind/name@version`` this view observes."""
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

        Best-effort and execution-local: attaches this view's ``card_ref`` and
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

class Observe:
    """The three emits available on one scoped run.

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

        Args:
            features: a non-empty flat mapping with ``str`` feature-name keys,
                a dataclass instance, or a Pydantic model instance whose
                values are strings, integers no larger in magnitude than
                ``2**53``, finite floats, or booleans. It is converted here,
                before admission.
            session_id: a UUID naming the observed interaction's session.
                Omitted, the rows carry no session.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an input of another
                shape, a non-``str`` key, a non-finite float, or a
                ``session_id`` that is not a UUID;
                ``WYRD_SDK_400_INVALID_OBSERVATION`` for an empty input, an
                invalid feature name, or a ``None``, nested, or out-of-range
                value; ``WYRD_SDK_400_BIFROST_NOT_STARTED``,
                ``WYRD_SDK_409_BIFROST_CLOSED``, or
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

        Args:
            context: the observed interaction the Eval judges score, as a
                mapping with ``str`` keys, a dataclass instance, or a Pydantic
                model instance; nested JSON values are allowed.
            session_id: as for ``drift()``.
            media: a ``list`` of ``wyrd.eval.MediaRef`` descriptors for the
                judge Prompt's ``${media:id}`` slots. Omitted, none.
            trace_id: the 32-hex-character OpenTelemetry trace id.
            span_id: the 16-hex-character span id within ``trace_id``. When
                both ids are omitted, Python's active OpenTelemetry span
                supplies them if there is one.

        Raises:
            WyrdError: As ``drift()``, plus ``WYRD_SPEC_400_VALIDATION`` for a
                ``media`` value that is not a list of valid descriptors, a
                malformed or all-zero trace or span id, or a ``span_id``
                without ``trace_id``.
        """
        ...

    def record(self, table: str, row: Mapping[str, Any] | Any) -> None:
        """Emit one row into a registered ``vala.datasets.<name>`` table.

        The first call for a table blocks to describe it; later calls reuse
        the cached schema and producer. Row values are checked against that
        schema when the queue seals a batch, not here.

        Args:
            table: the registered ``"vala.datasets.<name>"`` table.
            row: the row, in any shape ``drift()`` accepts for ``features``,
                with any JSON values.

        Raises:
            WyrdError: As ``drift()`` for the row's shape, lifecycle, and
                queue, plus ``WYRD_SDK_400_INVALID_OBSERVATION`` for a table outside
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
    the client's active binding, so an instrumented process writes its signals
    alongside whatever the application writes. If the producer refuses the row,
    for example because its queue is full, the row is dropped and counted on
    ``bifrost.dropped`` instead of raising.

    Args:
        bifrost: the ``Bifrost`` client whose producers carry the row.
        table: the destination table name.
        schema: the table's JSON Schema text, mapped to the Arrow schema of
            the row.
        row: the row as JSON object text.
        correlation: optional ``card_ref`` (``space/Kind/name@version``) and
            ``run_id`` stamped on the row. Omitted, the row is uncorrelated.

    Raises:
        WyrdError: ``WYRD_SPEC_400_VALIDATION`` for malformed or unsupported
            ``schema`` text or an invalid ``card_ref``.
    """
    ...

__all__ = ["Observe", "Run", "record"]
