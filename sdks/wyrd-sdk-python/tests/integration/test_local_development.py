"""A developer runs Wyrd locally with only the administrator key ``wyrd setup`` printed.

They declare and pull an assistant, invoke a model through the Gateway with
the stock OpenAI client, observe and verify a Run, export its spans through
``state.start_telemetry``, and query the evidence back. No key is issued and
nothing is flushed on the server's behalf. ``work`` is that whole workflow for
one client, so the signed-in story proves the same steps for a saved login.
"""

from __future__ import annotations

import fcntl
import http.client
import json
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
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote, urlsplit

import httpx
import openai
import pytest
import tomllib
from opentelemetry import trace
from pydantic import BaseModel
from wyrd.bifrost import Bifrost
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.errors import WyrdError
from wyrd.gateway import Gateway
from wyrd.observe import Run
from wyrd.state import WyrdState
from wyrd.testing import UnmigratedDatabase, WyrdTestServer, cli

from .gateway.support import Upstream
from .support import Receiver, declare, hydrated, pull

pytestmark = pytest.mark.integration


class Attributed(BaseModel):
    """One evidence row: the Card it is attributed to."""

    card_uid: str | None


@dataclass
class Worked:
    """What ``work`` leaves running for a caller to keep using."""

    state: WyrdState
    agent_uid: str


def local_server(access_ttl_seconds: int | None = None) -> Iterator[WyrdTestServer]:
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
            access_ttl_seconds=access_ttl_seconds,
        ) as server:
            yield server
    finally:
        upstream.shutdown()


def work(client: WyrdClient, bundle_home: Path) -> Worked:
    """Run the whole local workflow as ``client`` and assert each step."""
    configure_gateway(client)
    cards = Cards(client)
    declare(cards, "cards/latency_baseline/latency-baseline.yaml")
    declare(cards, "cards/verify_in_real_time/latency-model.yaml")
    declare(cards, "cards/verify_in_real_time/assistant.yaml")
    state = hydrated(pull(cards, "assistant", bundle_home), client)
    state.start_bifrost()

    assert invoke(client) == "hi"

    run = state.run()
    run.for_card("model").observe.submit("latency-drift", {"latency": 12.5})
    result = run.for_card("agent").observe.verify("answer-check", "yes")
    assert (result.verdict, result.verifier.name) == ("passed", "answer-check")

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
    """Write the ``openai-key`` credential and deploy ``openai`` on it as ``client``."""
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
            "name": "openai",
            "adapter": "openai",
            "auth": {"bearer": {"credential": "openai-key"}},
            "capabilities": ["chat_completions"],
        }
    )


