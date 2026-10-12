"""A developer runs Wyrd locally with only the administrator key ``wyrd setup`` printed.

They register and hydrate an assistant, invoke a model through the Gateway with
the stock OpenAI client, observe and verify a Run, export its spans through
``state.start_telemetry``, and query the evidence back. No key is issued and
nothing is flushed on the server's behalf. ``work`` is that whole workflow for
one client, so the signed-in story proves the same steps for a saved login.
"""

from __future__ import annotations

import fcntl
import os
import pty
import select
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import termios
import threading
import time
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from http.server import ThreadingHTTPServer
from pathlib import Path

import openai
import pytest
from opentelemetry import trace
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.errors import WyrdError
from wyrd.gateway import Gateway, GatewayAuth
from wyrd.observe import Run
from wyrd.state import WyrdState
from wyrd.testing import UnmigratedDatabase, WyrdTestServer, cli

from .gateway.support import Upstream
from .support import Receiver, download, hydrated, register

pytestmark = pytest.mark.integration


class Attributed(BaseModel):
    """One evidence row: the Card it is attributed to."""

    card_uid: str | None


@dataclass
class Worked:
    """What ``work`` leaves running for a caller to keep using."""

    state: WyrdState
    agent_uid: str


def local_server(
    human_sso: bool = False, access_ttl_seconds: int | None = None
) -> Iterator[WyrdTestServer]:
    """A server that verifies in real time and routes the Gateway to a local upstream answering ``hi``."""
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    upstream.RequestHandlerClass = type("LocalUpstream", (Upstream,), {"upstream": recorded})
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    try:
        with WyrdTestServer(
            mutate_env=False,
            verification_runtime=True,
            provider_base_url=recorded.url,
            human_sso=human_sso,
            access_ttl_seconds=access_ttl_seconds,
        ) as server:
            yield server
    finally:
        upstream.shutdown()


def work(client: WyrdClient, bundle_home: Path) -> Worked:
    """Run the whole local workflow as ``client`` and assert each step."""
    configure_gateway(client)
    cards = Cards(client)
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    register(cards, "cards/verify_in_real_time/assistant.yaml")
    state = hydrated(download(cards, "assistant", bundle_home), client)
    state.start_bifrost()

    assert invoke(client) == "hi"

    run = state.run()
    run.for_card("model").observe.drift({"latency": 12.5})
    judgment = run.for_card("agent").observe.verify("answer-is-yes", {"answer": "yes"})
    assert (judgment.passed, judgment.kind) == (True, "eval_assertion")

    state.start_telemetry()
    state.start_telemetry()
    export(run.for_card("agent"))
    # Shutdown flushes the exported span.
    state.shutdown()

    bifrost = Bifrost(client=client)
    for table, alias in (("vala.drift.observations", "model"), ("vala.traces.spans", "agent")):
        rows = bifrost.sql(
            f"SELECT card_uid FROM {table} WHERE run_id = $1", [run.run_id], model=Attributed
        )
        assert rows == [Attributed(card_uid=str(state.card_ref(alias).uid))], table
    return Worked(state, str(state.card_ref("agent").uid))


def export(run: Run) -> None:
    """Start one ``answer`` span inside ``run``'s scope, which attributes it to that Run and Card."""
    with run, trace.get_tracer("wyrd.tests.local").start_as_current_span("answer"):
        pass


def configure_gateway(client: WyrdClient) -> None:
    """Write the ``openai-key`` credential and deploy ``gpt-4o`` on it as ``client``."""
    cli.put_provider_credential(
        {
            "name": "openai-key",
            "provider": "openai",
            "source": {"managed_secret": {"secret": "sk-local-upstream"}},
        },
        client=client,
    )
    Gateway(client).put_deployment(
        {
            "name": "gpt-4o",
            "model": {"provider": "openai", "model": "gpt-4o"},
            "adapter": "openai",
            "auth": {"bearer": {"credential": "openai-key"}},
            "capabilities": ["chat_completions"],
            "routing_weight": 1,
        }
    )


