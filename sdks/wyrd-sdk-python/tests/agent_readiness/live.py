"""Ask Claude Code and Codex to build a Wyrd-verified application, and judge each from server evidence.

``mise run test:agent-readiness:smoke:live`` runs this once the current CLI,
server, and docs dependencies are built and the repository's test Postgres is
up. It is opt-in and outside ``gate`` and CI: it calls paid agents.

The harness builds this checkout's Python wheel and serves this checkout's
docs. Each agent then gets its own deployment: an empty database, an isolated
Wyrd configuration, a foreground ``wyrd server dev`` the harness owns and
waits on, its host connected through ``wyrd mcp install``, and a fresh
application directory and home. Both agents receive the same objective and
run concurrently. ``observe`` then judges each agent from its own server;
the transcript, exit status, and files are diagnostics only.

Claude Code's credential comes from the environment, normally an untracked
``mise.local.toml``: ``ANTHROPIC_API_KEY`` or ``CLAUDE_CODE_OAUTH_TOKEN``
(``claude setup-token``). Codex uses the owner's existing login: its empty
``CODEX_HOME`` links only ``auth.json`` from the owner's, so no instructions,
plugins, memories, or configuration come along. An agent never sees another's
credential.
Every artifact lands under ``target/agent-readiness/<UTC time>/``, with
``report.json`` holding each agent's outcome, failure point, and evidence IDs.
Exit status is 0 only when both agents pass.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time
from collections.abc import Iterator, Mapping, Sequence
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from dataclasses import asdict, dataclass, field, replace
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, TextIO

import httpx

from ..integration.test_local_development import local_deployment, stop_local_cli

ROOT = Path(__file__).resolve().parents[4]
"""The repository checkout under test."""

OBJECTIVE = """\
This directory is empty and is yours. Build and run a small Python application
integrated with Wyrd, the AI verification server, so that it:

1. declares to Wyrd a Service for the application with its components and at
   least one Verifier bound to them;
2. records what it does as correlated Run observations: OpenTelemetry
   telemetry and rows in a custom dataset table the Service declares; and
3. obtains one synchronous verification judgment and one asynchronous
   (queued) verification judgment, reading the queued one back until it has
   a terminal result.

A Wyrd server is already running and this machine's saved Wyrd settings point
to it. Do not start, stop, or reconfigure it. The `wyrd` CLI is on PATH and the
Wyrd MCP server is connected. pip and uv resolve the `wyrd-ai` package to the
version this server needs.

Wyrd's documentation is served at {docs}; start from {docs}llms.txt. Use only
that documentation, the Wyrd MCP server, and ordinary package tooling; do not
use any other copy of the Wyrd documentation. No model-provider credential is
available, so the application may answer deterministically without calling a
model.

Work without asking questions. You are done when the application has run and
its evidence is on the server; then summarize what you built.
"""
"""The one bounded objective both agents get, with ``{docs}`` the local docs base."""

SECRET_PATTERN = re.compile(r"wyrd_(?:sk|global)_[A-Za-z0-9_]+")
"""Wyrd keys that must never reach a transcript or report."""


class Cancelled(Exception):
    """The owner interrupted the run; every child still settles."""


class StageFailed(Exception):
    """A harness stage failed; the message is already redacted."""

    def __init__(self, stage: str, detail: str) -> None:
        super().__init__(f"{stage}: {detail}")
        self.stage = stage
        self.detail = detail


@dataclass(frozen=True)
class Agent:
    """One coding agent: how to find, authenticate, connect, and invoke it."""

    name: str
    binary: str
    host: str
    credentials: tuple[str, ...]
    command: tuple[str, ...]
    login: str | None = None
    """A login file shared by link from the owner's ``CODEX_HOME``, in place of ``credentials``."""


