"""The README example, run as a reader copies it.

``examples/support-agent/support_agent.py`` declares a two-requirement
Verifier in code, asks a support agent one question with the stock OpenAI
client, and verifies the agent's reply and a policy-breaking reply. The
journey runs the file alone in an empty directory against a real server whose
``openai`` deployment forwards the judge to a local model, with the agent's
own OpenAI client pointed at the same local model.
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
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

from .conftest import SUPPORT_AGENT, SUPPORT_JUDGE, serving
from .support import Receiver
from .test_local_development import configure_gateway

pytestmark = pytest.mark.integration

EXAMPLE = Path(__file__).resolve().parents[4] / "examples/support-agent"


@pytest.fixture
def model() -> Iterator[Receiver]:
    """The local model provider: it plays the support agent and the judge."""
    with serving() as model:
        yield model


@pytest.fixture
def reader(tmp_path: Path, model: Receiver, monkeypatch: pytest.MonkeyPatch) -> Iterator[Path]:
    """An empty directory holding only the README file, reached as `wyrd init` leaves it."""
    reader = tmp_path / "readme"
    reader.mkdir()
    shutil.copy(EXAMPLE / "support_agent.py", reader)
    monkeypatch.chdir(reader)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
    monkeypatch.setenv("OPENAI_BASE_URL", f"{model.url}/v1")
    monkeypatch.setenv("OPENAI_API_KEY", "sk-local-upstream")
    with WyrdTestServer(
        verification_runtime=True, provider_base_url=model.url, mutate_env=False
    ) as server:
        monkeypatch.setenv("WYRD_SERVER_URL", server.base_url)
        monkeypatch.setenv("WYRD_GRPC_URL", server.grpc_url)
        monkeypatch.setenv("WYRD_API_KEY", server.tenant_admin_key())
        configure_gateway(WyrdClient())
        yield reader


def run_example(directory: Path) -> str:
    """Run the README program in ``directory`` and return what it prints."""
    printed = io.StringIO()
    with contextlib.redirect_stdout(printed):
        runpy.run_path(str(directory / "support_agent.py"), run_name="__main__")
    return printed.getvalue()


def test_readme_example_verifies_the_agent_reply(reader: Path, model: Receiver) -> None:
    assert run_example(reader) == (EXAMPLE / "output.txt").read_text()

    calls = [json.dumps(each.body) for each in model.to("/v1/chat/completions")]
    asked = [call for call in calls if SUPPORT_AGENT in call]
    judged = [call for call in calls if SUPPORT_JUDGE in call]
    assert len(asked) == 1, "the agent answers once"
    assert len(judged) == 2, "the judge reads each reply"
    assert all("45 days ago" in call for call in judged), "the judge sees the question"

    assert run_example(reader) == (EXAMPLE / "output.txt").read_text(), (
        "a rerun declares nothing new and judges the same"
    )