def invoke(client: WyrdClient) -> str:
    """Send one Chat Completions turn through the Gateway with the stock OpenAI client."""
    gateway = openai.OpenAI(
        base_url=f"{client.server_url}/v1",
        api_key="wyrd",
        http_client=openai.DefaultHttpxClient(auth=GatewayAuth(client)),
        max_retries=0,
    )
    completion = gateway.chat.completions.create(
        model="openai/gpt-4o", messages=[{"role": "user", "content": "hi"}]
    )
    return completion.choices[0].message.content or ""


@pytest.fixture(scope="module")
def server() -> Iterator[WyrdTestServer]:
    """The local deployment."""
    yield from local_server()


@pytest.mark.usefixtures("fresh_tracer_provider")
def test_admin_key_completes_the_local_workflow(server: WyrdTestServer, tmp_path: Path) -> None:
    client = WyrdClient(
        server_url=server.base_url, credential=server.tenant_admin_key(), grpc_url=server.grpc_url
    )

    work(client, tmp_path)


@pytest.mark.usefixtures("fresh_tracer_provider")
def test_start_telemetry_refuses_the_applications_own_provider(
    server: WyrdTestServer, tmp_path: Path
) -> None:
    from opentelemetry.sdk.trace import TracerProvider

    client = WyrdClient(
        server_url=server.base_url, credential=server.tenant_admin_key(), grpc_url=server.grpc_url
    )
    cards = Cards(client)
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    register(cards, "cards/verify_in_real_time/assistant.yaml")
    state = hydrated(download(cards, "assistant", tmp_path), client)
    own = TracerProvider()
    trace.set_tracer_provider(own)

    with pytest.raises(WyrdError) as refused:
        state.start_telemetry()

    assert refused.value.code == "WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS"
    assert trace.get_tracer_provider() is own


@pytest.fixture
def cli_bootstrap_client(monkeypatch: pytest.MonkeyPatch) -> WyrdClient:
    """The saved deployment made by the installed-CLI integration fixture."""
    import os

    home = os.environ.get("WYRD_LOCAL_DEV_CONFIG_HOME")
    if home is None:
        raise pytest.skip.Exception("requires the installed CLI local-development fixture")
    monkeypatch.setenv("WYRD_CONFIG_HOME", home)
    for name in (
        "WYRD_API_KEY",
        "WYRD_ACCESS_TOKEN",
        "WYRD_WORKLOAD_TOKEN",
        "WYRD_TENANT",
        "WYRD_SERVER_URL",
        "WYRD_GRPC_URL",
    ):
        monkeypatch.delenv(name, raising=False)
    return WyrdClient()


def test_cli_bootstrap_saved_config(cli_bootstrap_client: WyrdClient) -> None:
    """A fresh Python runtime writes and reads using only the CLI's saved files."""
    cards = Cards(cli_bootstrap_client)
    refs = register(cards, "cards/gateway_inference/ask-prompt.yaml")
    assert cards.get(refs["ask-prompt"]).metadata.name == "ask-prompt"


def local_listener() -> str:
    """Ask the OS for a loopback port for an isolated server listener."""
    with socket.socket() as bound:
        bound.bind(("127.0.0.1", 0))
        return f"127.0.0.1:{bound.getsockname()[1]}"


