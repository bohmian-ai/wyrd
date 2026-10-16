"""A scheduled drift check over submitted samples fails and its Operator alerts the on-call hook.

The story follows the full scheduled lifecycle: register the baseline and the
``latency-watch`` Service, let the real fitter fit the baseline,
``observe.submit`` the samples, make the schedule due, read the retained
result back, and receive the ``alert-on-call`` Operator's delivery exactly
once on the origin, and with the bearer token, of the ``on-call-hooks``
connection. A passing or empty window alerts no one.
"""

import json
import time
from collections.abc import Iterable

import pytest
from wyrd import WyrdError, verify
from wyrd.cards import Cards

from .support import FIXTURES, ON_CALL_TOKEN, Watch, scheduled_result

pytestmark = pytest.mark.integration

WAIT_SECONDS = 90.0
"""How long the story waits for the result and for the alert."""

QUIET_SECONDS = 5.0
"""How long the story listens for an alert that must not arrive."""

HOOK = "/hooks/latency-shift"
"""The path the Operator posts to on the connection's origin."""


def submit(watch: Watch, latencies: Iterable[int]) -> None:
    """Submit every latency as the Service, each recorded for the schedule, and publish them."""
    observe = watch.state.run().observe
    for latency in latencies:
        assert isinstance(observe.submit("latency-shift", {"latency": latency}), verify.Recorded)
    watch.state.flush()
    watch.server.flush_bifrost()


def assert_quiet(watch: Watch) -> None:
    """Assert the hook receives nothing for ``QUIET_SECONDS``."""
    time.sleep(QUIET_SECONDS)
    assert watch.hooks.to(HOOK) == []


def test_failed_schedule_alerts_its_operator_on_the_connection_origin(
    latency_watch: Watch,
) -> None:
    submit(latency_watch, [99] * 100)
    assert latency_watch.hooks.to(HOOK) == [], "submitting alone runs no check"

    result = scheduled_result(latency_watch.server, latency_watch.binding_id, WAIT_SECONDS)

    assert result.verdict == "failed"
    assert result.subject == latency_watch.root
    assert result.verifier.name == "latency-shift"
    assert result.run_id is None, "a scheduled execution has no Run"
    [requirement] = result.requirements
    assert (requirement["id"], requirement["outcome"]) == ("latency-unshifted", "failed")
    window = requirement["provenance"]["window"]
    assert requirement["provenance"]["kind"] == "drift"
    assert window["start"] < window["end"], window
    assert requirement["check"]["signal"]["baseline_ref"]["name"] == "latency-baseline"
    assert requirement["detail"]["drift"]["features"]["latency"]["score"] > 0.25

    alert = latency_watch.hooks.wait_for(HOOK, WAIT_SECONDS)
    assert alert.headers["authorization"] == f"Bearer {ON_CALL_TOKEN}"
    assert result.execution_id in json.dumps(alert.body), alert.body
    time.sleep(QUIET_SECONDS)
    assert len(latency_watch.hooks.to(HOOK)) == 1, "the alert is delivered once"


def test_passing_window_alerts_no_one(latency_watch: Watch) -> None:
    submit(latency_watch, ((row * 37) % 100 for row in range(100)))

    result = scheduled_result(latency_watch.server, latency_watch.binding_id, WAIT_SECONDS)

    assert result.verdict == "passed"
    assert_quiet(latency_watch)


def test_empty_window_is_inconclusive_and_alerts_no_one(latency_watch: Watch) -> None:
    result = scheduled_result(latency_watch.server, latency_watch.binding_id, WAIT_SECONDS)

    assert result.verdict == "inconclusive"
    assert_quiet(latency_watch)


def test_path_only_operator_without_connection_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as raised:
        cards.register_from_path(FIXTURES / "invalid/operator-path-without-connection.yaml")
    assert raised.value.code == "WYRD_SPEC_400_INVALID_OPERATOR"
