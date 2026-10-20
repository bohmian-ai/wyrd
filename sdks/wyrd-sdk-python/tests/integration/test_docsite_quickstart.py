"""The docsite Quickstart, run as a reader runs them.

``examples/quickstart`` holds a Verifier, the Service that references it, and
one script that declares them, pulls the bundle, and verifies a passing and a
failing refund. The journey copies the files to an empty directory and runs
the script against a real server, twice.
"""

from __future__ import annotations

import contextlib
import io
import runpy
import shutil
from collections.abc import Iterator
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration

EXAMPLE = Path(__file__).resolve().parents[4] / "examples/quickstart"


def run_quickstart(directory: Path) -> tuple[dict, str]:
    """Run the Quickstart script in ``directory``; return its globals and printed output."""
    printed = io.StringIO()
    with contextlib.redirect_stdout(printed):
        script = runpy.run_path(str(directory / "quickstart.py"), run_name="__main__")
    return script, printed.getvalue()


@pytest.fixture
def reader(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """An empty directory holding only the Quickstart's files, as the working directory."""
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    monkeypatch.chdir(tmp_path)
    # Keep the developer's saved Wyrd config out of the SDK credential chain.
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
    return tmp_path


@pytest.fixture
def server(reader: Path, monkeypatch: pytest.MonkeyPatch) -> Iterator[WyrdTestServer]:
    """A server the reader's script reaches as the tenant administrator `wyrd server dev` saves."""
    with WyrdTestServer(verification_runtime=True, mutate_env=False) as server:
        with monkeypatch.context() as env:
            env.setenv("WYRD_SERVER_URL", server.base_url)
            env.setenv("WYRD_GRPC_URL", server.grpc_url)
            env.setenv("WYRD_API_KEY", server.tenant_admin_key())
            yield server


def test_docsite_quickstart_verifies_and_repeats(reader: Path, server: WyrdTestServer) -> None:
    identities = set()
    for _ in range(2):
        script, printed = run_quickstart(reader)
        subject, verifier = script["state"].root_ref, script["result"].verifier

        assert (subject.kind.name, subject.name, subject.uid is not None) == (
            "Service",
            "support-reply",
            True,
        )
        assert (verifier.name, str(verifier.version), verifier.uid is not None) == (
            "refund-limit",
            "1.0.0",
            True,
        )
        assert printed == (
            f"passed: the comparison held\n  subject:  {subject}\n  verifier: {verifier}\n"
            f"failed: operator LessThanOrEquals returned false\n"
            f"  subject:  {subject}\n  verifier: {verifier}\n"
        )
        [broken] = script["result"].requirements
        assert (broken["id"], broken["outcome"], broken["actual"]) == (
            "within-limit",
            "failed",
            250,
        )
        identities.add((str(subject), str(verifier)))
    assert len(identities) == 1, "the rerun declares nothing new"


def test_docsite_quickstart_refuses_insufficient_authority(
    reader: Path, server: WyrdTestServer
) -> None:
    run_quickstart(reader)

    # The subject is declared and pulled; this caller may read it but not run its Verifier.
    viewer = server.bootstrap_service(["viewer"], name="quickstart-viewer")
    client = WyrdClient(server_url=server.base_url, credential=viewer, grpc_url=server.grpc_url)
    state = WyrdState.from_path(reader / ".wyrd-bundle", client)
    with pytest.raises(WyrdError) as raised:
        state.run().observe.verify("refund-limit", {"refund": 40})
    assert raised.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
