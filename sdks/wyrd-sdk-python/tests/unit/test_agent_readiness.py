"""The live agent-readiness lane's decisions, without agents, credentials, or a server.

Process doubles stand in for an agent and for ``wyrd init`` only to
force failures and deadlines; the real two-agent journey is
``mise run test:agent-readiness:smoke:live``.
"""

from __future__ import annotations

import os
import re
import sys
import threading
import time
from pathlib import Path

import pytest
import tomllib

from tests.agent_readiness.live import (
    AGENTS,
    Agent,
    Cancelled,
    Redactor,
    StageFailed,
    agent_environment,
    keep_login,
    preflight,
    run,
    run_agent,
    wyrd_served,
)
from tests.agent_readiness.observe import Evidence, Judgment

GRAPH = ["Agent", "Prompt", "Service", "Verifier"]


def judged(
    verdict: str = "passed", status: str = "completed", run_id: str | None = "r1"
) -> Judgment:
    return Judgment("e1", status, verdict, run_id)


def complete(**overrides: object) -> Evidence:
    """Evidence holding every milestone, with ``overrides`` replacing fields."""
    evidence = Evidence(
        service="Service/app@1.0.0",
        kinds=GRAPH,
        telemetry_runs=["r1"],
        custom_runs={"events": ["r1"]},
        sync=[judged()],
        queued=[judged()],
    )
    for name, value in overrides.items():
        setattr(evidence, name, value)
    return evidence


@pytest.mark.parametrize(
    ("overrides", "point"),
    [
        ({"service": None, "kinds": []}, "declared"),
        ({"kinds": ["Agent", "Service"]}, "declared"),
        ({"kinds": ["Service", "Verifier"]}, "declared"),
        ({"telemetry_runs": []}, "telemetry"),
        ({"custom_runs": {"events": []}}, "custom_rows"),
        ({"sync": []}, "sync_judgment"),
        ({"sync": [judged("inconclusive")]}, "sync_judgment"),
        ({"sync": [judged(run_id=None)]}, "sync_judgment"),
        # Two direct results never stand in for a queued one.
        ({"queued": [], "sync": [judged(), judged()]}, "async_judgment"),
        ({"queued": [Judgment("e2", "pending", None, None)]}, "async_judgment"),
        ({"queued": [Judgment("e2", "completed", None, None)]}, "async_judgment"),
        ({"queued": [judged(status="errored")]}, "async_judgment"),
        ({"queued": [judged("inconclusive")]}, "async_judgment"),
        (
            {"queued": [Judgment("e2", "WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND", None, None)]},
            "async_judgment",
        ),
        ({}, None),
        # A failed application verdict is still a working judgment.
        ({"sync": [judged("failed")], "queued": [judged("failed")]}, None),
    ],
)
def test_evidence_failure_point(overrides: dict[str, object], point: str | None) -> None:
    evidence = complete(**overrides)

    assert evidence.failure_point() == point
    assert evidence.report()["failure_point"] == point


def test_redactor_hides_credentials_and_wyrd_keys() -> None:
    redact = Redactor.from_environment(
        {"CODEX_API_KEY": "sk-codex-secret", "OTHER_TOKEN": "tok-12345678", "HOME": "/home/x"}
    )

    shown = redact("sk-codex-secret tok-12345678 wyrd_sk_0a1b_c2 wyrd_global_x9 /home/x")

    assert shown == "[redacted] [redacted] [redacted] [redacted] /home/x"


def test_preflight_names_every_missing_prerequisite(tmp_path: Path) -> None:
    missing = preflight(
        list(AGENTS.values()), {"CODEX_HOME": str(tmp_path)}, which=lambda binary: None
    )

    assert "test Postgres" in missing["harness"] and "pnpm" in missing["harness"]
    assert (
        missing["claude"]
        == "missing `claude` on PATH; ANTHROPIC_API_KEY or CLAUDE_CODE_OAUTH_TOKEN"
    )
    assert missing["codex"] == "missing `codex` on PATH; a `codex login` (auth.json)"


