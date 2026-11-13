"""The docsite Quickstart, run as a reader runs it.

``examples/finance-release`` holds ``finance_release.py``, the company sharing
policy PDF, and ``releases.json``. The program declares the ``finance-release``
Service with its three-requirement ``release-check`` Verifier, verifies each
proposed release once, and releases only a passed one. The journey copies the
files to an empty directory and runs the program against a real server whose
gateway forwards the policy judge to a local model that reads the policy first.
"""

from __future__ import annotations

import contextlib
import io
import json
import runpy
import shutil
from collections.abc import Iterator
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer
from wyrd.verify import Artifact, Service

from .conftest import serving
from .support import Receiver
from .test_local_development import configure_gateway

pytestmark = pytest.mark.integration

EXAMPLE = Path(__file__).resolve().parents[4] / "examples/finance-release"

PERMITTED = {
    "report": {"text": "Synthetic customer report: quarterly total $120."},
    "recipient": {"email": "auditor@example.test", "authorized": True},
    "destination": {"url": "https://auditor.example.test/reports", "restricted": True},
}
"""A permitted release's evidence."""


def run_quickstart(directory: Path) -> tuple[dict, str]:
    """Run the Quickstart program in ``directory``; return its globals and printed output."""
    printed = io.StringIO()
    with contextlib.redirect_stdout(printed):
        script = runpy.run_path(str(directory / "finance_release.py"), run_name="__main__")
    return script, printed.getvalue()


def write_releases(directory: Path, releases: list[dict]) -> None:
    """Replace the reader's ``releases.json`` with ``releases``."""
    (directory / "releases.json").write_text(json.dumps(releases))


@pytest.fixture
def reader(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """An empty directory holding only the Quickstart's files, as the working directory."""
    reader = tmp_path / "quickstart"
    reader.mkdir()
    for name in ("finance_release.py", "company-sharing-policy.pdf", "releases.json"):
        shutil.copy(EXAMPLE / name, reader)
    monkeypatch.chdir(reader)
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
    assert printed[0] == (
        "quarterly-report: passed (release-check@0.1.0) -> released\n"
        "report-with-bank-account: failed (release-check@0.1.0) -> held for review\n"
        "  follows-policy failed: the report names a bank account identifier\n"
    )
    assert printed[1] == printed[0], "the rerun declares nothing new"
    judged = [
        each.body
        for each in model.to("/v1/chat/completions")
        if any(message.get("role") == "tool" for message in each.body["messages"])
    ]
    assert len(judged) == 4, "one policy judgment per release"
    assert all("external-audit" in json.dumps(body) for body in judged), (
        "each verdict follows a read of the policy extraction"
    )

    write_releases(
        reader,
        [
            {
                "name": "unknown-recipient",
                **PERMITTED,
                "recipient": {"email": "x@example.test", "authorized": False},
            },
            {
                "name": "public-destination",
                **PERMITTED,
                "destination": {"url": "https://example.test", "restricted": False},
            },
            {
                "name": "uncertain-policy",
                **PERMITTED,
                "report": {"text": "Synthetic report flagged PROVIDER-UNCERTAIN."},
            },
            {"name": "unreadable-report", **PERMITTED, "report": {"text": 123}},
        ],
    )
    script, output = run_quickstart(reader)
    lines = output.splitlines()
    assert lines[0] == "unknown-recipient: failed (release-check@0.1.0) -> held for review"
    assert lines[1].startswith("  recipient-authorized failed: ")
    assert lines[2] == "public-destination: failed (release-check@0.1.0) -> held for review"
    assert lines[3].startswith("  destination-restricted failed: ")
    assert lines[4:] == [
        "uncertain-policy: inconclusive (release-check@0.1.0) -> held for review",
        "  follows-policy inconclusive: the policy does not settle this release",
        "unreadable-report: WYRD_VERIFICATION_400_INPUT_INVALID -> held for review",
    ]


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
        state.run().observe.verify("release-check", **PERMITTED)

    assert declared.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert verified.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_docsite_quickstart_refuses_changed_content_and_broken_references(
    reader: Path, server: WyrdTestServer
) -> None:
    run_quickstart(reader)
    policy = reader / "company-sharing-policy.pdf"
    policy.write_bytes(policy.read_bytes() + b"% revision 2\n")

    # The registered policy version keeps its content; a new one is a new version.
    with pytest.raises(WyrdError) as drifted:
        WyrdState.declare(Artifact.from_path("company-sharing-policy", "0.1.0", policy))
    policy.unlink()
    with pytest.raises(WyrdError) as missing:
        run_quickstart(reader)

    assert drifted.value.code == "WYRD_REGISTRY_409_SPEC_DRIFT"
    assert missing.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"
