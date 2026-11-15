"""A service verifies through hosted Python while process memory is contended.

``verify`` waits for memory and then passes, is refused as retryable ``503``
when memory cannot even hold the waiting request, and ends in ``504`` at the
one deadline from its arrival without a replay; ``submit`` defers a queued
execution until memory frees.
"""

from __future__ import annotations

import time
from collections.abc import Generator
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from pathlib import Path

import pytest
from wyrd import WyrdError, verify
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import declare, pull, service_key

pytestmark = pytest.mark.integration

VERIFY_TIMEOUT_SECONDS = 2.0
"""The direct deadline every journey here runs under."""

SHORT_OF_A_SESSION = 64 << 20
"""Bytes left free while an operation should wait: short of one session envelope."""

REQUEST_BYTES = 256 << 10
"""Bytes of the answer the full-memory refusal sends."""

WAIT_SECONDS = 60.0
"""How long a test waits for a queued execution or a waiter to appear."""


@contextmanager
def capacity_desk(tmp_path: Path) -> Generator[tuple[WyrdTestServer, WyrdState]]:
    """A server hosting Python Verifiers under a short deadline, and the hydrated ``capacity-desk``."""
    with WyrdTestServer(
        verification_runtime=True, verify_timeout_seconds=VERIFY_TIMEOUT_SECONDS
    ) as server:
        cards = Cards(WyrdClient())
        declare(cards, "cards/verification_capacity/capacity-desk.yaml")
        state = WyrdState.from_path(
            pull(cards, "capacity-desk", tmp_path / "desk"),
            WyrdClient(credential=service_key("capacity-desk")),
        )
        state.start_bifrost()
        try:
            yield server, state
        finally:
            state.shutdown()


def await_waiters(server: WyrdTestServer, count: int) -> None:
    """Block until ``count`` hosted operations wait for process memory."""
    server.wait_for_hosted_waiters(count, WAIT_SECONDS)


def test_verify_waits_for_hosted_memory_then_passes(tmp_path: Path) -> None:
    with capacity_desk(tmp_path) as (server, state), ThreadPoolExecutor(max_workers=1) as pool:
        with server.occupy_hosted_memory(SHORT_OF_A_SESSION):
            waiting = pool.submit(lambda: state.run("agent").observe.verify("adds-one", 2))
            await_waiters(server, 1)
            assert not waiting.done(), "the verify waits for memory"
        assert waiting.result(WAIT_SECONDS).verdict == "passed"


def test_submit_defers_under_hosted_memory_then_passes(tmp_path: Path) -> None:
    with capacity_desk(tmp_path) as (server, state):
        with server.occupy_hosted_memory(SHORT_OF_A_SESSION):
            accepted = state.run("agent").observe.submit("adds-one", 2)
            await_waiters(server, 1)
        assert isinstance(accepted, verify.Execution)
        assert accepted.wait(WAIT_SECONDS).verdict == "passed"


def test_verify_refusals_follow_the_arrival_deadline(tmp_path: Path) -> None:
    with capacity_desk(tmp_path) as (server, state):
        with server.occupy_hosted_memory(0), pytest.raises(WyrdError) as refused:
            state.run("agent").observe.verify("adds-one", "2" * REQUEST_BYTES)
        assert refused.value.status == 503
        assert refused.value.code == "WYRD_SERVER_503_SERVICE_UNAVAILABLE"
        assert refused.value.details["reason"] == (
            "process memory cannot hold the Python operation now"
        )

        with server.occupy_hosted_memory(SHORT_OF_A_SESSION):
            started = time.monotonic()
            with pytest.raises(WyrdError) as timed_out:
                state.run("agent").observe.verify("adds-one", 2)
            elapsed = time.monotonic() - started
            assert timed_out.value.status == 504
            assert timed_out.value.code == "WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT"
            assert VERIFY_TIMEOUT_SECONDS <= elapsed < 2 * VERIFY_TIMEOUT_SECONDS, (
                f"one wait ends at the deadline, unreplayed: {elapsed}"
            )
            await_waiters(server, 0)

        assert state.run("agent").observe.verify("adds-one", 2).verdict == "passed"
