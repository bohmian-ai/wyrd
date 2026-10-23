"""A scheduled drift check over submitted samples fails and its Operator alerts the on-call hook.

The story follows the full scheduled lifecycle: register the baseline and the
``latency-watch`` Service, let the real fitter fit the baseline,
``observe.submit`` the samples, make the schedule due, read the retained
result back, and receive the ``alert-on-call`` Operator's delivery exactly
once on the origin, and with the bearer token, of the ``on-call-hooks``
connection. A passing or empty window alerts no one. A second Service,
``latency-steady``, control-charts the same latency against its fitted SPC
baseline.
"""

import json
import time
from collections.abc import Iterable
from typing import Any, cast

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
        assert isinstance(observe.submit(watch.verifier, {"latency": latency}), verify.Recorded)
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
    provenance = requirement.get("provenance")
    assert provenance is not None and provenance["kind"] == "drift", provenance
    window = provenance.get("window")
    assert window is not None and window["start"] < window["end"], window
    check = requirement["check"]
    assert check["kind"] == "drift", check
    signal = cast("dict[str, Any]", check["signal"])
    assert signal["baseline_ref"]["name"] == "latency-baseline"
    score = requirement["detail"]["drift"]["features"]["latency"]["score"]
    assert score is not None and score > 0.25

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


def test_ready_spc_baseline_charts_a_scheduled_window(latency_steady: Watch) -> None:
    submit(latency_steady, [99] * 20)

    result = scheduled_result(latency_steady.server, latency_steady.binding_id, WAIT_SECONDS)

    assert result.verdict == "failed"
    assert result.verifier.name == "latency-control"
    [requirement] = result.requirements
    assert (requirement["id"], requirement["outcome"]) == ("latency-in-control", "failed")
    check = requirement["check"]
    assert check["kind"] == "drift", check
    assert check["method"] == "Spc", check
    latency = requirement["detail"]["drift"]["features"]["latency"]
    assert (latency["score"], latency["threshold"]) == (4.0, 0.0), latency
    chart = cast("dict[str, Any]", latency.get("evidence"))["Spc"]
    assert (
        chart["subgroup_size"],
        chart["subgroups"],
        chart["x_bar"]["center"],
        chart["x_bar"]["signals"],
        chart["s"]["signals"],
    ) == (5, 4, 49.5, 4, 0), chart


def test_path_only_operator_without_connection_is_refused(cards: Cards) -> None:
    with pytest.raises(WyrdError) as raised:
        cards.declare_from_path(FIXTURES / "invalid/operator-path-without-connection.yaml")
    assert raised.value.code == "WYRD_SPEC_400_INVALID_OPERATOR"


@pytest.mark.parametrize(
    "file,field,kind,name",
    [
        (
            "scheduled-binding-empty-inputs.yaml",
            "verified_by[0].inputs",
            "Service",
            "budget-with-empty-inputs",
        ),
        (
            "scheduled-binding-exact-inputs.yaml",
            "verified_by[0].inputs",
            "Service",
            "budget-with-exact-inputs",
        ),
        ("optional-samples-slot.yaml", "inputs.latency", "Verifier", "optional-latency"),
    ],
)
def test_scheduled_binding_rejects_redundant_inputs(
    cards: Cards, file: str, field: str, kind: str, name: str
) -> None:
    with pytest.raises(WyrdError) as raised:
        cards.declare_from_path(FIXTURES / "invalid" / file)
    assert field in json.dumps(raised.value.details), raised.value.details
    with pytest.raises(WyrdError) as absent:
        cards.resolve_latest(kind, name)
    assert absent.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"


@pytest.mark.parametrize(
    "sample,verdict,outcomes",
    [
        (
            {"latency": 120, "error_rate": 0.01},
            "failed",
            {"latency-within-budget": "failed", "error-rate-within-budget": "passed"},
        ),
        (
            {"latency": 80},
            "inconclusive",
            {"latency-within-budget": "passed", "error-rate-within-budget": "inconclusive"},
        ),
    ],
)
def test_scheduled_binding_scores_every_requirement(
    latency_budget: Watch, sample: dict[str, Any], verdict: str, outcomes: dict[str, str]
) -> None:
    observe = latency_budget.state.run().observe
    for _ in range(10):
        assert isinstance(observe.submit(latency_budget.verifier, **sample), verify.Recorded)
    latency_budget.state.flush()
    latency_budget.server.flush_bifrost()

    result = scheduled_result(latency_budget.server, latency_budget.binding_id, WAIT_SECONDS)

    assert result.verdict == verdict
    assert result.subject == latency_budget.root
    assert result.verifier.name == "latency-error-budget"
    assert {
        requirement["id"]: requirement["outcome"] for requirement in result.requirements
    } == outcomes
    for requirement in result.requirements:
        assert requirement["provenance"]["kind"] == "drift"
        window = requirement["provenance"]["window"]
        assert window["start"] < window["end"], window
