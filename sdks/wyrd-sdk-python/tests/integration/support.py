"""Helpers shared by the integration journeys; ``conftest`` holds only fixtures."""

from __future__ import annotations

import threading
import time
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import httpx
from pydantic import BaseModel
from wyrd import verify
from wyrd.bifrost import Bifrost, SpanRecord
from wyrd.cards import CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.model import ModelInterface
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer, cli

FIXTURES = Path(__file__).resolve().parents[4] / "fixtures"
"""The repository-root fixture corpus shared by the Rust, Python, and TypeScript journeys."""

BASELINE_TIMEOUT_SECONDS = 90.0
"""How long a fixture waits for a Drift Verifier's baseline to be fitted."""

ON_CALL_TOKEN = "on-call-bearer-token"
"""The bearer secret the ``on-call-hooks`` connection presents to the receiver."""

LIVE_REVIEW = "spans were reviewed live"
"""The ``coding-verifier`` statement every request of its ``spans-reviewed`` check carries."""

FRAME_CLAIM = "Cite the frame."
"""Text every ``frame-verifier`` judge request carries."""

UNBADGED = "no badge!"
"""The nine-byte label of the one frame whose person wears no badge."""

UNBADGED_BASE64 = "bm8gYmFkZ2Uh"
"""``UNBADGED`` in base64, as the judge's image data URL carries it."""


@dataclass
class Delivery:
    """One request a local receiver accepted."""

    path: str
    headers: dict[str, str]
    body: dict


@dataclass
class Receiver:
    """A loopback endpoint that records every request in arrival order."""

    url: str
    deliveries: list[Delivery] = field(default_factory=list)
    delivered: threading.Condition = field(default_factory=threading.Condition)

    def record(self, delivery: Delivery) -> None:
        """Append ``delivery`` and wake every ``wait_for``."""
        with self.delivered:
            self.deliveries.append(delivery)
            self.delivered.notify_all()

    def to(self, path: str) -> list[Delivery]:
        """Every delivery received on ``path`` so far."""
        return [delivery for delivery in self.deliveries if delivery.path == path]

    def wait_for(self, path: str, timeout: float) -> Delivery:
        """Block until a delivery arrives on ``path`` and return the first one.

        Raises:
            TimeoutError: when nothing reaches ``path`` within ``timeout`` seconds.
        """
        with self.delivered:
            if not self.delivered.wait_for(lambda: self.to(path), timeout):
                raise TimeoutError(f"nothing was delivered to {path} in {timeout} s")
        return self.to(path)[0]


class Count(BaseModel):
    """One ``COUNT(*)`` result."""

    n: int


class QueryRow(BaseModel):
    """The user columns of the query journeys' seeded table."""

    id: int
    value: str


class StandInModel(ModelInterface):
    """Stand-in for the fixture Models' ``Custom`` loader.

    The journeys observe and verify the Model Card without running it, and the
    Card names a loader module that does not exist offline. Supplying this
    instance for the alias keeps hydration from importing it.
    """

    def __init__(self) -> None:
        """Start with the empty holder slot the Model holder expects after load."""
        super().__init__()
        self.model: object = None

    def save(self, path: Path, save_kwargs: dict[str, object] | None = None) -> None:
        """Never called: the journeys declare checked-in Cards, they do not save one."""
        raise NotImplementedError

    def load(self, path: Path, load_kwargs: dict[str, object] | None = None) -> None:
        """Read nothing and publish an identity function as the loaded model."""
        self.model = lambda value: value


def client_of(server: WyrdTestServer, credential: str | None = None) -> WyrdClient:
    """A client of ``server`` acting as ``credential``, by default its administrator."""
    return WyrdClient(
        server_url=server.base_url,
        credential=credential or server.api_key,
        grpc_url=server.grpc_url,
    )


def timeline(server: WyrdTestServer, trace_id: str) -> list[SpanRecord]:
    """Publish every captured row, then read trace ``trace_id``'s spans as ``server``'s administrator."""
    server.flush_bifrost()
    return Bifrost(client=client_of(server)).trace_timeline(trace_id)


def ancestry(spans: Sequence[SpanRecord], span: SpanRecord) -> list[SpanRecord]:
    """``span`` and each ancestor its parent ids reach within ``spans``, nearest first."""
    by_id = {(each["trace_id"], each["span_id"]): each for each in spans}
    chain = [span]
    while (
        parent := by_id.get((chain[-1]["trace_id"], chain[-1].get("parent_span_id") or ""))
    ) is not None:
        chain.append(parent)
    return chain