AGENTS = {
    "claude": Agent(
        name="claude",
        binary="claude",
        host="claude-code",
        credentials=("ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN"),
        # The objective arrives on stdin; web search would reach other docs.
        command=(
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-mode",
            "bypassPermissions",
            "--disallowedTools",
            "WebSearch",
        ),
    ),
    "codex": Agent(
        name="codex",
        binary="codex",
        host="codex",
        credentials=(),
        login="auth.json",
        command=(
            "exec",
            "--json",
            "--dangerously-bypass-approvals-and-sandbox",
            "--skip-git-repo-check",
            "-",
        ),
    ),
}
"""Every agent this lane runs, by name."""


@dataclass
class Outcome:
    """One agent's result, as ``report.json`` records it."""

    agent: str
    outcome: str = "failed"
    failure_point: str | None = None
    detail: str = ""
    version: str | None = None
    transcript: str | None = None
    app: str | None = None
    evidence: dict[str, Any] | None = None
    notes: list[str] = field(default_factory=list)


@dataclass
class Redactor:
    """Replaces every known secret value and Wyrd key pattern in text."""

    secrets: list[str] = field(default_factory=list)

    @classmethod
    def from_environment(cls, environ: Mapping[str, str]) -> Redactor:
        """Collect every credential-shaped environment value worth hiding."""
        names = {name for agent in AGENTS.values() for name in agent.credentials}
        secrets = [
            value
            for name, value in environ.items()
            if len(value) >= 8 and (name in names or name.endswith(("_KEY", "_TOKEN", "_SECRET")))
        ]
        secrets.sort(key=len, reverse=True)
        return cls(secrets)

    def __call__(self, text: str) -> str:
        """``text`` with every secret replaced by ``[redacted]``."""
        for secret in self.secrets:
            text = text.replace(secret, "[redacted]")
        return SECRET_PATTERN.sub("[redacted]", text)


def preflight(
    agents: Sequence[Agent], environ: Mapping[str, str], which: Any = shutil.which
) -> dict[str, str]:
    """Every missing prerequisite by owner: an agent name or ``harness``.

    The harness needs the built CLI and server, pnpm, and the repository
    test Postgres; each agent needs its binary and one credential variable, or
    for Codex the owner's existing login.
    Nothing is skipped: a missing prerequisite is a blocked outcome.
    """
    missing: dict[str, str] = {}
    harness = [
        str(path.relative_to(ROOT))
        for path in (ROOT / "target/debug/wyrd", ROOT / "target/debug/wyrd-server")
        if not path.is_file()
    ]
    if which("pnpm") is None:
        harness.append("pnpm")
    if "WYRD_TEST_DATABASE_ADMIN_URL" not in environ:
        harness.append("test Postgres (run through scripts/postgres/with-test-postgres.sh)")
    if harness:
        missing["harness"] = "missing " + ", ".join(harness)
    for agent in agents:
        problems = []
        if resolve(agent.binary, which) is None:
            problems.append(f"`{agent.binary}` on PATH")
        if agent.login is not None:
            if not (owner_codex_home(environ) / agent.login).is_file():
                problems.append(f"a `{agent.binary} login` ({agent.login})")
        elif not any(environ.get(name) for name in agent.credentials):
            problems.append(" or ".join(agent.credentials))
        if problems:
            missing[agent.name] = "missing " + "; ".join(problems)
    return missing


def owner_codex_home(environ: Mapping[str, str]) -> Path:
    """The owner's own ``CODEX_HOME``, where an existing Codex login lives."""
    return Path(environ.get("CODEX_HOME") or Path(environ.get("HOME", "~")).expanduser() / ".codex")


def login_secrets(login: Path) -> list[str]:
    """Every long string value in a JSON login file, for redaction."""
    found: list[str] = []
    pending: list[Any] = [json.loads(login.read_text())]
    while pending:
        value = pending.pop()
        if isinstance(value, dict):
            pending.extend(value.values())
        elif isinstance(value, list):
            pending.extend(value)
        elif isinstance(value, str) and len(value) >= 20:
            found.append(value)
    return found


def keep_login(link: Path, original: Path) -> str | None:
    """Keep the owner's login valid after an agent ran on a linked copy.

    Codex refreshing its token through the link keeps one file. If it
    replaced the link with a refreshed file instead, the owner's original may
    hold a rotated-out token, so the refreshed file is copied back over it.
    Returns a note when that happened.
    """
    if link.is_symlink() or not link.is_file():
        return None
    shutil.copy2(link, original)
    return f"the agent replaced its linked login; the refreshed one was copied back to {original}"


