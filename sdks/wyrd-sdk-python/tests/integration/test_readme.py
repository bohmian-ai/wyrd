"""The root README's Declare, Observe, Verify walkthrough, run verbatim.

``examples/readme`` holds the README's Cards and Python as files. The first
test proves those files are exactly what the README shows; the journey then
runs the README's own shell commands and Python against a real server, and
reruns the README's Bifrost query until both verdicts are readable.
"""

from __future__ import annotations

import contextlib
import io
import re
import runpy
import shutil
import subprocess
import time
from pathlib import Path

import pytest
from wyrd.testing import WyrdTestServer

ROOT = Path(__file__).resolve().parents[4]
README = ROOT / "README.md"
EXAMPLE = ROOT / "examples/readme"
CARDS = ("service.yaml", "agent.yaml", "no-refund-promise.yaml", "names-order.yaml")
"""The README's YAML documents, in the order the README shows them."""

VERDICT_TIMEOUT_SECONDS = 90.0
"""How long the journey waits for the submitted verdict to become queryable."""


def blocks(language: str) -> list[str]:
    """Every fenced ``language`` code block in the README, in order."""
    return re.findall(rf"```{language}\n(.*?)```", README.read_text(), re.S)


def test_readme_example_files_match_the_readme_verbatim() -> None:
    cards = [(EXAMPLE / "refund-letters" / name).read_text().rstrip("\n") for name in CARDS]
    assert blocks("yaml") == ["\n---\n".join(cards) + "\n"]
    assert blocks("python") == [
        (EXAMPLE / name).read_text() for name in ("observe.py", "verify.py")
    ]


@pytest.mark.integration
def test_readme_declares_observes_and_verifies(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    monkeypatch.chdir(tmp_path)
    # Keep the developer's saved Wyrd config out of the CLI and SDK credential chain.
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
    (declare,) = blocks("bash")

    with WyrdTestServer(verification_runtime=True) as server:
        # The README reader uses the tenant administrator that `wyrd server dev` saves.
        monkeypatch.setenv("WYRD_API_KEY", server.tenant_admin_key())
        subprocess.run(["bash", "-euc", declare], check=True)
        assert (tmp_path / ".wyrd-bundle").is_dir()

        with contextlib.redirect_stdout(io.StringIO()):
            script = runpy.run_path(str(tmp_path / "observe.py"), run_name="__main__")
        assert script["result"].verdict == "passed"

        # verify.py continues observe.py's script; the submitted verdict settles in the background.
        query = (tmp_path / "verify.py").read_text()
        deadline = time.monotonic() + VERDICT_TIMEOUT_SECONDS
        while True:
            server.flush_bifrost()
            with contextlib.redirect_stdout(io.StringIO()):
                exec(query, script)
            rows = script["rows"].to_arrow().to_pylist()
            if len(rows) == 2 or time.monotonic() > deadline:
                break
            time.sleep(1)

        assert [(row["order_id"], row["verdict"]) for row in rows] == [
            ("A-1001", "passed"),
            ("A-1001", "passed"),
        ], rows
        assert len({row["execution_id"] for row in rows}) == 2
