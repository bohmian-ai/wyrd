"""The docsite Quickstart, run as a reader runs it.

``examples/finance-release`` holds ``finance_service.py``, which uploads the
company sharing policy PDF with its extraction and declares the
``finance-release`` Service with its three-requirement ``release-check``
Verifier, and ``finance_release.py``, which verifies four proposed releases
against it and releases only the ones that pass. The journey copies the files
and the downloaded PDF to an empty directory and runs the program against a
real server whose gateway forwards the policy judge to a local model that
reads the policy extraction first.
"""

from __future__ import annotations

import contextlib
import io
import json
import re
import runpy
import shutil
import sys
from collections.abc import Iterator, Mapping, Sequence
from pathlib import Path
from typing import Any

import pytest
from fastapi.testclient import TestClient
from wyrd import WyrdError, verify
from wyrd.cards import CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer
from wyrd.verify import Artifact, Service

from .conftest import serving
from .support import Receiver, details
from .test_local_development import configure_gateway

pytestmark = pytest.mark.integration

EXAMPLE = Path(__file__).resolve().parents[4] / "examples/finance-release"

PERMITTED: dict[str, Any] = {
    "purpose": "external audit",
    "report": {"text": "Synthetic customer report: quarterly total $120."},
    "recipient": {"email": "auditor@example.test", "authorized": True},
    "destination": {"url": "https://auditor.example.test/reports", "restricted": True},
}
"""A permitted release's evidence."""

FIRST_RUN = (EXAMPLE / "output.txt").read_text()
"""What the Quickstart program prints under the original policy, as the docs show it."""


def run_quickstart(directory: Path) -> tuple[dict, str]:
    """Run the Quickstart program in ``directory``; return its globals and printed output."""
    # Each run imports the reader's `finance_service.py` afresh.
    sys.modules.pop("finance_service", None)
    printed = io.StringIO()
    with contextlib.redirect_stdout(printed):
        script = runpy.run_path(str(directory / "finance_release.py"), run_name="__main__")
    return script, printed.getvalue()


def policy_file(requirements: Sequence[Mapping[str, Any]]) -> dict[str, Any]:
    """The policy Artifact file the ``follows-policy`` requirement mounts for its judge."""
    [follows_policy] = [each for each in requirements if each["id"] == "follows-policy"]
    return follows_policy["check"]["workspace"]["/inputs/company-sharing-policy.pdf.json"][
        "artifact"
    ]