@contextmanager
def shared_login(agent: Agent, codex_home: Path, notes: list[str]) -> Iterator[list[str]]:
    """Link the owner's login into the agent's ``CODEX_HOME`` for the block.

    Yields the login's secret values for redaction; afterwards ``keep_login``
    guards the owner's file and any note lands in ``notes``.
    """
    if agent.login is None:
        yield []
        return
    original = owner_codex_home(os.environ) / agent.login
    link = codex_home / agent.login
    link.symlink_to(original)
    try:
        yield login_secrets(original)
    finally:
        if note := keep_login(link, original):
            notes.append(note)


def free_port() -> int:
    """A loopback port the OS just reported free."""
    with socket.socket() as bound:
        bound.bind(("127.0.0.1", 0))
        return bound.getsockname()[1]


def settle(child: subprocess.Popen[Any], grace: float = 20.0) -> None:
    """Interrupt ``child``'s process group, then kill it if it outlives ``grace``."""
    if child.poll() is not None:
        return
    for sent in (signal.SIGINT, signal.SIGKILL):
        try:
            os.killpg(child.pid, sent)
        except ProcessLookupError:
            return
        try:
            child.wait(timeout=grace)
            return
        except subprocess.TimeoutExpired:
            continue


def tail(path: Path, redact: Redactor, lines: int = 20) -> str:
    """The last ``lines`` of ``path``, redacted, for a failure detail."""
    if not path.is_file():
        return ""
    return redact("\n".join(path.read_text(errors="replace").splitlines()[-lines:]))


def wait_until(
    ready: Any, child: subprocess.Popen[Any], deadline: float, cancel: threading.Event
) -> bool:
    """Poll ``ready`` until it holds; ``False`` once ``child`` exits or ``deadline`` passes.

    Raises:
        Cancelled: when ``cancel`` is set first.
    """
    while time.monotonic() < deadline:
        if cancel.is_set():
            raise Cancelled
        if ready():
            return True
        if child.poll() is not None:
            return False
        time.sleep(1)
    return False


@contextmanager
def docs_served(artifacts: Path, redact: Redactor, cancel: threading.Event) -> Iterator[str]:
    """Build and serve this checkout's docs through ``docs:preview``; yield their base URL.

    Readiness is the built ``llms-full.txt`` answering. The serving process
    group settles when the block ends.

    Raises:
        StageFailed: ``docs`` when the build or server fails or is not ready
            within 15 minutes.
    """
    port = free_port()
    base = f"http://127.0.0.1:{port}/wyrd/"
    log = artifacts / "docs.log"
    with log.open("w") as written:
        child = subprocess.Popen(
            ["mise", "run", "docs:preview"],
            cwd=ROOT,
            env={**os.environ, "WYRD_DOCS_PREVIEW_PORT": str(port)},
            stdin=subprocess.DEVNULL,
            stdout=written,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )

    def answering() -> bool:
        try:
            return httpx.get(f"{base}llms-full.txt", timeout=5).status_code == 200
        except httpx.HTTPError:
            return False

    try:
        if not wait_until(answering, child, time.monotonic() + 900, cancel):
            raise StageFailed("docs", tail(log, redact))
        yield base
    finally:
        settle(child)