def invoke(client: WyrdClient) -> str:
    """Send one Chat Completions turn through the Gateway with the stock OpenAI client."""
    gateway = openai.OpenAI(
        base_url=f"{client.server_url}/v1",
        api_key="wyrd",
        http_client=openai.DefaultHttpxClient(auth=client.http_auth),
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
    declare(cards, "cards/latency_baseline/latency-baseline.yaml")
    declare(cards, "cards/verify_in_real_time/latency-model.yaml")
    declare(cards, "cards/verify_in_real_time/assistant.yaml")
    state = hydrated(pull(cards, "assistant", tmp_path), client)
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
    refs = declare(cards, "cards/gateway_inference/ask-prompt.yaml")
    assert cards.get(refs["ask-prompt"]).metadata["name"] == "ask-prompt"


GATEWAY_AGENTS = {
    "openai": ("openai-prod", "acme/gpt-4o-agent", "hi"),
    "anthropic": ("claude-prod", "claude-haiku-4-5", "hi"),
    "google": ("gemini-team", "tuned/gemini-2.5-pro", "hi"),
    "jev": ("judge-pool", "jev-judge", '{"helpful":{"type":"noul","noul":0.9}}'),
}
"""``examples/gateway-setup/agents`` alias (its Prompt provider) → deployment, model, and answer."""

INGRESS = {
    "openai": (lambda _: "/v1/chat/completions", lambda r: r["choices"][0]["message"]["content"]),
    "anthropic": (lambda _: "/v1/messages", lambda r: r["content"][0]["text"]),
    "google": (
        lambda model: f"/v1beta/models/{model}:generateContent",
        lambda r: r["candidates"][0]["content"]["parts"][0]["text"],
    ),
    "jev": (lambda _: "/v1/systemone", lambda r: json.dumps(r["answers"], separators=(",", ":"))),
}
"""Prompt provider → its gateway ingress path for a ``<deployment>/<model>`` and its answer text."""


def call_agent(client: WyrdClient, state: WyrdState, alias: str, run_id: str) -> httpx.Response:
    """Send Agent ``alias``'s rendered Prompt to its deployment as the application's own call.

    The ``wyrd-run-id`` and ``wyrd-card-uid`` headers correlate the call to
    ``run_id`` and the Agent Card.
    """
    deployment = state.card(alias).spec["execution_route"]["deployment"]
    prompt = state.agent(alias).prompt().prompt
    path, _ = INGRESS[prompt.provider]
    body = prompt.render(question="hi").model_dump()["body"]
    if prompt.provider == "google":
        model = f"{deployment}/{quote(prompt.model, safe='')}"
    else:
        model = body["model"] = f"{deployment}/{prompt.model}"
    return httpx.post(
        f"{client.server_url}{path(model)}",
        json=body,
        headers={"wyrd-run-id": run_id, "wyrd-card-uid": str(state.card_ref(alias).uid)},
        auth=client.http_auth,
    )


def test_cli_gateway_agents(cli_bootstrap_client: WyrdClient) -> None:
    """A fresh Python runtime loads the downloaded gateway Agents and calls each one's deployment.

    Each loaded Agent keeps the registered route, Prompt provider, model, and
    identities. The application sends each rendered Prompt itself, and the call
    is retained under its own Run and Card. A caller whose key is bound to
    another Card is refused.
    """
    bundle = Path(os.environ["WYRD_LOCAL_DEV_AGENT_BUNDLE"])
    state = WyrdState.from_path(bundle, cli_bootstrap_client)
    cards = Cards(cli_bootstrap_client)
    runs = {}
    for alias, (deployment, model, answer) in GATEWAY_AGENTS.items():
        downloaded = state.card(alias)
        registered = cards.get(downloaded.card_ref)
        prompt = state.agent(alias).prompt()
        route = {"kind": "wyrd_gateway", "deployment": deployment}
        assert downloaded.card_ref == registered.card_ref == state.card_ref(alias)
        assert downloaded.spec == registered.spec
        assert downloaded.spec["execution_route"] == route
        assert cards.get(prompt.ref).card_ref == prompt.ref
        assert (prompt.prompt.provider, prompt.prompt.model) == (alias, model)
        run = state.run(alias)
        called = call_agent(cli_bootstrap_client, state, alias, run.run_id)
        assert called.status_code == 200, called.text
        assert INGRESS[alias][1](called.json()) == answer
        runs[run.run_id] = {
            "deployment": deployment,
            "model": model,
            "resolved_deployment": deployment,
            "card_uid": str(downloaded.card_ref.uid),
            "outcome": "succeeded",
        }

    other_card = WyrdClient(
        server_url=cli_bootstrap_client.server_url,
        credential=os.environ["WYRD_LOCAL_DEV_DENIED_KEY"],
    )
    other = WyrdState.from_path(bundle, other_card)
    refused = call_agent(other_card, other, "openai", other.run("openai").run_id)
    assert refused.status_code == 403, refused.text
    assert refused.json()["error"]["code"] == "WYRD_VALA_403_BIFROST_CARD_SCOPE", refused.text

    bifrost = Bifrost(client=cli_bootstrap_client)
    query = (
        "SELECT run_id, requested_model['deployment'] AS deployment,"
        " requested_model['model'] AS model, resolved_deployment, card_uid, outcome"
        " FROM vala.gateway.calls WHERE run_id IN ($1, $2, $3, $4)"
    )
    deadline = time.monotonic() + 60
    while True:
        rows = bifrost.sql(query, list(runs)).to_arrow().to_pylist()
        retained = {row.pop("run_id"): row for row in rows}
        if retained == runs or time.monotonic() > deadline:
            break
        time.sleep(0.5)
    assert retained == runs


def run_python(env: dict[str, str], name: str) -> None:
    """Run the local-development Python test ``name`` in a fresh interpreter with ``env``."""
    root = Path(__file__).resolve().parents[4]
    command = [
        sys.executable,
        *("-m", "pytest", "-q", "-m", "integration"),
        f"tests/integration/test_local_development.py::{name}",
    ]
    ran = subprocess.run(
        command, cwd=root / "sdks/wyrd-sdk-python", env=env, capture_output=True, text=True
    )
    assert ran.returncode == 0 and "1 passed" in ran.stdout, ran.stdout + ran.stderr


def local_listener() -> str:
    """Ask the OS for a loopback port for an isolated server listener."""
    with socket.socket() as bound:
        bound.bind(("127.0.0.1", 0))
        return f"127.0.0.1:{bound.getsockname()[1]}"


def terminal_session() -> None:
    """Give the fixture CLI its own controlling terminal before exec."""
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def wait_for_local_cli(
    child: subprocess.Popen[str], master: int, terminal: str, binds: tuple[str, ...]
) -> None:
    """Assert readiness, or the first-setup offer's outcome, without exposing credentials.

    ``decline`` answers the offer, ``interrupt`` interrupts the CLI at it, and
    ``exit`` terminates only the UI child while it is unanswered, after which
    the CLI must settle its server too. ``restart`` requires readiness with no
    offer. Every listener in ``binds`` is free once an interrupted or failed
    CLI has exited.
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
                    ui = subprocess.check_output(
                        ["pgrep", "-P", str(child.pid), "-f", "ui/build/index.js"], text=True
                    )
                    for pid in ui.split():
                        os.kill(int(pid), signal.SIGTERM)
            if not chunk:
                assert terminal in ("interrupt", "exit") and asked
                code = child.wait(timeout=50)
                assert code == 130 if terminal == "interrupt" else code not in (0, 130)
                for bind in binds:
                    host, port = bind.rsplit(":", 1)
                    with socket.socket() as settled:
                        settled.bind((host, int(port)))
                return
    assert terminal in ("decline", "restart"), "ready after the offer was not answered"
    assert asked == (terminal == "decline")
    if terminal == "decline":
        assert b"wyrd_global_" in disclosed


def stop_local_cli(child: subprocess.Popen[str] | subprocess.Popen[bytes]) -> None:
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


PROVIDER_KEY_VARIABLES = (
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GOOGLE_API_KEY",
    "GEMINI_API_KEY",
    "TYPESAFE_API_KEY",
)
"""Provider keys ``wyrd server dev`` offers; removed so only a test opts in to the offer."""


@contextmanager
def local_deployment() -> Iterator[tuple[Path, dict[str, str]]]:
    """Own one deployment's empty database and temporary installed files.

    The database outlives every CLI and SDK process started for this
    deployment, so restarts reuse its database, configuration, and warehouse.
    The CLI migrates it and issues credentials; this fixture never does. The
    release's ``ui/`` is the checkout's production UI build.
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
        config.mkdir(mode=0o700)
        release.mkdir(parents=True)
        shutil.copy2(root / "target/debug/wyrd-server", release / "wyrd-server")
        ui = root / "crates/wyrd/wyrd-server/wyrd-ui"
        assert (ui / "build/index.js").is_file(), "build the production UI through mise first"
        (release / "ui").symlink_to(ui)
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
            *PROVIDER_KEY_VARIABLES,
        ):
            env.pop(name, None)
        env.update(
            WYRD_CONFIG_HOME=str(config),
            APP_ENV="development",
            WYRD_DATABASE_URL=database.url,
            WYRD_SERVER_BIND=local_listener(),
            WYRD_GRPC_BIND=local_listener(),
            WYRD_BIFROST_PEER_BIND_ADDR=local_listener(),
            WYRD_UI_BIND=local_listener(),
            WYRD_METRICS_BIND="127.0.0.1:0",
            WYRD_LOCAL_DEV_CONFIG_HOME=str(config),
        )
        (home / "codex").mkdir()
        (home / "codex/config.toml").write_text("# isolated detected host\n")
        yield home, env