@pytest.fixture
def reader(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """A directory holding only the Quickstart's files and policy PDF, as the working directory."""
    reader = tmp_path / "quickstart"
    reader.mkdir()
    for name in ("finance_service.py", "finance_release.py", "company-sharing-policy.pdf"):
        shutil.copy(EXAMPLE / name, reader)
    monkeypatch.chdir(reader)
    monkeypatch.syspath_prepend(str(reader))
    # Keep the developer's saved Wyrd config out of the SDK credential chain.
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
    return reader


@pytest.fixture
def model() -> Iterator[Receiver]:
    """The local model provider: it plays the policy judge."""
    with serving() as model:
        yield model


@pytest.fixture
def server(model: Receiver, monkeypatch: pytest.MonkeyPatch) -> Iterator[WyrdTestServer]:
    """A server the reader's program reaches as the tenant administrator `wyrd init` saves.

    Its ``openai`` deployment forwards to the local model, as the page's
    gateway step configures it for a real provider.
    """
    with WyrdTestServer(
        verification_runtime=True, provider_base_url=model.url, mutate_env=False
    ) as server:
        with monkeypatch.context() as env:
            env.setenv("WYRD_SERVER_URL", server.base_url)
            env.setenv("WYRD_GRPC_URL", server.grpc_url)
            env.setenv("WYRD_API_KEY", server.tenant_admin_key())
            configure_gateway(WyrdClient())
            yield server


def test_docsite_quickstart_verifies_and_repeats(
    reader: Path, server: WyrdTestServer, model: Receiver
) -> None:
    printed = []
    for _ in range(2):
        script, output = run_quickstart(reader)
        printed.append(output)
        service = script["state"].root_ref
        assert (service.kind.name, service.name, str(service.version)) == (
            "Service",
            "finance-release",
            "0.1.0",
        )
    assert printed[0] == FIRST_RUN
    assert printed[1] == printed[0], "the rerun declares nothing new"
    judged = [
        each.body
        for each in model.to("/v1/chat/completions")
        if any(message.get("role") == "tool" for message in each.body["messages"])
    ]
    assert len(judged) == 8, "one policy judgment per release"
    assert all(
        "external-audit" in json.dumps(body)
        and "/inputs/company-sharing-policy.pdf.json" in json.dumps(body)
        for body in judged
    ), "each verdict follows a read of the policy extraction"
    assert any("marketing partnership" in json.dumps(body) for body in judged), (
        "the judge sees each release's purpose"
    )

    # Each requirement is judged on its own, and exactly one verdict predicate holds.
    state = WyrdState.declare(script["service"])
    run = state.run()
    results = [
        run.observe.verify("release-check", release)
        for release in (
            PERMITTED,
            {**PERMITTED, "recipient": {"email": "x@example.test", "authorized": False}},
            {**PERMITTED, "destination": {"url": "https://example.test", "restricted": False}},
            {**PERMITTED, "report": {"text": "Synthetic report flagged PROVIDER-UNCERTAIN."}},
        )
    ]
    state.shutdown()
    assert [r.verdict for r in results] == ["passed", "failed", "failed", "inconclusive"]
    assert [(r.passed, r.failed, r.inconclusive) for r in results] == [
        (True, False, False),
        (False, True, False),
        (False, True, False),
        (False, False, True),
    ]

    # The reader downloads the revised policy over the original and reruns.
    shutil.copy(
        EXAMPLE / "company-sharing-policy-revised.pdf", reader / "company-sharing-policy.pdf"
    )
    script, revised = run_quickstart(reader)
    assert revised == FIRST_RUN.replace(
        "regulator request · Synthetic customer report: quarterly total $120. → failed\n"
        "  follows-policy: the policy does not permit sharing for this purpose\n",
        "regulator request · Synthetic customer report: quarterly total $120. → passed\n",
    ), "the revised policy releases the regulator request"
    assert str(script["state"].root_ref.version) == "0.1.1"

    cards = Cards()
    original_check = cards.get(CardRef("Verifier", "release-check", "0.1.0")).spec
    revised_check = cards.get(CardRef("Verifier", "release-check", "0.1.1")).spec
    revised_service = cards.get(CardRef("Service", "finance-release", "0.1.1")).spec
    assert policy_file(original_check["requirements"])["card"]["version"] == "0.1.0"
    assert policy_file(revised_check["requirements"])["card"]["version"] == "0.1.1"
    assert revised_service["verified_by"][0]["verifier"]["version"] == "0.1.1"

    # An execution from before the update still names the original policy file.
    earlier = details(results[0].execution_id)
    assert earlier["verifier"]["version"] == "0.1.0"
    assert policy_file(earlier["outcome"]["requirements"]) == policy_file(
        original_check["requirements"]
    )


def test_yaml_release_check_judges_like_the_code(
    reader: Path, server: WyrdTestServer, model: Receiver
) -> None:
    # The landing page's YAML tab: the same Verifier, declared from files
    # after the policy upload its header names.
    for name in ("finance-release.yaml", "release-check.yaml", "policy-judge.yaml"):
        shutil.copy(EXAMPLE / name, reader)
    Cards().upload_artifact(
        reader / "company-sharing-policy.pdf",
        {"name": "company-sharing-policy", "version": "0.1.0"},
        extract=True,
    )
    state = WyrdState.declare(reader / "finance-release.yaml")
    releases = [
        PERMITTED,
        {
            **PERMITTED,
            "report": {"text": "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001."},
        },
        {**PERMITTED, "purpose": "marketing partnership"},
        {**PERMITTED, "purpose": "regulator request"},
    ]
    with state.run() as run:
        results = [run.observe.verify("release-check", release) for release in releases]
    state.shutdown()

    assert [r.verdict for r in results] == ["passed", "failed", "failed", "failed"]
    assert [
        r["reason"] for result in results for r in result.requirements if r["outcome"] != "passed"
    ] == [
        "the report names a bank account identifier, which the policy forbids",
        "the policy does not permit sharing for this purpose",
        "the policy does not permit sharing for this purpose",
    ]
    judged = [each.body for each in model.to("/v1/chat/completions")]
    assert any("/inputs/company-sharing-policy.pdf.json" in json.dumps(body) for body in judged)


def test_docsite_quickstart_refuses_insufficient_authority(
    reader: Path, server: WyrdTestServer
) -> None:
    # The administrator keeps a bundle; a viewer may read the subject but not
    # declare it or run its Verifier.
    script, _ = run_quickstart(reader)
    service: Service = script["service"]
    WyrdState.declare(service, bundle_dir=reader.parent / "bundle")
    viewer = server.bootstrap_service(["viewer"], name="quickstart-viewer")
    client = WyrdClient(server_url=server.base_url, credential=viewer, grpc_url=server.grpc_url)

    with pytest.raises(WyrdError) as declared:
        WyrdState.declare(service, client)
    state = WyrdState.from_path(reader.parent / "bundle", client)
    with pytest.raises(WyrdError) as verified:
        state.run().observe.verify("release-check", PERMITTED)

    assert declared.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert verified.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_docsite_quickstart_refuses_changed_content_and_broken_references(
    reader: Path, server: WyrdTestServer
) -> None:
    run_quickstart(reader)
    policy = reader / "company-sharing-policy.pdf"
    shutil.copy(EXAMPLE / "company-sharing-policy-revised.pdf", policy)

    # The registered policy version keeps its content; a new one is a new version.
    with pytest.raises(WyrdError) as drifted:
        WyrdState.declare(Artifact.from_path("company-sharing-policy", "0.1.0", policy))
    with pytest.raises(WyrdError) as missing:
        WyrdState.declare(Artifact.from_path("company-sharing-policy", reader / "missing.pdf"))

    assert drifted.value.code == "WYRD_REGISTRY_409_SPEC_DRIFT"
    assert missing.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"


def test_finance_api_releases_only_passed_reports(
    reader: Path, server: WyrdTestServer, model: Receiver, monkeypatch: pytest.MonkeyPatch
) -> None:
    # The finance API beside the Quickstart Service pulled as `.wyrd-bundle`,
    # sent the request files the real-time guide's `curl` commands send.
    script, _ = run_quickstart(reader)
    WyrdState.declare(script["service"], bundle_dir=reader / ".wyrd-bundle")
    for name in ("api.py", "permitted.json", "bank-account.json"):
        shutil.copy(EXAMPLE / name, reader)
    requests = [
        json.loads((reader / name).read_text()) for name in ("permitted.json", "bank-account.json")
    ]
    uncertain = {**PERMITTED, "report": {"text": "Synthetic report flagged PROVIDER-UNCERTAIN."}}
    example = runpy.run_path(str(reader / "api.py"))
    with TestClient(example["api"]) as client:
        responses = [client.post("/reports/send", json=body) for body in (*requests, uncertain)]
    released, bank_account, inconclusive = [response.json() for response in responses]

    assert [response.status_code for response in responses] == [200, 200, 200]
    assert released["decision"] == "released"
    assert bank_account["decision"] == inconclusive["decision"] == "held for review"
    assert (bank_account["verdict"], bank_account["requirements"]) == (
        "failed",
        {"follows-policy": "failed"},
    )
    assert (inconclusive["verdict"], inconclusive["requirements"]) == (
        "inconclusive",
        {"follows-policy": "inconclusive"},
    )
    # Each decision names the execution that judged it, so it was made after
    # the verdict: only the passed one was released, each in a Run of its own.
    for response in (released, bank_account, inconclusive):
        assert details(response["execution_id"])["run_id"] == response["run_id"]
    assert verify.Execution.connect(released["execution_id"]).wait(60.0).passed
    assert len({response["run_id"] for response in (released, bank_account, inconclusive)}) == 3
    printed = "".join(f"{response.text}\n" for response in responses[:2])
    assert uuids(printed) == uuids((EXAMPLE / "api-output.txt").read_text())

    # A credential that may not run the Verifier: nothing is judged, so the
    # typed error holds the report.
    viewer = server.bootstrap_service(["viewer"], name="finance-api-viewer")
    with monkeypatch.context() as env:
        env.setenv("WYRD_API_KEY", viewer)
        example = runpy.run_path(str(reader / "api.py"))
        with TestClient(example["api"]) as client:
            refused = client.post("/reports/send", json=requests[0])
    assert refused.status_code == 503
    assert refused.json() == {
        "decision": "held for review",
        "error": "WYRD_PERMISSION_403_DENIED_RBAC",
    }


def uuids(text: str) -> str:
    """``text`` with every UUID replaced by one placeholder, so printed IDs compare."""
    return re.sub(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", "<id>", text)
