"""Helpers shared by the integration journeys; ``conftest`` holds only fixtures."""

from __future__ import annotations

import threading
from dataclasses import dataclass, field
from pathlib import Path

from pydantic import BaseModel
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
        """Never called: the journeys register checked-in Cards, they do not save one."""
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


def register(cards: Cards, path: str) -> dict[str, CardRef]:
    """Register one checked-in fixture file and return every Card it registered, by name."""
    receipt = cards.register_from_path(FIXTURES / path)
    return {outcome.card_ref.name: outcome.card_ref for outcome in receipt.outcomes}


def download(cards: Cards, service: str, destination: Path) -> Path:
    """Hydrate the registered ``service`` graph into ``destination``."""
    ref = CardRef("Service", service, "1.0.0", space="default")
    return cards.hydrate(ref, destination).destination


def service_key(service: str) -> str:
    """Issue the registered ``service``'s own key; it holds only the default Card Role."""
    return cli.issue_key(kind="Service", name=service, version="1.0.0", space="default").key


def hydrated(bundle: Path, client: WyrdClient | None = None) -> WyrdState:
    """The offline state of a downloaded Service bundle, acting as ``client``."""
    # ``interfaces=`` swaps in the stand-in for the Model's loader module, which
    # does not exist offline.
    return WyrdState.from_path(bundle, client, interfaces={"model": StandInModel()})


def binding_ids(cards: Cards, ref: CardRef) -> list[str]:
    """The verification binding ids the server derived for ``ref``."""
    status = cards.get(ref).status
    # Status and its verification block stay empty until the server derives them.
    assert status is not None and status.verification is not None
    return status.verification.binding_ids