@contextmanager
def wyrd_served(
    cli: Path, tenant: str, home: Path, env: dict[str, str], log: Path, cancel: threading.Event
) -> Iterator[dict[str, Any]]:
    """Run ``wyrd server dev --format json`` in the foreground; yield its readiness document.

    The harness owns the child: it waits for the one readiness line and stops
    it with an interrupt when the block ends, exactly as a person would.

    Raises:
        StageFailed: ``server`` when the CLI exits, reports an error, or is
            not ready within five minutes.
    """
    with log.open("w") as diagnostics:
        child = subprocess.Popen(
            [str(cli), "server", "dev", "--tenant", tenant, "--non-interactive"]
            + ["--skip-gateway", "--format", "json"],
            cwd=home,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=diagnostics,
            text=True,
            start_new_session=True,
        )
    assert child.stdout is not None
    lines: list[str] = []
    reader = threading.Thread(target=lambda: lines.extend(child.stdout or ()), daemon=True)
    reader.start()
    try:
        if not wait_until(lambda: bool(lines), child, time.monotonic() + 300, cancel):
            raise StageFailed("server", tail(log, Redactor()))
        readiness = json.loads(lines[0])
        if readiness.get("ready") is not True:
            raise StageFailed("server", "readiness document without ready: true")
        yield readiness
    finally:
        try:
            stop_local_cli(child)
        except (AssertionError, RuntimeError) as error:
            print(f"{tenant}: server did not settle cleanly: {error}", file=sys.stderr)


def run_agent(
    agent: Agent,
    objective: str,
    app: Path,
    env: dict[str, str],
    transcript: Path,
    timeout: float,
    redact: Redactor,
    cancel: threading.Event,
) -> int | None:
    """Run ``agent`` non-interactively in ``app``; its exit status, or ``None`` past ``timeout``.

    Its combined output streams through ``redact`` into ``transcript``. The
    agent's process group is settled on return, deadline, or cancellation.

    Raises:
        Cancelled: when ``cancel`` is set first.
    """
    child = subprocess.Popen(
        [agent.binary, *agent.command],
        cwd=app,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        start_new_session=True,
    )
    assert child.stdin is not None and child.stdout is not None

    def copy(source: TextIO) -> None:
        with transcript.open("w") as written:
            for line in source:
                written.write(redact(line))
                written.flush()

    copier = threading.Thread(target=copy, args=(child.stdout,), daemon=True)
    copier.start()
    try:
        child.stdin.write(objective)
        child.stdin.close()
        deadline = time.monotonic() + timeout
        while child.poll() is None:
            if cancel.is_set():
                raise Cancelled
            if time.monotonic() >= deadline:
                return None
            time.sleep(1)
        copier.join(timeout=10)
        return child.returncode
    finally:
        settle(child, grace=10)


def resolve(binary: str, which: Any = shutil.which) -> str | None:
    """``binary``'s real path; a mise shim resolves through ``mise which``.

    A shim would otherwise resolve against the agent's isolated home.
    """
    found = which(binary)
    if found is None or "/mise/shims/" not in found:
        return found
    real = subprocess.run(["mise", "which", binary], capture_output=True, text=True)
    if real.returncode == 0 and real.stdout.strip():
        return real.stdout.strip()
    # A shim inactive here still leaves any real install later on PATH.
    path = os.pathsep.join(
        entry
        for entry in os.environ.get("PATH", "").split(os.pathsep)
        if "/mise/shims" not in entry
    )
    return which(binary, path=path)


def agent_environment(
    agent: Agent, home: Path, config: Path, bin_dir: Path, wheels: Path, environ: Mapping[str, str]
) -> dict[str, str]:
    """The agent's whole environment: isolated home, its own credential, and the local build.

    Nothing else of the owner's environment is inherited, so no other
    secret, Wyrd setting, or host configuration reaches the agent. ``PATH``
    keeps real tool directories but not mise shims, which would resolve
    against the owner's home.
    """
    path = [str(bin_dir), str(Path(agent.binary).parent)] + [
        entry for entry in environ.get("PATH", "").split(os.pathsep) if "/mise/shims" not in entry
    ]
    env = {
        "HOME": str(home),
        "CODEX_HOME": str(home / "codex"),
        "WYRD_CONFIG_HOME": str(config),
        "PATH": os.pathsep.join(path),
        "PIP_FIND_LINKS": str(wheels),
        "UV_FIND_LINKS": str(wheels),
        "TERM": "dumb",
    }
    for name in ("LANG", "LC_ALL", "TMPDIR", "SSL_CERT_FILE", "SSL_CERT_DIR"):
        if name in environ:
            env[name] = environ[name]
    for credential in agent.credentials:
        if environ.get(credential):
            env[credential] = environ[credential]
            break
    return env


