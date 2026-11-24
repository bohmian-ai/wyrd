"""CI publishes a deferred input after review starts; the waiting execution resumes.

The pull request's execution waits for ``ci``; ``Evidence.publish`` names the
repository and commit explicitly or through ``WYRD_REPOSITORY`` /
``WYRD_COMMIT``, and the receipt is durable and replayable. A pull request may
also wait for a person: another principal approves the pending signoff with
``Execution.decide`` and the same execution passes. An approver bound only on
the shared Verifier is refused until it may also read the owning Repository.
Once the settled execution's scheduling row is removed, a Repository reader
still reads the same pull request, result and GitHub delivery.
"""

import time
from collections.abc import Iterator
from pathlib import Path

import pytest
from wyrd import WyrdError, verify
from wyrd.auth import Principals
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
    state = until(execution, "waiting")
    assert state["waiting"] == ["ci"]
    assert state["input_waits"] == [{"task_id": "green", "slots": ["ci"]}]

    evidence = verify.Evidence(admin)
    accepted = evidence.publish("ci", {"status": "green"}, repository="acme/web", commit=HEAD)
    assert accepted["status"] == "accepted"
    assert accepted["matched_subscriptions"] == 1
    assert accepted["repository"]["repository_id"] == "2"
    assert accepted["tags"] == {"commit": HEAD}
    assert execution.wait(30).verdict == "passed"
    assert "input_waits" not in execution.status()

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


def client_for(server: WyrdTestServer, role: str) -> WyrdClient:
    """A client acting as a fresh service holding ``role``."""
    key = server.bootstrap_service([role], name=f"signoff-{role}")
    return WyrdClient(server_url=server.base_url, credential=key, grpc_url=server.grpc_url)


def test_a_person_approves_the_pending_signoff(
    admin: WyrdClient,
    mock: GithubAppMock,
    server: WyrdTestServer,
) -> None:
    cards = Cards(admin)
    receipt = cards.declare_from_path(FIXTURES / "cards/approvals/repository.yaml")
    [binding] = binding_ids(cards, receipt.root)
    mock.set_pull_request(2, 7, "b1" * 20, HEAD)
    payload = mock.pull_request_event(101, 2, 7, "opened")
    assert mock.deliver(server.base_url, "pull_request", payload) == 202
    execution_id = binding_status(server, binding)["last_execution_id"]
    execution = verify.Execution(execution_id, admin)
    state = until(execution, "waiting")
    assert state["waiting"] == ["ci"]
    [pending] = state["approvals"]
    assert pending["step_id"] == "signoff"
    assert pending["message"] == "Ship this change?"
    assert pending["status"] == "pending"
    assert pending["actor_id"] is None
    approval_id = pending["approval_id"]
    assert state["input_waits"] == [{"task_id": "green", "slots": ["ci"]}]

    principals = Principals(admin)

    def reader(name: str, kind: str, card: str) -> verify.Execution:
        family = {"kind": kind, "name": card}
        created = principals.create_service_principal(
            {
                "name": name,
                "assignments": [{"role": "viewer", "scope": {"card": {"family": family}}}],
            }
        )
        client = WyrdClient(
            server_url=server.base_url, credential=created["credential"], grpc_url=server.grpc_url
        )
        return verify.Execution(execution_id, client)

    repository_reader = reader("signoff-repository-reader", "Repository", "web")
    seen = repository_reader.status()
    assert seen["input_waits"] == [{"task_id": "green", "slots": ["ci"]}]
    verifier_reader = reader("signoff-verifier-reader", "Verifier", "signoff")
    with pytest.raises(WyrdError) as unseen:
        verifier_reader.status()
    assert unseen.value.code == "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"

    verify.Evidence(admin).publish("ci", {"status": "green"}, repository="acme/web", commit=HEAD)
    deadline = time.monotonic() + 30
    while (state := execution.status())["status"] != "waiting" or state.get("waiting"):
        assert time.monotonic() < deadline, state
        time.sleep(0.1)
    assert "input_waits" not in state

    viewer = verify.Execution(execution_id, client_for(server, "viewer"))
    with pytest.raises(WyrdError) as refused:
        viewer.decide(approval_id, "approved")
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"

    editor = verify.Execution(execution_id, client_for(server, "editor"))
    with pytest.raises(WyrdError) as refused:
        editor.decide(approval_id, "approved")
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"

    created = principals.create_service_principal(
        {
            "name": "signoff-approver",
            "assignments": [
                {
                    "role": "approver",
                    "scope": {"card": {"family": {"kind": "Verifier", "name": "signoff"}}},
                }
            ],
        }
    )
    approver = verify.Execution(
        execution_id,
        WyrdClient(
            server_url=server.base_url, credential=created["credential"], grpc_url=server.grpc_url
        ),
    )
    with pytest.raises(WyrdError) as refused:
        approver.decide(approval_id, "approved")
    assert refused.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"

    principals.bind(
        {
            "subject": {"principal": {"id": created["principal_id"], "kind": "service"}},
            "role": "viewer",
            "scope": {"card": {"family": {"kind": "Repository", "name": "web"}}},
        }
    )
    decided = approver.decide(approval_id, "approved", reason="reviewed the diff")
    assert decided["approval_id"] == approval_id
    assert decided["status"] == "approved"
    assert decided["reason"] == "reviewed the diff"
    assert decided["actor_id"] is not None
    assert decided["decided_at"] is not None
    assert approver.decide(approval_id, "approved", reason="reviewed the diff") == decided

    assert execution.wait(30).verdict == "passed"
    [approved] = execution.status()["approvals"]
    assert approved["status"] == "approved"
    assert approved["actor_id"] == decided["actor_id"]

    deadline = time.monotonic() + 30
    while (before := repository_reader.status()).get("github_delivery", {}).get("status") in (
        None,
        "pending",
    ):
        assert time.monotonic() < deadline, before
        time.sleep(0.1)
    assert before["pull_request"]["members"] == [execution_id]
    server.remove_settled_execution_row(execution_id)
    after = repository_reader.status()
    assert after["status"] == "completed"
    for field in ("pull_request", "result", "github_delivery"):
        assert after[field] == before[field], field
    with pytest.raises(WyrdError) as unseen:
        verifier_reader.status()
    assert unseen.value.code == "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND"
