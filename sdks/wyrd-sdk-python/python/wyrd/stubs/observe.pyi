#### begin imports ####
from collections.abc import Mapping, Sequence
from types import TracebackType
from typing import Any, Literal

from .bifrost import Bifrost, Correlation
from .cards import CardRef
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
    def kind(
        self,
    ) -> Literal[
        "drift_psi",
        "drift_spc",
        "drift_custom",
        "eval_assertion",
        "eval_llm_judge",
        "eval_other",
        "unknown",
    ]:
        """The Verifier classification: method family and profile."""
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

        Args:
            verifier: the name of a Verifier bound to this view's subject.
            input: one context mapping, dataclass, or Pydantic model for an
                Eval Verifier; a list of flat feature rows for a Drift
                Verifier.
            media: ``MediaRef`` items for an Eval judge Prompt's media slots.

        Returns:
            The Verifier's judgment.

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
    schema: Mapping[str, Any] | type[Any],
    row: Mapping[str, Any] | Any,
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
        schema: the table's JSON Schema mapping, or a Pydantic model class
            whose ``model_json_schema()`` is used.
        row: the row as a mapping or a Pydantic model instance.
        correlation: optional ``card_ref`` (``space/Kind/name@version``) and
            ``run_id`` stamped on the row. Omitted, the row is uncorrelated.

    Returns:
        ``None``; a refused row is counted on ``bifrost.dropped``.

    Raises:
        WyrdError: ``WYRD_VALA_400_SCHEMA_PARSE`` for a schema with no
            mappable columns, as ``TableConfig.from_json_schema`` raises;
            ``WYRD_SPEC_400_VALIDATION`` for an invalid ``card_ref``.
    """
    ...

__all__ = ["Judgment", "Observe", "Run", "record"]