def exercise(
    agent: Agent,
    docs: str,
    wheels: Path,
    artifacts: Path,
    timeout: float,
    redact: Redactor,
    cancel: threading.Event,
) -> Outcome:
    """Run one agent end to end on its own deployment and judge it from server evidence."""
    outcome = Outcome(agent=agent.name)
    agent = replace(agent, binary=resolve(agent.binary) or agent.binary)
    workdir = artifacts / agent.name
    app = workdir / "app"
    app.mkdir(parents=True)
    outcome.app = str(app)
    outcome.transcript = str(workdir / "transcript.jsonl")
    cli = ROOT / "target/debug/wyrd"
    bin_dir = workdir / "bin"
    bin_dir.mkdir()
    (bin_dir / "wyrd").symlink_to(cli)
    try:
        outcome.version = subprocess.run(
            [agent.binary, "--version"], capture_output=True, text=True, timeout=60
        ).stdout.strip()
        with local_deployment() as (home, server_env):
            config = home / "wyrd"
            (home / ".claude").mkdir()
            server_env = {**server_env, "HOME": str(home), "CODEX_HOME": str(home / "codex")}
            with (
                shared_login(agent, home / "codex", outcome.notes) as login,
                wyrd_served(cli, agent.name, home, server_env, workdir / "server.log", cancel),
            ):
                redact = Redactor([*login, *redact.secrets])
                env = agent_environment(agent, home, config, bin_dir, wheels, os.environ)
                installed = subprocess.run(
                    [str(cli), "mcp", "install", "--host", agent.host, "--format", "json"],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=60,
                )
                (workdir / "mcp-install.json").write_text(
                    redact(installed.stdout + installed.stderr)
                )
                if installed.returncode != 0 or json.loads(installed.stdout)["hosts"][0][
                    "status"
                ] not in ("added", "updated", "unchanged"):
                    raise StageFailed("mcp", redact(installed.stdout + installed.stderr))
                status = run_agent(
                    agent,
                    OBJECTIVE.format(docs=docs),
                    app,
                    env,
                    Path(outcome.transcript),
                    timeout,
                    redact,
                    cancel,
                )
                observed = subprocess.run(
                    [sys.executable, "-m", "tests.agent_readiness.observe"],
                    cwd=ROOT / "sdks/wyrd-sdk-python",
                    env={**server_env, "WYRD_CONFIG_HOME": str(config)},
                    capture_output=True,
                    text=True,
                    timeout=600,
                )
                (workdir / "observer.log").write_text(redact(observed.stderr))
                try:
                    outcome.evidence = json.loads(observed.stdout)
                except json.JSONDecodeError as error:
                    raise StageFailed("evidence", tail(workdir / "observer.log", redact)) from error
        point = outcome.evidence["failure_point"]
        if status != 0:
            ended = "timed out" if status is None else f"exited {status}"
            outcome.failure_point = "agent"
            outcome.detail = f"agent {ended}; evidence stopped at {point or 'none'}"
        elif point is not None:
            outcome.failure_point = point
            outcome.detail = f"no server evidence for {point}"
        else:
            outcome.outcome = "passed"
    except StageFailed as failed:
        outcome.failure_point, outcome.detail = failed.stage, failed.detail
    except Cancelled:
        outcome.outcome, outcome.failure_point = "cancelled", "cancelled"
    except Exception as error:  # noqa: BLE001 - one agent's harness error must not hide the other's outcome
        outcome.failure_point, outcome.detail = (
            "harness",
            redact(f"{type(error).__name__}: {error}"),
        )
    return outcome


def build_wheel(wheels: Path) -> None:
    """Build this checkout's production ``wyrd-ai`` wheel into ``wheels``.

    Raises:
        StageFailed: ``wheel`` when the build fails.
    """
    built = subprocess.run(
        ["uv", "run", "maturin", "build", "--out", str(wheels)],
        cwd=ROOT / "sdks/wyrd-sdk-python",
        capture_output=True,
        text=True,
    )
    if built.returncode != 0:
        raise StageFailed("wheel", "\n".join(built.stderr.splitlines()[-20:]))


