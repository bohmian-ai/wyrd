"""The docsite Quickstart, run as a reader runs them.

``examples/quickstart`` holds a Verifier, the Service that references it, and
``quickstart.py``, one script that declares them with ``WyrdState.declare`` and verifies a passing
and a failing answer. The journey copies the files to an empty directory and
runs the script against a real server, twice.
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
    reader = tmp_path / "quickstart"
    reader.mkdir()
    for path in [*EXAMPLE.glob("*.yaml"), EXAMPLE / "quickstart.py"]:
        shutil.copy(path, reader)
    monkeypatch.chdir(reader)
    # Keep the developer's saved Wyrd config out of the SDK credential chain.
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
    (tmp_path / "tmp").mkdir()
    return reader


@pytest.fixture
def server(reader: Path, monkeypatch: pytest.MonkeyPatch) -> Iterator[WyrdTestServer]:
    """A server the reader's script reaches as the tenant administrator `wyrd init` saves."""
    with WyrdTestServer(verification_runtime=True, mutate_env=False) as server:
        with monkeypatch.context() as env:
            env.setenv("WYRD_SERVER_URL", server.base_url)
            env.setenv("WYRD_GRPC_URL", server.grpc_url)
            env.setenv("WYRD_API_KEY", server.tenant_admin_key())
            # The state's private files land here, where the journey can see them.
            env.setenv("TMPDIR", str(reader.parent / "tmp"))
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
            "no-refund-promise",
            "1.0.0",
            True,
        )
        assert printed == (
            f"passed: the comparison held\n  subject:  {subject}\n  verifier: {verifier}\n"
            f"failed: operator NotContains returned false\n"
            f"  subject:  {subject}\n  verifier: {verifier}\n"
        )
        [broken] = script["result"].requirements
        assert (broken["id"], broken["outcome"], broken["actual"]) == (
            "no-refund-promise",
            "failed",
            "You have a guaranteed refund.",
        )
        identities.add((str(subject), str(verifier)))
        assert sorted(path.name for path in reader.iterdir()) == [
            "no-refund-promise.yaml",
            "quickstart.py",
            "support-reply.yaml",
        ], "the reader manages no bundle"
        assert list((reader.parent / "tmp").iterdir()) == [], "a metadata-only state keeps no files"
    assert len(identities) == 1, "the rerun declares nothing new"


def test_docsite_quickstart_refuses_insufficient_authority(
    reader: Path, server: WyrdTestServer
) -> None:
    # The administrator keeps a bundle; a viewer may read the subject but not
    # declare it or run its Verifier.
    WyrdState.declare("support-reply.yaml", bundle_dir=reader.parent / "bundle")
    viewer = server.bootstrap_service(["viewer"], name="quickstart-viewer")
    client = WyrdClient(server_url=server.base_url, credential=viewer, grpc_url=server.grpc_url)

    with pytest.raises(WyrdError) as declared:
        WyrdState.declare("support-reply.yaml", client)
    state = WyrdState.from_path(reader.parent / "bundle", client)
    with pytest.raises(WyrdError) as verified:
        state.run().observe.verify("no-refund-promise", answer="Your refund request is approved.")

    assert declared.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
    assert verified.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"


def test_docsite_quickstart_refuses_changed_content_and_broken_references(
    reader: Path, server: WyrdTestServer
) -> None:
    run_quickstart(reader)
    verifier = reader / "no-refund-promise.yaml"
    verifier.write_text(verifier.read_text().replace("expected: guaranteed refund", "expected: guaranteed"))

    with pytest.raises(WyrdError) as drifted:
        WyrdState.declare("support-reply.yaml")
    verifier.unlink()
    with pytest.raises(WyrdError) as missing:
        WyrdState.declare("support-reply.yaml")

    assert drifted.value.code == "WYRD_REGISTRY_409_SPEC_DRIFT"
    assert missing.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"