def terminal_session() -> None:
    """Give the fixture CLI its own controlling terminal before exec."""
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def wait_for_local_cli(child: subprocess.Popen[str], master: int, terminal: str, bind: str) -> None:
    """Assert readiness, or the first-setup offer's outcome, without exposing credentials.

    ``decline`` answers the offer, ``interrupt`` interrupts the CLI at it, and
    ``exit`` terminates only the serving child while it is unanswered.
    ``restart`` requires readiness with no offer.
    """
    assert child.stdout is not None
    if not terminal:
        for line in child.stdout:
            assert "wyrd_sk_" not in line and "wyrd_global_" not in line
            if line.startswith("Local Wyrd ready at "):
                return
        raise AssertionError("local CLI did not become ready")
    shown = b""
    disclosed = b""
    asked = False
    deadline = time.monotonic() + 150
    while b"Local Wyrd ready at " not in shown:
        assert time.monotonic() < deadline, "terminal startup deadline"
        readable, _, _ = select.select([child.stdout, master], [], [], 1)
        for source in readable:
            descriptor = master if source == master else child.stdout.fileno()
            try:
                chunk = os.read(descriptor, 4096)
            except OSError:
                if source != master:
                    raise
                chunk = b""
            if source == master:
                disclosed += chunk
                assert b"wyrd_sk_" not in disclosed, "tenant key in terminal"
                continue
            shown += chunk
            assert b"wyrd_sk_" not in shown and b"wyrd_global_" not in shown
            if not asked and b"Connect which hosts" in shown:
                asked = True
                assert terminal != "restart", "completed local setup asked again"
                if terminal == "decline":
                    os.write(master, b"\n")
                elif terminal == "interrupt":
                    child.send_signal(signal.SIGINT)
                else:
                    serving = subprocess.check_output(["pgrep", "-P", str(child.pid)], text=True)
                    for pid in serving.split():
                        os.kill(int(pid), signal.SIGTERM)
            if not chunk:
                assert terminal in ("interrupt", "exit") and asked
                code = child.wait(timeout=50)
                assert code == 130 if terminal == "interrupt" else code not in (0, 130)
                host, port = bind.rsplit(":", 1)
                with socket.socket() as settled:
                    settled.bind((host, int(port)))
                return
    assert terminal in ("decline", "restart"), "ready after the offer was not answered"
    assert asked == (terminal == "decline")
    if terminal == "decline":
        assert b"wyrd_global_" in disclosed


def stop_local_cli(child: subprocess.Popen[str]) -> None:
    """Interrupt and reap the owned CLI, killing its group on deadline failure."""
    if child.poll() is None:
        child.send_signal(signal.SIGINT)
    try:
        assert child.wait(timeout=50) == 130, "local CLI interruption exit code"
    except subprocess.TimeoutExpired as err:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
        raise RuntimeError("local CLI failed to settle its server") from err


@contextmanager
def local_deployment() -> Iterator[tuple[Path, dict[str, str]]]:
    """Own one deployment's empty database and temporary installed files.

    The database outlives every CLI and SDK process started for this
    deployment, so restarts reuse its database, configuration, and warehouse.
    The CLI migrates it and issues credentials; this fixture never does.
    """
    root = Path(__file__).resolve().parents[4]
    version = subprocess.check_output(
        [str(root / "target/debug/wyrd"), "--version"], text=True
    ).split()[1]
    with (
        UnmigratedDatabase() as database,
        tempfile.TemporaryDirectory(prefix="wyrd-local-dev-") as working,
    ):
        home = Path(working)
        config = home / "wyrd"
        release = config / "server/versions" / version
        release.mkdir(parents=True)
        shutil.copy2(root / "target/debug/wyrd-server", release / "wyrd-server")
        (config / "server/current").symlink_to(Path("versions") / version)
        env = dict(os.environ)
        for name in (
            "WYRD_API_KEY",
            "WYRD_ACCESS_TOKEN",
            "WYRD_WORKLOAD_TOKEN",
            "WYRD_TENANT",
            "WYRD_SERVER_URL",
            "WYRD_GRPC_URL",
            "WYRD_STORAGE_URL",
            "WYRD_CONFIG_PATH",
            "WYRD_PLATFORM_CREDENTIAL",
            "WYRD_PLATFORM_DATABASE_URL",
        ):
            env.pop(name, None)
        env.update(
            WYRD_CONFIG_HOME=str(config),
            APP_ENV="development",
            WYRD_DATABASE_URL=database.url,
            WYRD_SERVER_BIND=local_listener(),
            WYRD_GRPC_BIND=local_listener(),
            WYRD_BIFROST_PEER_BIND_ADDR=local_listener(),
            WYRD_METRICS_BIND="127.0.0.1:0",
            WYRD_LOCAL_DEV_CONFIG_HOME=str(config),
        )
        (home / "codex").mkdir()
        (home / "codex/config.toml").write_text("# isolated detected host\n")
        yield home, env