def identity() -> dict[str, str]:
    """The checkout and binaries a report describes."""
    commit = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True
    ).stdout.strip()
    dirty = subprocess.run(
        ["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True
    ).stdout.strip()
    version = subprocess.run(
        [str(ROOT / "target/debug/wyrd"), "--version"], capture_output=True, text=True
    ).stdout.strip()
    return {"commit": commit, "dirty": str(bool(dirty)).lower(), "wyrd": version}


def run(
    agents: Sequence[Agent], artifacts: Path, timeout: float, cancel: threading.Event
) -> list[Outcome]:
    """Prepare the shared wheel and docs, then exercise every ready agent concurrently.

    A missing prerequisite blocks only the agents it affects (all of them for
    a harness one); a failed shared stage fails every exercised agent with
    its reason. Nothing is skipped.
    """
    redact = Redactor.from_environment(os.environ)
    missing = preflight(agents, os.environ)
    outcomes = {
        agent.name: Outcome(
            agent=agent.name,
            outcome="blocked",
            failure_point="preflight",
            detail="; ".join(v for k, v in missing.items() if k in ("harness", agent.name)),
        )
        for agent in agents
        if "harness" in missing or agent.name in missing
    }
    ready = [agent for agent in agents if agent.name not in outcomes]
    if ready:
        outcomes |= {
            outcome.agent: outcome
            for outcome in exercise_all(ready, artifacts, timeout, redact, cancel)
        }
    return [outcomes[agent.name] for agent in agents]


def exercise_all(
    agents: Sequence[Agent],
    artifacts: Path,
    timeout: float,
    redact: Redactor,
    cancel: threading.Event,
) -> list[Outcome]:
    """Build the shared wheel, serve the docs, and exercise ``agents`` concurrently."""
    try:
        build_wheel(artifacts / "wheels")
        with (
            docs_served(artifacts, redact, cancel) as docs,
            ThreadPoolExecutor(len(agents)) as pool,
        ):
            futures = [
                pool.submit(
                    exercise, agent, docs, artifacts / "wheels", artifacts, timeout, redact, cancel
                )
                for agent in agents
            ]
            return [future.result() for future in futures]
    except StageFailed as failed:
        return [
            Outcome(agent=agent.name, failure_point=failed.stage, detail=failed.detail)
            for agent in agents
        ]
    except Cancelled:
        return [
            Outcome(agent=agent.name, outcome="cancelled", failure_point="cancelled")
            for agent in agents
        ]


def main(argv: Sequence[str] | None = None) -> int:
    """Run the lane, write ``report.json``, print a summary, and exit 0 only when every agent passed."""
    parser = argparse.ArgumentParser(description="Run the live coding-agent readiness lane.")
    parser.add_argument("--agent", action="append", choices=sorted(AGENTS), dest="agents")
    parser.add_argument("--timeout", type=float, default=3600.0, help="seconds per agent")
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args(argv)
    agents = [AGENTS[name] for name in args.agents or sorted(AGENTS)]
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    artifacts = (args.artifacts or ROOT / "target/agent-readiness" / stamp).resolve()
    artifacts.mkdir(parents=True)

    cancel = threading.Event()

    def interrupted(signum: int, frame: object) -> None:
        cancel.set()

    signal.signal(signal.SIGINT, interrupted)
    signal.signal(signal.SIGTERM, interrupted)
    outcomes = run(agents, artifacts, args.timeout, cancel)
    report = {"identity": identity(), "outcomes": [asdict(outcome) for outcome in outcomes]}
    (artifacts / "report.json").write_text(json.dumps(report, indent=2))
    for outcome in outcomes:
        print(
            f"{outcome.agent}: {outcome.outcome}"
            + (f" at {outcome.failure_point}: {outcome.detail}" if outcome.failure_point else "")
        )
    print(f"report: {artifacts / 'report.json'}")
    if cancel.is_set():
        return 130
    return 0 if all(outcome.outcome == "passed" for outcome in outcomes) else 1


if __name__ == "__main__":
    raise SystemExit(main())
