# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from collections.abc import Mapping, Sequence
from typing import Any

from ..bifrost import Bifrost, Correlation
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

__all__ = ["Observe", "Run", "record"]
