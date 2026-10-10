"""Connecting MCP hosts to a Wyrd server writes only the selected hosts' launch entries.

Each test runs in an isolated ``HOME`` with every supported host detected and
holding an unrelated entry, and connects hosts to the session server through
the in-process ``wyrd mcp install``.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

import pytest
from wyrd import WyrdError
from wyrd.testing import WyrdTestServer, cli

pytestmark = pytest.mark.integration

VSCODE_PROFILE = (
    "Library/Application Support/Code/User" if sys.platform == "darwin" else ".config/Code/User"
)


@pytest.fixture
def hosts(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> dict[str, Path]:
    """Every host's configuration file in an isolated home, each with an ``other`` entry."""
    monkeypatch.setenv("HOME", str(tmp_path))
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "wyrd"))
    for name in ("XDG_CONFIG_HOME", "CODEX_HOME", "COPILOT_HOME"):
        monkeypatch.delenv(name, raising=False)
    files = {
        "codex": tmp_path / ".codex/config.toml",
        "claude-code": tmp_path / ".claude.json",
        "copilot-cli": tmp_path / ".copilot/mcp-config.json",
        "vscode": tmp_path / VSCODE_PROFILE / "mcp.json",
    }
    (tmp_path / ".claude").mkdir()
    for path in files.values():
        path.parent.mkdir(parents=True, exist_ok=True)
    files["codex"].write_text('# mine\n[mcp_servers.other]\ncommand = "other"\n')
    files["claude-code"].write_text('{"mcpServers":{"other":{"command":"other"}}}')
    files["copilot-cli"].write_text('{"mcpServers":{"other":{"command":"other"}}}')
    files["vscode"].write_text('{"servers":{"other":{"command":"other"}}}')
    return files


def snapshot(hosts: dict[str, Path]) -> dict[str, str]:
    """Every host file's current text."""
    return {name: path.read_text() for name, path in hosts.items()}


def test_install_changes_only_selected_hosts(
    hosts: dict[str, Path], wyrd_server: WyrdTestServer
) -> None:
    before = snapshot(hosts)

    report = cli.mcp_install(["codex", "claude-code"], server=wyrd_server.base_url)

    assert [(host.host, host.status) for host in report.hosts] == [
        ("codex", "added"),
        ("claude-code", "added"),
    ]
    after = snapshot(hosts)
    assert after["copilot-cli"] == before["copilot-cli"]
    assert after["vscode"] == before["vscode"]
    assert after["codex"].startswith('# mine\n[mcp_servers.other]\ncommand = "other"\n')
    assert f'args = ["mcp", "proxy", "--server", "{wyrd_server.base_url}"]' in after["codex"]
    claude = json.loads(after["claude-code"])["mcpServers"]
    assert claude["other"] == {"command": "other"}
    assert claude["wyrd"]["type"] == "stdio"
    assert claude["wyrd"]["env"] == {"WYRD_CONFIG_HOME": os.environ["WYRD_CONFIG_HOME"]}
    assert all(os.environ["WYRD_API_KEY"] not in text for text in after.values())


def test_repeat_install_rewrites_nothing(hosts: dict[str, Path]) -> None:
    cli.mcp_install(["vscode", "vscode"])
    first = snapshot(hosts)

    report = cli.mcp_install(["vscode"])

    assert [(host.host, host.status) for host in report.hosts] == [("vscode", "unchanged")]
    assert snapshot(hosts) == first


def test_conflicting_entry_is_reported_and_left_alone(hosts: dict[str, Path]) -> None:
    hosts["copilot-cli"].write_text('{"mcpServers":{"wyrd":{"command":"someone-else"}}}')
    before = snapshot(hosts)

    report = cli.mcp_install(["copilot-cli", "vscode"])

    conflict, added = report.hosts
    assert (conflict.status, conflict.path) == ("conflict", hosts["copilot-cli"])
    assert conflict.detail is not None and "rename or remove it" in conflict.detail
    assert added.status == "added"
    assert hosts["copilot-cli"].read_text() == before["copilot-cli"]


def test_undetected_host_is_reported(hosts: dict[str, Path]) -> None:
    hosts["vscode"].unlink()
    hosts["vscode"].parent.rmdir()

    (report,) = cli.mcp_install(["vscode"]).hosts

    assert (report.status, report.path) == ("not_detected", None)
    assert not hosts["vscode"].parent.exists()


def test_external_server_preserves_global_endpoint(hosts: dict[str, Path]) -> None:
    config = Path(os.environ["WYRD_CONFIG_HOME"]) / "config.toml"
    config.parent.mkdir()
    config.write_text('[client]\nhttp_url = "http://127.0.0.1:9"\n')

    cli.mcp_install(["claude-code"], server="https://wyrd.example")

    entry = json.loads(hosts["claude-code"].read_text())["mcpServers"]["wyrd"]
    assert entry["args"] == ["mcp", "proxy", "--server", "https://wyrd.example"]
    assert config.read_text() == '[client]\nhttp_url = "http://127.0.0.1:9"\n'


@pytest.mark.parametrize(
    ("selection", "server", "code"),
    [
        ([], None, "WYRD_SPEC_400_VALIDATION"),
        (["cursor"], None, "WYRD_SPEC_400_VALIDATION"),
        (["codex"], "http://wyrd.example", "WYRD_CLIENT_400_CONFIG_INVALID"),
    ],
)
def test_invalid_install_changes_nothing(
    hosts: dict[str, Path], selection: list[str], server: str | None, code: str
) -> None:
    before = snapshot(hosts)

    with pytest.raises(WyrdError) as caught:
        cli.mcp_install(selection, server=server)

    assert caught.value.code == code
    assert snapshot(hosts) == before