def test_missing_prerequisites_block_every_agent_instead_of_skipping(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    for agent in AGENTS.values():
        for name in agent.credentials:
            monkeypatch.delenv(name, raising=False)
    monkeypatch.setenv("CODEX_HOME", str(tmp_path / "logged-out"))

    outcomes = run(list(AGENTS.values()), tmp_path, 1.0, threading.Event())

    assert [(each.agent, each.outcome, each.failure_point) for each in outcomes] == [
        ("claude", "blocked", "preflight"),
        ("codex", "blocked", "preflight"),
    ]
    assert "API_KEY" in outcomes[0].detail and "codex login" in outcomes[1].detail


def test_one_agents_missing_prerequisite_leaves_the_other_running(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from tests.agent_readiness import live

    monkeypatch.setattr(live, "preflight", lambda agents, environ: {"codex": "missing codex"})
    monkeypatch.setattr(
        live,
        "exercise_all",
        lambda agents, *_: [live.Outcome(agent.name, outcome="passed") for agent in agents],
    )

    outcomes = run(list(AGENTS.values()), tmp_path, 1.0, threading.Event())

    assert [(each.agent, each.outcome, each.detail) for each in outcomes] == [
        ("claude", "passed", ""),
        ("codex", "blocked", "missing codex"),
    ]


def test_linked_login_replaced_by_the_agent_is_copied_back(tmp_path: Path) -> None:
    original = tmp_path / "owner" / "auth.json"
    original.parent.mkdir()
    original.write_text("old")
    link = tmp_path / "auth.json"
    link.symlink_to(original)

    assert keep_login(link, original) is None
    link.unlink()
    link.write_text("refreshed")

    assert keep_login(link, original) is not None
    assert original.read_text() == "refreshed"


def test_agent_environment_carries_only_its_own_credential(tmp_path: Path) -> None:
    owner = {
        "PATH": "/home/o/.local/share/mise/shims:/usr/bin",
        "ANTHROPIC_API_KEY": "sk-ant",
        "CODEX_API_KEY": "sk-codex",
        "WYRD_API_KEY": "wyrd_sk_owner",
        "LANG": "C.UTF-8",
    }
    agent = Agent("codex", "/opt/codex/bin/codex", "codex", ("CODEX_API_KEY",), ())

    env = agent_environment(
        agent, tmp_path, tmp_path / "wyrd", tmp_path / "bin", tmp_path / "w", owner
    )

    assert env["CODEX_API_KEY"] == "sk-codex"
    assert "ANTHROPIC_API_KEY" not in env and "WYRD_API_KEY" not in env
    assert env["PATH"].split(os.pathsep) == [str(tmp_path / "bin"), "/opt/codex/bin", "/usr/bin"]
    assert env["HOME"] == str(tmp_path) and env["WYRD_CONFIG_HOME"] == str(tmp_path / "wyrd")


def double(tmp_path: Path, name: str, body: str) -> Path:
    """An executable Python process double named ``name``."""
    script = tmp_path / name
    script.write_text(f"#!{sys.executable}\nimport os, signal, subprocess, sys, time\n{body}\n")
    script.chmod(0o755)
    return script


def alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def agent_double(tmp_path: Path, body: str) -> Agent:
    return Agent("double", str(double(tmp_path, "agent", body)), "codex", (), ())


def test_agent_exit_status_is_reported_with_a_redacted_transcript(tmp_path: Path) -> None:
    agent = agent_double(
        tmp_path, "print(sys.stdin.read(), 'sk-agent-secret wyrd_sk_abc'); sys.exit(3)"
    )
    transcript = tmp_path / "transcript.jsonl"

    status = run_agent(
        agent,
        "objective",
        tmp_path,
        dict(os.environ),
        transcript,
        30,
        Redactor(["sk-agent-secret"]),
        threading.Event(),
    )

    assert status == 3
    assert transcript.read_text() == "objective [redacted] [redacted]\n"


def test_agent_past_its_deadline_is_settled_with_its_children(tmp_path: Path) -> None:
    pids = tmp_path / "pids"
    agent = agent_double(
        tmp_path,
        "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(600)'])\n"
        f"open({str(pids)!r}, 'w').write(str(child.pid))\n"
        "signal.signal(signal.SIGINT, signal.SIG_IGN)\ntime.sleep(600)",
    )

    status = run_agent(
        agent, "", tmp_path, dict(os.environ), tmp_path / "t", 2, Redactor(), threading.Event()
    )

    assert status is None
    grandchild = int(pids.read_text())
    deadline = time.monotonic() + 30
    while alive(grandchild) and time.monotonic() < deadline:
        time.sleep(0.1)
    assert not alive(grandchild)


def test_cancellation_settles_the_agent(tmp_path: Path) -> None:
    pids = tmp_path / "pid"
    agent = agent_double(
        tmp_path, f"open({str(pids)!r}, 'w').write(str(os.getpid()))\ntime.sleep(600)"
    )
    cancel = threading.Event()
    threading.Timer(1.0, cancel.set).start()

    with pytest.raises(Cancelled):
        run_agent(agent, "", tmp_path, dict(os.environ), tmp_path / "t", 600, Redactor(), cancel)

    assert not alive(int(pids.read_text()))


def test_server_failure_before_readiness_stops_the_stage(tmp_path: Path) -> None:
    cli = double(
        tmp_path, "wyrd", "sys.stderr.write('startup failed wyrd_sk_leak\\n'); sys.exit(2)"
    )

    with pytest.raises(StageFailed) as failed:
        with wyrd_served(
            cli, "t", tmp_path, dict(os.environ), tmp_path / "server.log", threading.Event()
        ):
            raise AssertionError("an agent must never start without readiness")

    assert failed.value.stage == "server"
    assert failed.value.detail == "startup failed [redacted]"


def test_ready_server_is_interrupted_when_the_block_ends(tmp_path: Path) -> None:
    pids = tmp_path / "pid"
    cli = double(
        tmp_path,
        "wyrd",
        f"open({str(pids)!r}, 'w').write(str(os.getpid()))\n"
        'print(\'{"ready": true, "endpoint": "http://127.0.0.1:1"}\', flush=True)\n'
        "signal.signal(signal.SIGINT, lambda *_: sys.exit(130))\ntime.sleep(600)",
    )

    with wyrd_served(
        cli, "t", tmp_path, dict(os.environ), tmp_path / "log", threading.Event()
    ) as ready:
        assert ready["endpoint"] == "http://127.0.0.1:1"
        assert alive(int(pids.read_text()))

    assert not alive(int(pids.read_text()))


def test_no_gate_or_ci_path_reaches_the_live_lane() -> None:
    """Walk every task ``gate`` and CI reach, through ``depends`` and nested ``mise run``."""
    root = Path(__file__).resolve().parents[4]
    tasks = tomllib.loads((root / "mise.toml").read_text())["tasks"]
    workflows = "\n".join(path.read_text() for path in (root / ".github").rglob("*.y*ml"))
    assert "agent-readiness" not in workflows and "agent_readiness" not in workflows
    nested = re.compile(r"mise run ([\w:.-]+)")
    pending = {"gate", *nested.findall(workflows)}
    commands: dict[str, str] = {}
    while pending:
        name = pending.pop()
        if name in commands or name not in tasks:
            continue
        run = tasks[name].get("run", "")
        commands[name] = run if isinstance(run, str) else "\n".join(run)
        pending.update(tasks[name].get("depends", []), nested.findall(commands[name]))

    assert "gate" in commands and len(commands) > 20
    reached = [name for name, command in commands.items() if "readiness" in name + command]
    assert reached == []