@contextmanager
def installed_local_cli(
    cli_path: str,
    home: Path,
    env: dict[str, str],
    terminal: str,
    tenant: str = "local",
    extra: tuple[str, ...] = (),
) -> Iterator[None]:
    """Run a real installed CLI serving ``tenant`` against ``home`` until the block ends.

    ``extra`` appends further ``server dev`` arguments, such as a gateway file.
    """
    command_env = dict(env)
    master, slave = pty.openpty() if terminal else (-1, -1)
    if terminal:
        command_env["HOME"] = str(home)
        command_env["CODEX_HOME"] = str(home / "codex")
    with tempfile.TemporaryFile(mode="w+") as errors:
        child = subprocess.Popen(
            [cli_path, "server", "dev", "--tenant", tenant, *extra],
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
                wait_for_local_cli(
                    child, master, terminal, (env["WYRD_SERVER_BIND"], env["WYRD_UI_BIND"])
                )
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


def run_typescript(env: dict[str, str], name: str) -> None:
    """Run the local-development TypeScript test ``name`` in a fresh Node process with ``env``."""
    root = Path(__file__).resolve().parents[4]
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
        name,
    ]
    subprocess.run(command, cwd=root / "sdks/wyrd-sdk-ts/wyrd", env=env, check=True)


@pytest.mark.local_dev
@pytest.mark.parametrize("runtime", ["rust", "python", "typescript", "interrupt", "exit"])
def test_installed_cli_bootstrap(runtime: str) -> None:
    """Run fresh SDK processes through real installed startup and shutdown.

    Interactive first setup offers detected hosts once; after a decline, an
    interruption at the offer, or the UI child's exit there, a restart of the
    same deployment reaches readiness without another question.
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
                    "wyrd-ai",
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
                run_typescript(env, "cli_bootstrap_saved_config")
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


UPSTREAM_KEYS = {
    "openai": ("authorization", "Bearer sk-openai-local"),
    "anthropic": ("x-api-key", "sk-anthropic-local"),
    "gemini": ("x-goog-api-key", "sk-gemini-local"),
    "jev": ("authorization", "Bearer sk-jev-local"),
}
"""Provider → the upstream credential header and value its fixture key arrives as."""

UPSTREAM_CALLS = {
    "/v1/chat/completions": ("openai", "gpt-4o"),
    "/v1/messages": ("anthropic", "claude-sonnet-4-5"),
    "/v1beta/models/gemini-2.5-flash:generateContent": ("gemini", None),
    "/v1/systemone": ("jev", "jev-latest"),
}
"""Upstream path → the provider serving it and the body model it must receive unchanged."""

AGENT_CALLS = [
    ("/v1/chat/completions", "acme/gpt-4o-agent"),
    ("/v1/messages", "claude-haiku-4-5"),
    ("/v1beta/models/tuned%2Fgemini-2.5-pro:generateContent", None),
    ("/v1/systemone", "jev-judge"),
]
"""The upstream path and unchanged body model of each ``GATEWAY_AGENTS`` run, in run order."""


def calls_since(recorded: Receiver, start: int) -> list[tuple[str, str | None]]:
    """The path and body model of every upstream request after the first ``start``."""
    return [(d.path, d.body.get("model")) for d in recorded.deliveries[start:]]


class LosingProxy(BaseHTTPRequestHandler):
    """Forward-proxies plain HTTP, dropping the answer to every composite gateway apply.

    The apply is forwarded and its full answer read, so the server has
    committed before the connection closes with no response.
    """

    protocol_version = "HTTP/1.1"

    def forward(self) -> None:
        """Forward one request to its absolute target and relay or drop the answer."""
        target = urlsplit(self.path)
        body = self.rfile.read(int(self.headers.get("Content-Length", 0)))
        upstream = http.client.HTTPConnection(target.netloc, timeout=60)
        try:
            upstream.request(
                self.command,
                target.path + (f"?{target.query}" if target.query else ""),
                body=body,
                headers={k: v for k, v in self.headers.items() if k.lower() != "proxy-connection"},
            )
            answer = upstream.getresponse()
            payload = answer.read()
        finally:
            upstream.close()
        if self.command == "POST" and target.path == "/v1/admin/gateway/provider-deployments":
            self.close_connection = True
            return
        self.send_response(answer.status)
        for name, value in answer.getheaders():
            if name.lower() not in ("transfer-encoding", "connection", "content-length"):
                self.send_header(name, value)
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    do_GET = do_POST = do_PUT = do_DELETE = do_PATCH = forward

    def log_message(self, format: str, *args: object) -> None:  # noqa: A002 - stdlib signature
        """Keep proxy traffic out of test output."""


@pytest.mark.local_dev
def test_installed_cli_gateway_setup(tmp_path: Path) -> None:
    """Set up four named deployments with the installed CLI, as ``examples/gateway-setup`` does.

    The server reaches local provider fixtures through its non-production
    ``[gateway.endpoints]`` configuration. Refused files, including the
    shipped document with its ``local_env`` keys unset, write nothing; the
    document applied from the environment, and the two-step deployment list,
    converge; and a restart with ``--gateway-file`` reapplies it, a fresh TypeScript process calls every deployment
    with the saved deployment alone, and each upstream sees its own key and
    never a Wyrd credential. One Agent per provider is then declared, pulled
    with ``wyrd pull``, and loaded from that bundle by a fresh Python process,
    which sends each Agent's rendered Prompt to its deployment. A restarted
    server still decrypts the credentials and serves the same downloaded
    Agents, and a revoked credential or an under-granted caller is refused. A
    startup whose gateway apply commits but loses its answer fails without
    claiming noncommit and settles its server; the committed deployment reads
    back on restart and reapplying the same document succeeds.
    """
    root = Path(__file__).resolve().parents[4]
    cli_path = str(root / "target/debug/wyrd")
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    upstream.RequestHandlerClass = type("SetupUpstream", (Upstream,), {"upstream": recorded})
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    for name in ("gateway.json", "capture-policy.json"):
        shutil.copy2(root / "examples/gateway-setup" / name, tmp_path / name)
    composite = json.loads((tmp_path / "gateway.json").read_text())
    entries = composite["deployments"]
    keys = {
        credential["source"]["local_env"]: UPSTREAM_KEYS[credential["provider"]][1].removeprefix(
            "Bearer "
        )
        for credential in composite["credentials"]
    }
    (tmp_path / "deployments.json").write_text(json.dumps(entries))
    (tmp_path / "openai-prod.json").write_text(json.dumps(entries[0]))
    (tmp_path / "malformed.json").write_text("[{")
    (tmp_path / "duplicate.json").write_text(json.dumps([entries[0], entries[0]]))
    try:
        with local_deployment() as (home, env):
            (home / "server.toml").write_text(
                "[gateway.endpoints]\n"
                f'openai = "{recorded.url}/v1"\n'
                f'anthropic = "{recorded.url}"\n'
                f'gemini = "{recorded.url}"\n'
                f'jev = "{recorded.url}"\n'
            )
            env["WYRD_CONFIG_PATH"] = str(home / "server.toml")

            def wyrd(*args: str, caller: dict[str, str] = env) -> subprocess.CompletedProcess[str]:
                return subprocess.run(
                    [cli_path, *args], cwd=tmp_path, env=caller, capture_output=True, text=True
                )

            def deployments() -> list[str]:
                listed = wyrd("gateway", "deployment", "list")
                assert listed.returncode == 0, listed.stderr
                return sorted(deployment["name"] for deployment in json.loads(listed.stdout))

            with installed_local_cli(cli_path, home, env, "", tenant="acme"):
                for refused in ("malformed.json", "duplicate.json"):
                    failed = wyrd("gateway", "deployment", "put", "--file", refused)
                    assert failed.returncode != 0 and "--file" in failed.stderr, failed.stderr
                missing = wyrd("gateway", "deployment", "put", "--file", "deployments.json")
                assert missing.returncode != 0
                assert [o["status"] for o in json.loads(missing.stdout)] == [
                    "failed",
                    *["unattempted"] * 3,
                ], missing.stdout
                unset = wyrd("gateway", "deployment", "put", "--file", "gateway.json")
                assert unset.returncode != 0 and "local_env" in unset.stderr, unset.stderr
                assert deployments() == []

                for _ in range(2):
                    applied = wyrd(
                        "gateway",
                        "deployment",
                        "put",
                        "--file",
                        "gateway.json",
                        caller={**env, **keys},
                    )
                    assert applied.returncode == 0, applied.stderr
                    assert "sk-" not in applied.stdout + applied.stderr
                    assert sorted(d["name"] for d in json.loads(applied.stdout)["deployments"]) == (
                        sorted(entry["name"] for entry in entries)
                    )
                rewritten = wyrd("gateway", "deployment", "put", "--file", "deployments.json")
                assert {o["status"] for o in json.loads(rewritten.stdout)} == {"completed"}
                single = wyrd("gateway", "deployment", "put", "--file", "openai-prod.json")
                assert json.loads(single.stdout)["name"] == "openai-prod", single.stderr
                names = sorted(entry["name"] for entry in entries)
                assert deployments() == names
                captured = wyrd("gateway", "capture-policy", "put", "--file", "capture-policy.json")
                assert captured.returncode == 0, captured.stderr

                agent = root / "fixtures/cards/gateway_inference/ask-agent.yaml"
                assert wyrd("declare", str(agent)).returncode == 0
                issued = wyrd(
                    *("auth", "issue-key", "--kind", "Agent", "--name", "ask-agent"),
                    *("--version", "1.0.0", "--space", "default"),
                    *("--server", f"http://{env['WYRD_SERVER_BIND']}"),
                )
                assert issued.returncode == 0, issued.stderr
                key = next(
                    line.split()[1]
                    for line in issued.stdout.splitlines()
                    if line.startswith("key:")
                )
                under_granted = {**env, "WYRD_API_KEY": key}
                denied = wyrd(
                    "gateway",
                    "deployment",
                    "put",
                    "--file",
                    "openai-prod.json",
                    caller=under_granted,
                )
                assert denied.returncode != 0 and "403" in denied.stderr, denied.stderr

                run_typescript(env, "cli_gateway_setup")
                direct = len(recorded.deliveries)

                applied = wyrd("declare", str(root / "examples/gateway-setup/agents/service.yaml"))
                assert applied.returncode == 0, applied.stderr
                got = wyrd(
                    *("pull", "--kind", "Service", "--name", "gateway-agents"),
                    *("--version", "1.0.0", "--space", "default", "--output-dir", "gateway-agents"),
                )
                assert got.returncode == 0, got.stderr
                agent_env = {
                    **env,
                    "WYRD_LOCAL_DEV_AGENT_BUNDLE": str(tmp_path / "gateway-agents"),
                    "WYRD_LOCAL_DEV_DENIED_KEY": key,
                }
                run_python(agent_env, "test_cli_gateway_agents")
                assert calls_since(recorded, direct) == AGENT_CALLS

            wyrd_credentials = (home / "wyrd/credentials.toml").read_text()
            assert sorted(calls_since(recorded, 0)[:direct]) == sorted(
                (path, model) for path, (_, model) in UPSTREAM_CALLS.items()
            )

            gateway_file = ("--gateway-file", str(tmp_path / "gateway.json"))
            with installed_local_cli(
                cli_path, home, {**env, **keys}, "", tenant="acme", extra=gateway_file
            ):
                assert deployments() == names
                run_typescript(env, "cli_gateway_setup")
                rerun = len(recorded.deliveries)
                run_python(agent_env, "test_cli_gateway_agents")
                assert calls_since(recorded, rerun) == AGENT_CALLS
                revoked = wyrd("gateway", "credential", "revoke", "openai-key")
                assert revoked.returncode == 0, revoked.stderr
                delivered = len(recorded.deliveries)
                run_typescript(env, "cli_gateway_revoked")
                assert len(recorded.deliveries) == delivered, "a revoked credential was sent"
            assert (home / "wyrd/credentials.toml").read_text() == wyrd_credentials

            lost = tmp_path / "lost.json"
            lost.write_text(json.dumps({"deployments": [{**entries[1], "name": "lost-answer"}]}))
            proxy = ThreadingHTTPServer(("127.0.0.1", 0), LosingProxy)
            threading.Thread(target=proxy.serve_forever, daemon=True).start()
            via = f"http://127.0.0.1:{proxy.server_address[1]}"
            try:
                failed = subprocess.run(
                    [cli_path, "server", "dev", "--tenant", "acme", "--gateway-file", str(lost)],
                    cwd=home,
                    env={
                        **env,
                        "HTTP_PROXY": via,
                        "http_proxy": via,
                        "NO_PROXY": "",
                        "no_proxy": "",
                    },
                    stdin=subprocess.DEVNULL,
                    capture_output=True,
                    text=True,
                    timeout=200,
                )
            finally:
                proxy.shutdown()
            assert failed.returncode not in (0, 130), failed.stderr
            assert "may have committed" in failed.stderr, failed.stderr
            assert "nothing was applied" not in failed.stderr
            assert "wyrd_sk_" not in failed.stdout + failed.stderr
            host, port = env["WYRD_SERVER_BIND"].rsplit(":", 1)
            with socket.socket() as settled:
                settled.bind((host, int(port)))
            with installed_local_cli(cli_path, home, env, "", tenant="acme"):
                assert deployments() == sorted([*names, "lost-answer"]), "commit not durable"
                reapplied = wyrd("gateway", "deployment", "put", "--file", str(lost))
                assert reapplied.returncode == 0, reapplied.stderr
                assert deployments() == sorted([*names, "lost-answer"])
            assert (home / "wyrd/credentials.toml").read_text() == wyrd_credentials
            for delivery in recorded.deliveries:
                gemini = delivery.path.endswith(":generateContent")
                header, value = UPSTREAM_KEYS[
                    "gemini" if gemini else UPSTREAM_CALLS[delivery.path][0]
                ]
                assert delivery.headers[header] == value
                assert header == "authorization" or "authorization" not in delivery.headers
                assert not any(name.startswith("x-wyrd") for name in delivery.headers)
                assert "wyrd_" not in json.dumps(delivery.headers)
                assert not any(v.startswith("Bearer ey") for v in delivery.headers.values())
    finally:
        upstream.shutdown()


def converse(
    child: subprocess.Popen[bytes], master: int, answers: list[tuple[bytes, bytes]]
) -> tuple[bytes, bytes]:
    """Answer each expected question, in order, until the whole ready message.

    Returns everything the CLI printed and everything the terminal echoed or
    was sent, so callers can prove what was and was not disclosed.
    """
    assert child.stdout is not None
    shown = disclosed = b""
    seen = 0
    pending = list(answers)
    deadline = time.monotonic() + 150
    # The ready message arrives one line per write; wait for its last line.
    while b"Press Ctrl-C to stop." not in shown:
        assert time.monotonic() < deadline, f"guided startup deadline: {shown!r}"
        readable, _, _ = select.select([child.stdout, master], [], [], 1)
        for source in readable:
            descriptor = master if source == master else child.stdout.fileno()
            try:
                chunk = os.read(descriptor, 4096)
            except OSError:
                chunk = b""
            if source == master:
                disclosed += chunk
                continue
            assert chunk, f"guided CLI exited: {shown!r}"
            shown += chunk
            while pending and (found := shown.find(pending[0][0], seen)) >= 0:
                seen = found + len(pending[0][0])
                os.write(master, pending.pop(0)[1] + b"\n")
    assert not pending, f"unasked questions: {pending!r}"
    return shown, disclosed


def interrupt_hidden_question(cli_path: str, home: Path, env: dict[str, str], sent: int) -> None:
    """Send ``sent`` while the first concealed question waits on a real terminal.

    The CLI must exit with 130, leave terminal echo restored, disclose nothing,
    and install, migrate, or save nothing.
    """
    master, slave = pty.openpty()
    try:
        child = subprocess.Popen(
            [cli_path, "server", "dev"],
            cwd=home,
            env={**env, "HOME": str(home), "CODEX_HOME": str(home / "codex")},
            stdin=slave,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            preexec_fn=terminal_session,
        )
        assert child.stdout is not None
        shown = b""
        deadline = time.monotonic() + 60
        while b"(input hidden): " not in shown:
            assert time.monotonic() < deadline, f"hidden question deadline: {shown!r}"
            chunk = os.read(child.stdout.fileno(), 4096)
            assert chunk, f"CLI exited before asking: {shown!r}"
            shown += chunk
        while termios.tcgetattr(slave)[3] & termios.ECHO:
            assert time.monotonic() < deadline, "echo was never disabled"
            time.sleep(0.05)
        os.write(master, b"postgres://typed-but-unsent")
        child.send_signal(sent)
        stdout, stderr = child.communicate(timeout=30)
        assert child.returncode == 130, stderr
        assert termios.tcgetattr(slave)[3] & termios.ECHO, "echo left disabled"
        assert b"typed-but-unsent" not in shown + stdout + stderr
        assert not (home / "wyrd/config.toml").exists()
        assert not (home / "wyrd/credentials.toml").exists()
        assert not (home / "wyrd/server/sealing.key").exists()
    finally:
        os.close(master)
        os.close(slave)


def unattended_terminal_offers_no_hosts(cli_path: str) -> None:
    """A complete ``--non-interactive`` run on a terminal never asks about hosts.

    A detected host is left unchanged on first setup; an explicit ``--host``
    on the configured restart still installs it without a question.
    """
    with local_deployment() as (home, env):
        host_config = home / "codex/config.toml"
        before = host_config.read_bytes()
        with installed_local_cli(cli_path, home, env, "restart", extra=("--non-interactive",)):
            pass
        assert host_config.read_bytes() == before, "unattended setup changed a host"
        with installed_local_cli(
            cli_path, home, env, "restart", extra=("--non-interactive", "--host", "codex")
        ):
            assert host_config.read_bytes() != before, "explicit host was not installed"


def restart_on_another_listener(
    cli_path: str, home: Path, env: dict[str, str], listed: Callable[[str], list[str]]
) -> None:
    """Resume completed setup on a terminal with only the HTTP listener changed.

    Provider keys and a detected host remain, yet no gateway or host question
    is asked: the saved key, proven against the server, marks the deployment
    as configured. The key and gateway state are unchanged, the new endpoint
    is saved and usable, and the owned server settles on interruption.
    """
    rebound = local_listener()
    protected = home / "wyrd/credentials.toml"
    key = tomllib.loads(protected.read_text())["default"]["api_key"]
    master, slave = pty.openpty()
    child = subprocess.Popen(
        [cli_path, "server", "dev", "--bind", rebound],
        cwd=home,
        env={**env, "HOME": str(home), "CODEX_HOME": str(home / "codex")},
        stdin=slave,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        preexec_fn=terminal_session,
    )
    os.close(slave)
    try:
        shown, _ = converse(child, master, [])
        assert child.stdout is not None
        threading.Thread(target=child.stdout.read, daemon=True).start()
        assert b"Configure which providers" not in shown, "restart offered providers"
        assert b"Connect which hosts" not in shown, "restart offered hosts"
        assert f"Local Wyrd ready at http://{rebound} for tenant guided.".encode() in shown
        assert listed("deployment") == ["gemini-local"]
        assert listed("credential") == ["gemini-key"]
    finally:
        stop_local_cli(child)
        os.close(master)
    assert tomllib.loads(protected.read_text())["default"]["api_key"] == key
    saved = tomllib.loads((home / "wyrd/config.toml").read_text())
    assert saved["client"]["http_url"] == f"http://{rebound}"
    assert saved["dev"]["bind"] == rebound
    host, port = rebound.rsplit(":", 1)
    with socket.socket() as settled:
        settled.bind((host, int(port)))


@pytest.mark.local_dev
def test_installed_cli_guided_setup() -> None:
    """Guided first setup asks only for missing inputs and approved providers.

    An unattended run lists every missing input before any effect. On a
    terminal the database URL is asked with echo off and the tenant slug in
    the open. Of the detected provider keys only the approved Gemini key,
    selected from ``GOOGLE_API_KEY`` over ``GEMINI_API_KEY`` and renamed, is
    applied. Fresh CLI processes then read it with saved settings alone, and
    no key or database URL reaches output or general configuration.
    Interrupting the first hidden question restores echo and changes nothing,
    and an unattended run on a terminal never offers hosts.
    """
    root = Path(__file__).resolve().parents[4]
    cli_path = str(root / "target/debug/wyrd")
    secrets = {
        "OPENAI_API_KEY": "sk-guided-openai",
        "GOOGLE_API_KEY": "sk-guided-google",
        "GEMINI_API_KEY": "sk-guided-gemini",
    }
    with local_deployment() as (home, env):
        database = env.pop("WYRD_DATABASE_URL")
        env.update(secrets)
        unattended = subprocess.run(
            [cli_path, "server", "dev", "--non-interactive"],
            cwd=home,
            env=env,
            capture_output=True,
            text=True,
        )
        assert unattended.returncode != 0
        assert "WYRD_DATABASE_URL" in unattended.stderr and "--tenant" in unattended.stderr
        assert not (home / "wyrd/credentials.toml").exists()
        for sent in (signal.SIGINT, signal.SIGTERM):
            interrupt_hidden_question(cli_path, home, env, sent)

        master, slave = pty.openpty()
        stderr = tempfile.TemporaryFile()  # noqa: SIM115 - read after the child exits
        child = subprocess.Popen(
            [cli_path, "server", "dev"],
            cwd=home,
            env={**env, "HOME": str(home), "CODEX_HOME": str(home / "codex")},
            stdin=slave,
            stdout=subprocess.PIPE,
            stderr=stderr,
            preexec_fn=terminal_session,
        )
        os.close(slave)
        try:
            shown, disclosed = converse(
                child,
                master,
                [
                    (b"(input hidden): ", database.encode()),
                    (b"Tenant slug to provision: ", b"guided"),
                    (b"Configure which providers", b"2"),
                    (b"Deployment name for gemini [gemini]: ", b"gemini-local"),
                    (b"Apply deployments gemini-local", b""),
                    (b"Connect which hosts", b""),
                ],
            )
            assert child.stdout is not None
            threading.Thread(target=child.stdout.read, daemon=True).start()
            assert b"openai from OPENAI_API_KEY" in shown
            assert b"gemini from GOOGLE_API_KEY" in shown
            assert b"WYRD_STORAGE_URL is unset" in shown
            assert b"configured deployments gemini-local" in shown

            def listed(resource: str) -> list[str]:
                fresh = subprocess.run(
                    [cli_path, "gateway", resource, "list"],
                    cwd=home,
                    env={k: v for k, v in env.items() if k not in secrets},
                    capture_output=True,
                    text=True,
                )
                assert fresh.returncode == 0, fresh.stderr
                return [item["name"] for item in json.loads(fresh.stdout)]

            assert listed("deployment") == ["gemini-local"]
            assert listed("credential") == ["gemini-key"]
        finally:
            stop_local_cli(child)
            os.close(master)
        stderr.seek(0)
        errors = stderr.read()
        stderr.close()
        general = (home / "wyrd/config.toml").read_bytes()
        protected = (home / "wyrd/credentials.toml").read_bytes()
        assert b'tenant = "guided"' in general
        for leaked in (database, *secrets.values(), "wyrd_sk_"):
            assert leaked.encode() not in shown + errors + general
        assert database.encode() not in disclosed
        assert database.encode() in protected
        assert not any(value.encode() in protected for value in secrets.values())
        restart_on_another_listener(cli_path, home, env, listed)
    unattended_terminal_offers_no_hosts(cli_path)


EXAMPLE_REPOS = ("wyrd-example-py", "wyrd-example-ts", "wyrd-example-rs")
"""Standalone example repositories, checked out beside Wyrd or under ``WYRD_EXAMPLES_DIR``."""


@pytest.mark.local_dev
def test_standalone_examples_use_saved_local_setup() -> None:
    """Each standalone example starts Wyrd with its own shipped gateway document.

    No example ships a Wyrd bootstrap of its own: no key or tenant script, and
    a Compose file that runs PostgreSQL only. Starting the installed CLI from
    each repository's ``gateway.json`` with ``OPENAI_API_KEY`` set converges on
    one tenant; fresh CLI and Python processes then use the ``openai``
    deployment the apps call with saved settings alone, and the provider key
    reaches neither output nor saved files.
    """
    root = Path(__file__).resolve().parents[4]
    examples = Path(os.environ.get("WYRD_EXAMPLES_DIR", root.parent))
    repos = [examples / name for name in EXAMPLE_REPOS]
    absent = [str(repo) for repo in repos if not (repo / "gateway.json").is_file()]
    if absent:
        raise pytest.skip.Exception(f"standalone examples are not checked out: {absent}")
    cli_path = str(root / "target/debug/wyrd")
    provider_key = "sk-example-openai"
    with local_deployment() as (home, env):
        caller = dict(env)

        def listed(resource: str) -> list[str]:
            fresh = subprocess.run(
                [cli_path, "gateway", resource, "list"],
                cwd=home,
                env=caller,
                capture_output=True,
                text=True,
            )
            assert fresh.returncode == 0, fresh.stderr
            assert provider_key not in fresh.stdout + fresh.stderr
            return [item["name"] for item in json.loads(fresh.stdout)]

        for repo in repos:
            assert not (repo / "docker/keys.sh").exists()
            assert not (repo / "docker/setup.sh").exists()
            compose = repo / "docker-compose.yml"
            assert not compose.exists() or "wyrd:dev" not in compose.read_text(), compose
            assert provider_key not in (repo / "gateway.json").read_text()
            gateway_file = ("--gateway-file", str(repo / "gateway.json"))
            started = {**env, "OPENAI_API_KEY": provider_key}
            with installed_local_cli(
                cli_path, home, started, "", tenant="support", extra=gateway_file
            ):
                assert listed("deployment") == ["openai"], repo
                assert listed("credential") == ["openai-key"], repo
                run_python(caller, "test_cli_bootstrap_saved_config")
        for saved in (home / "wyrd").rglob("*.toml"):
            assert provider_key not in saved.read_text(), saved