@contextmanager
def installed_local_cli(
    cli_path: str, home: Path, env: dict[str, str], terminal: str
) -> Iterator[None]:
    """Run a real installed CLI against ``home`` until the block ends."""
    command_env = dict(env)
    master, slave = pty.openpty() if terminal else (-1, -1)
    if terminal:
        command_env["HOME"] = str(home)
        command_env["CODEX_HOME"] = str(home / "codex")
    with tempfile.TemporaryFile(mode="w+") as errors:
        child = subprocess.Popen(
            [cli_path, "server", "dev", "--tenant", "local"],
            cwd=home,
            env=command_env,
            stdin=slave if terminal else subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=errors,
            text=True,
            preexec_fn=terminal_session if terminal else None,
            start_new_session=not terminal,
        )
        if terminal:
            os.close(slave)
        try:
            try:
                wait_for_local_cli(child, master, terminal, env["WYRD_SERVER_BIND"])
                errors.seek(0)
                failure = errors.read()
                assert "wyrd_sk_" not in failure and "wyrd_global_" not in failure
                assert terminal != "exit" or "server exit" in failure, failure
            except (AssertionError, OSError):
                errors.seek(0)
                print(errors.read(), file=sys.stderr)
                raise
            if terminal not in ("interrupt", "exit"):
                assert child.stdout is not None
                threading.Thread(target=child.stdout.read, daemon=True).start()
            yield
        finally:
            try:
                if terminal not in ("interrupt", "exit"):
                    stop_local_cli(child)
            finally:
                if terminal:
                    os.close(master)


@pytest.mark.parametrize("runtime", ["rust", "python", "typescript", "interrupt", "exit"])
def test_installed_cli_bootstrap(runtime: str) -> None:
    """Run fresh SDK processes through real installed startup and shutdown.

    Interactive first setup offers detected hosts once; after a decline, an
    interruption at the offer, or the serving child's exit there, a restart
    of the same deployment reaches readiness without another question.
    """
    root = Path(__file__).resolve().parents[4]
    native = str(root / "target/debug/wyrd")
    cli_path = os.environ.get(
        "WYRD_TEST_INSTALLED_CLI",
        str(Path(sys.executable).parent / "wyrd") if runtime == "python" else native,
    )
    terminal = {"typescript": "decline", "interrupt": "interrupt", "exit": "exit"}.get(runtime, "")
    with local_deployment() as (home, env):
        with installed_local_cli(cli_path, home, env, terminal):
            if runtime == "rust":
                command = [
                    "mise",
                    "exec",
                    "--",
                    "cargo",
                    "nextest",
                    "run",
                    "--locked",
                    "-p",
                    "wyrd-sdk-rust",
                    "--test",
                    "integration",
                    "-P",
                    "journey",
                    "--run-ignored=all",
                    "-E",
                    "test(=local_development::cli_bootstrap_saved_config_connects)",
                ]
                subprocess.run(command, cwd=root, env=env, check=True)
            elif runtime == "python":
                command = [
                    sys.executable,
                    "-m",
                    "pytest",
                    "-q",
                    "-m",
                    "integration",
                    "tests/integration/test_local_development.py::test_cli_bootstrap_saved_config",
                ]
                subprocess.run(command, cwd=root / "sdks/wyrd-sdk-python", env=env, check=True)
            elif runtime == "typescript":
                command = [
                    "mise",
                    "exec",
                    "--",
                    "pnpm",
                    "exec",
                    "vitest",
                    "run",
                    "tests/integration/local-development.test.ts",
                    "-t",
                    "cli_bootstrap_saved_config",
                ]
                subprocess.run(command, cwd=root / "sdks/wyrd-sdk-ts/wyrd", env=env, check=True)
        if not terminal:
            return
        saved = [
            (home / name).read_bytes()
            for name in ("wyrd/credentials.toml", "wyrd/config.toml", "codex/config.toml")
        ]
        with installed_local_cli(cli_path, home, env, "restart"):
            pass
        assert saved == [
            (home / name).read_bytes()
            for name in ("wyrd/credentials.toml", "wyrd/config.toml", "codex/config.toml")
        ]