def declare(cards: Cards, path: str) -> dict[str, CardRef]:
    """Register one checked-in fixture file and return every Card it registered, by name."""
    receipt = cards.declare_from_path(FIXTURES / path)
    return {outcome.card_ref.name: outcome.card_ref for outcome in receipt.outcomes}


def pull(cards: Cards, service: str, destination: Path) -> Path:
    """Hydrate the registered ``service`` graph into ``destination``."""
    ref = CardRef("Service", service, "1.0.0")
    return cards.pull(ref, destination).destination


def service_key(service: str, client: WyrdClient | None = None) -> str:
    """Issue the registered ``service``'s own key as ``client``; it holds only the default Card Role."""
    return cli.issue_key(kind="Service", name=service, version="1.0.0", client=client).key


def hydrated(bundle: Path, client: WyrdClient | None = None) -> WyrdState:
    """The offline state of a downloaded Service bundle, acting as ``client``."""
    # ``interfaces=`` swaps in the stand-in for the Model's loader module, which
    # does not exist offline.
    return WyrdState.from_path(bundle, client, interfaces={"model": StandInModel()})


def binding_ids(cards: Cards, ref: CardRef) -> list[str]:
    """The verification binding ids the server derived for ``ref``."""
    status = cards.get(ref).status
    # Status and its verification block stay empty until the server derives them.
    assert status is not None and status.get("verification") is not None
    return status["verification"]["binding_ids"]


def group_binding(server: WyrdTestServer, cards: Cards, ref: CardRef) -> str:
    """The one ``group_end`` binding the server derived for ``ref``."""
    [binding_id] = [
        binding
        for binding in binding_ids(cards, ref)
        if binding_status(server, binding).get("group_end") is not None
    ]
    return binding_id


def binding_status(server: WyrdTestServer, binding_id: str) -> dict[str, Any]:
    """``binding_id``'s status, read as the administrator over HTTP."""
    response = httpx.get(
        f"{server.base_url}/v1/verification/bindings/{binding_id}",
        headers={"x-wyrd-access-token": f"Bearer {server.access_token()}"},
        timeout=30,
    )
    response.raise_for_status()
    return response.json()


class Retained(BaseModel):
    """One ``vala.verification.results`` row's execution identity."""

    execution_id: str


def scheduled_result(
    server: WyrdTestServer, binding_id: str, timeout: float
) -> verify.VerificationResult:
    """Make ``binding_id`` due and read back the result the scheduler retains for it.

    Results the binding already retained are skipped, so a binding made due
    again yields its newest judgment. Rows are published until a new one
    names an execution, whose typed result is then read by its ID.
    """
    admin = client_of(server)
    bifrost = Bifrost(client=admin)
    query = "SELECT execution_id FROM vala.verification.results WHERE binding_id = $1"

    def retained() -> set[str]:
        server.flush_bifrost()
        return {row.execution_id for row in bifrost.sql(query, [binding_id], model=Retained)}

    earlier = retained()
    server.make_binding_due(binding_id)
    deadline = time.monotonic() + timeout
    while not (new := retained() - earlier):
        assert time.monotonic() < deadline, (
            f"binding {binding_id} retained no result in {timeout} s"
        )
        time.sleep(0.5)
    [execution_id] = new
    return verify.Execution.connect(execution_id, admin).wait(timeout)


def group_execution(
    server: WyrdTestServer, binding_id: str, timeout: float, after: str | None = None
) -> verify.ExecutionState:
    """The newest completed execution of group ``binding_id`` other than ``after``.

    Rows are published so the group poller discovers them, then the binding's
    ``last_execution_id`` is read until that execution completes with a result.
    """
    admin = client_of(server)
    deadline = time.monotonic() + timeout
    while True:
        server.flush_bifrost()
        latest = binding_status(server, binding_id).get("last_execution_id")
        if latest is not None and latest != after:
            state = verify.Execution.connect(latest, admin).status()
            if state["status"] == "completed" and "result" in state:
                return state
        assert time.monotonic() < deadline, (
            f"group binding {binding_id} completed no new execution in {timeout} s"
        )
        time.sleep(0.5)


@dataclass
class Watch:
    """The deployment of one scheduled-drift story.

    A server of its own, since a scheduled window spans every sample since
    the last cron boundary, with the Service hydrated under its own key, the
    ``on-call-hooks`` connection pointing at ``hooks``, and every sample
    submitted to the scheduled ``verifier``.
    """

    server: WyrdTestServer
    state: WyrdState
    root: CardRef
    binding_id: str
    hooks: Receiver
    verifier: str
