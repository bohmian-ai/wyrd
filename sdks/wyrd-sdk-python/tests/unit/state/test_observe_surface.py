"""Scoped observation surface reachable from an offline ``WyrdState``.

These tests stay server-free: opening a run performs no network IO, and every
emit is proven by the error it raises before admission. Durable emit behavior
belongs to the gated journey lanes, not here.
"""

from dataclasses import dataclass
from pathlib import Path
from uuid import UUID

import pytest
import wyrd
from opentelemetry.sdk.trace import TracerProvider
from wyrd.eval import MediaRef
from wyrd.observe import Observe, Run
from wyrd.state import WyrdState

from .support import TinyDataInterface, TinyModelInterface, build_complete_bundle


def _state(tmp_path: Path) -> WyrdState:
    """Hydrate the shared complete fixture bundle with its custom interfaces.

    The interfaces are fresh per call so one test's loader telemetry cannot leak
    into another's assertions.
    """
    return WyrdState.from_path(
        build_complete_bundle(tmp_path),
        interfaces={
            "model": TinyModelInterface(),
            "backup": TinyModelInterface(),
            "training_data": TinyDataInterface(),
        },
    )


def _code(error: BaseException) -> str:
    """Return the stable Wyrd error code carried by a raised boundary error."""
    assert isinstance(error, wyrd.WyrdError)
    return error.code


def test_run_targets_the_root_service_with_a_uuidv7_identity(tmp_path: Path) -> None:
    """One run is one invocation anchored on the bundle's root Service Card."""
    run = _state(tmp_path).run()
    assert isinstance(run, Run)
    assert UUID(run.run_id).version == 7
    assert run.card_ref.startswith("default/Service/service@1.0.0")
    assert isinstance(run.observe, Observe)


def test_scoped_views_share_one_invocation_and_keep_their_subjects(tmp_path: Path) -> None:
    """``for_card`` returns immutable siblings under one invocation identity."""
    run = _state(tmp_path).run()
    model = run.for_card("model")
    backup = run.for_card("backup")
    assert model.run_id == run.run_id == backup.run_id
    assert model.card_ref != backup.card_ref
    assert run.card_ref.startswith("default/Service/service@1.0.0")


def test_each_run_is_its_own_invocation(tmp_path: Path) -> None:
    """Two runs from one state are distinct invocations."""
    state = _state(tmp_path)
    assert state.run().run_id != state.run().run_id


def test_unknown_alias_is_refused(tmp_path: Path) -> None:
    """An alias the bundle does not register fails locally."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().for_card("missing")
    assert _code(raised.value) == "WYRD_SDK_404_UNKNOWN_ALIAS"


def test_drift_accepts_a_mapping_and_reaches_the_writer(tmp_path: Path) -> None:
    """A flat mapping converts, then fails only because Bifrost is not started."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({"latency_ms": 12.5, "tier": "gold"})
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_drift_accepts_a_dataclass_instance(tmp_path: Path) -> None:
    """A dataclass instance is reduced to its fields before admission."""

    @dataclass
    class Features:
        """Test-only flat feature payload for the dataclass conversion path."""

        latency_ms: float
        cached: bool

    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift(Features(latency_ms=3.0, cached=True))
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_drift_refuses_a_payload_that_is_not_an_object(tmp_path: Path) -> None:
    """A sequence is not a feature map and is refused at the boundary."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift([1, 2, 3])
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_drift_refuses_a_nested_feature_value(tmp_path: Path) -> None:
    """Feature values are scalars; a nested object reports the offending key."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({"nested": {"inner": 1}})
    assert _code(raised.value) == "WYRD_SDK_400_INVALID_OBSERVATION"


def test_drift_refuses_a_malformed_session_id(tmp_path: Path) -> None:
    """``session_id`` is a UUID and is parsed before anything is enqueued."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({"score": 1.0}, session_id="not-a-uuid")
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_eval_accepts_context_and_media(tmp_path: Path) -> None:
    """Context and media descriptors convert before admission."""
    media = [MediaRef(id="page", kind="document", uri="s3://bucket/page.pdf")]
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"}, media=media)
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_eval_refuses_a_span_without_its_trace(tmp_path: Path) -> None:
    """A span id is only meaningful inside its trace."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"}, span_id="00f067aa0ba902b7")
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_record_refuses_a_table_outside_vala_datasets(tmp_path: Path) -> None:
    """``record`` writes registered caller-owned tables only."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.record("vala.drift.observations", {"value": 1})
    assert _code(raised.value) == "WYRD_SDK_400_INVALID_OBSERVATION"


def test_flush_requires_a_started_writer(tmp_path: Path) -> None:
    """An explicit drain of a state that never started Bifrost is an error."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).flush()
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_shutdown_without_startup_succeeds(tmp_path: Path) -> None:
    """Graceful shutdown is idempotent: a never-started state has no rows."""
    state = _state(tmp_path)
    assert state.shutdown() is None
    assert state.shutdown() is None


@pytest.mark.parametrize("key", [1, 1.5, True, None])
def test_drift_refuses_a_non_string_mapping_key(tmp_path: Path, key: object) -> None:
    """``json.dumps`` would stringify the key, so it is refused before the writer."""
    with pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.drift({key: 1.0})
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_eval_checks_keys_after_reduction(tmp_path: Path) -> None:
    """Eval refuses a non-``str`` mapping key; a dataclass reduces to ``str`` keys."""

    @dataclass
    class Context:
        """Test-only context whose reduced keys are its field names."""

        answer: str

    run = _state(tmp_path).run()
    with pytest.raises(wyrd.WyrdError) as raised:
        run.observe.eval({1: "yes"})
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"
    with pytest.raises(wyrd.WyrdError) as raised:
        run.observe.eval(Context(answer="yes"))
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"


def test_explicit_span_without_trace_is_refused_inside_an_active_span(tmp_path: Path) -> None:
    """An explicit id disables the active-span lookup, so it is never half-merged."""
    tracer = TracerProvider().get_tracer("wyrd.tests.observe")
    with tracer.start_as_current_span("unit"), pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"}, span_id="00f067aa0ba902b7")
    assert _code(raised.value) == "WYRD_SPEC_400_VALIDATION"


def test_active_span_reaches_the_writer(tmp_path: Path) -> None:
    """A valid active span is read without failing the emit before admission."""
    tracer = TracerProvider().get_tracer("wyrd.tests.observe")
    with tracer.start_as_current_span("unit"), pytest.raises(wyrd.WyrdError) as raised:
        _state(tmp_path).run().observe.eval({"answer": "yes"})
    assert _code(raised.value) == "WYRD_SDK_400_BIFROST_NOT_STARTED"
