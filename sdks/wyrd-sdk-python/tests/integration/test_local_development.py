"""A developer runs Wyrd locally with only the administrator key ``wyrd setup`` printed.

They register and hydrate an assistant, invoke a model through the Gateway with
the stock OpenAI client, observe and verify a Run, export its spans through
``state.start_telemetry``, and query the evidence back. No key is issued and
nothing is flushed on the server's behalf. ``work`` is that whole workflow for
one client, so the signed-in story proves the same steps for a saved login.
"""

from __future__ import annotations

import fcntl
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
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from http.server import ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote

import httpx
import openai
import pytest
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
    register(cards, "cards/latency_baseline/latency-baseline.yaml")
    register(cards, "cards/verify_in_real_time/latency-model.yaml")
    register(cards, "cards/verify_in_real_time/assistant.yaml")
    state = hydrated(download(cards, "assistant", bundle_home), client)
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
    body = prompt.render(question="hi").model_dump()
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
        config.mkdir(mode=0o700)
        release.mkdir(parents=True)
        shutil.copy2(root / "target/debug/wyrd-server", release / "wyrd-server")
        shutil.copy2(root / "target/debug/wyrd-monty-worker", release / "wyrd-monty-worker")
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
    cli_path: str, home: Path, env: dict[str, str], terminal: str, tenant: str = "local"
) -> Iterator[None]:
    """Run a real installed CLI serving ``tenant`` against ``home`` until the block ends."""
    command_env = dict(env)
    master, slave = pty.openpty() if terminal else (-1, -1)
    if terminal:
        command_env["HOME"] = str(home)
        command_env["CODEX_HOME"] = str(home / "codex")
    with tempfile.TemporaryFile(mode="w+") as errors:
        child = subprocess.Popen(
            [cli_path, "server", "dev", "--tenant", tenant],
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


@pytest.mark.local_dev
def test_installed_cli_gateway_setup(tmp_path: Path) -> None:
    """Set up four named deployments with the installed CLI, as ``examples/gateway-setup`` does.

    The server reaches local provider fixtures through its non-production
    ``[gateway.endpoints]`` configuration. Refused files write nothing,
    reapplying converges, a fresh TypeScript process calls every deployment
    with the saved deployment alone, and each upstream sees its own key and
    never a Wyrd credential. One Agent per provider is then applied, downloaded
    with ``wyrd get``, and loaded from that bundle by a fresh Python process,
    which sends each Agent's rendered Prompt to its deployment. A restarted
    server still decrypts the credentials and serves the same downloaded
    Agents, and a revoked credential or an under-granted caller is refused.
    """
    root = Path(__file__).resolve().parents[4]
    cli_path = str(root / "target/debug/wyrd")
    upstream = ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
    recorded = Receiver(url=f"http://127.0.0.1:{upstream.server_address[1]}")
    upstream.RequestHandlerClass = type("SetupUpstream", (Upstream,), {"upstream": recorded})
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    for name in ("gateway.json", "capture-policy.json"):
        shutil.copy2(root / "examples/gateway-setup" / name, tmp_path / name)
    entries = json.loads((tmp_path / "gateway.json").read_text())
    for provider, (_, value) in UPSTREAM_KEYS.items():
        secret = value.removeprefix("Bearer ")
        source = {"managed_secret": {"secret": secret}}
        document = {"name": f"{provider}-key", "provider": provider, "source": source}
        (tmp_path / f"{provider}-credential.json").write_text(json.dumps(document))
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
                missing = wyrd("gateway", "deployment", "put", "--file", "gateway.json")
                assert missing.returncode != 0
                assert [o["status"] for o in json.loads(missing.stdout)] == [
                    "failed",
                    *["unattempted"] * 3,
                ], missing.stdout
                assert deployments() == []

                for provider in UPSTREAM_KEYS:
                    put = wyrd(
                        "gateway", "credential", "put", "--file", f"{provider}-credential.json"
                    )
                    assert put.returncode == 0, put.stderr
                    assert "sk-" not in put.stdout + put.stderr
                for _ in range(2):
                    applied = wyrd("gateway", "deployment", "put", "--file", "gateway.json")
                    assert applied.returncode == 0, applied.stderr
                    assert {o["status"] for o in json.loads(applied.stdout)} == {"completed"}
                single = wyrd("gateway", "deployment", "put", "--file", "openai-prod.json")
                assert json.loads(single.stdout)["name"] == "openai-prod", single.stderr
                names = sorted(entry["name"] for entry in entries)
                assert deployments() == names
                captured = wyrd("gateway", "capture-policy", "put", "--file", "capture-policy.json")
                assert captured.returncode == 0, captured.stderr

                agent = root / "fixtures/cards/gateway_inference/ask-agent.yaml"
                assert wyrd("apply", str(agent)).returncode == 0
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

                applied = wyrd("apply", str(root / "examples/gateway-setup/agents/service.yaml"))
                assert applied.returncode == 0, applied.stderr
                got = wyrd(
                    *("get", "--kind", "Service", "--name", "gateway-agents"),
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

            with installed_local_cli(cli_path, home, env, "", tenant="acme"):
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
