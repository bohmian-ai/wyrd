"""CI publishes a deferred input after review starts; the waiting execution resumes.

The pull request's execution waits for ``ci``; ``Evidence.publish`` names the
repository and commit explicitly or through ``WYRD_REPOSITORY`` /
``WYRD_COMMIT``, and the receipt is durable and replayable.
"""

import time
from collections.abc import Iterator
from pathlib import Path

import pytest
from wyrd import WyrdError, verify
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.github import GithubConnection
from wyrd.testing import GithubAppMock, GithubBrowser, WyrdTestServer

from .support import FIXTURES, binding_ids, binding_status

pytestmark = pytest.mark.integration

HEAD = "a1" * 20


@pytest.fixture
def mock() -> GithubAppMock:
    """A GitHub where ``octo`` installs ``101`` with ``acme/api`` and ``acme/web``."""
    mock = GithubAppMock()
    mock.add_installation(101, [(1, "acme/api"), (2, "acme/web")])
    mock.add_user("octo", [101])
    mock.sign_in("octo")
    mock.install_target(101)
    return mock


@pytest.fixture
def server(mock: GithubAppMock) -> Iterator[WyrdTestServer]:
    """A deployment that runs Verifiers, with its GitHub App against ``mock``."""
    with WyrdTestServer(mutate_env=False, github=mock, verification_runtime=True) as server:
        yield server


@pytest.fixture
def admin(server: WyrdTestServer) -> WyrdClient:
    """A tenant administrator who connected installation ``101``."""
    key = server.bootstrap_service(["admin"], name="evidence-admin")
    admin = WyrdClient(server_url=server.base_url, credential=key, grpc_url=server.grpc_url)
    receipt = GithubConnection(admin).begin()
    browser = GithubBrowser.sign_in(server.base_url, server.tenant_slug, key)
    status, view, _ = browser.open(receipt["authorization_url"])
    assert status == 200, view
    return admin


def until(execution: verify.Execution, status: str) -> "verify.ExecutionState":
    """Poll ``execution`` until it reaches ``status``, for at most 30 seconds."""
    deadline = time.monotonic() + 30
    while (state := execution.status())["status"] != status:
        assert time.monotonic() < deadline, state
        time.sleep(0.1)
    return state


def test_published_evidence_resumes_the_waiting_execution(
    admin: WyrdClient,
    mock: GithubAppMock,
    server: WyrdTestServer,
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    cards = Cards(admin)
    receipt = cards.declare_from_path(FIXTURES / "cards/deferred_evidence/repository.yaml")
    [binding] = binding_ids(cards, receipt.root)
    mock.set_pull_request(2, 7, "b1" * 20, HEAD)
    payload = mock.pull_request_event(101, 2, 7, "opened")
    assert mock.deliver(server.base_url, "pull_request", payload) == 202
    execution = verify.Execution(binding_status(server, binding)["last_execution_id"], admin)
    assert until(execution, "waiting")["waiting"] == ["ci"]

    evidence = verify.Evidence(admin)
    accepted = evidence.publish("ci", {"status": "green"}, repository="acme/web", commit=HEAD)
    assert accepted["status"] == "accepted"
    assert accepted["matched_subscriptions"] == 1
    assert accepted["repository"]["repository_id"] == "2"
    assert accepted["tags"] == {"commit": HEAD}
    assert execution.wait(30).verdict == "passed"

    monkeypatch.setenv("WYRD_REPOSITORY", "acme/web")
    monkeypatch.setenv("WYRD_COMMIT", HEAD)
    monkeypatch.chdir(tmp_path)
    replayed = evidence.publish("ci", {"status": "green"})
    assert replayed["status"] == "replayed"
    assert replayed["evidence_id"] == accepted["evidence_id"]
    assert replayed["recorded_at"] == accepted["recorded_at"]

    for value, repository, code in (
        ({"status": "red"}, "acme/web", "WYRD_VERIFICATION_409_EVIDENCE_CONFLICT"),
        ({"status": "green"}, "acme/vault", "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE"),
    ):
        with pytest.raises(WyrdError) as refused:
            evidence.publish("ci", value, repository=repository)
        assert refused.value.code == code
