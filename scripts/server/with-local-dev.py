#!/usr/bin/env python3
"""Run a fresh SDK process against the real installed CLI's saved configuration.

The managed Postgres wrapper owns the database. This fixture owns only the
isolated installation/configuration, server process, and its termination.
"""

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
from pathlib import Path


def listener() -> str:
    """Ask the OS for a loopback port for one isolated server listener."""
    with socket.socket() as bound:
        bound.bind(("127.0.0.1", 0))
        return f"127.0.0.1:{bound.getsockname()[1]}"


def terminal_session() -> None:
    """Give the fixture CLI its own controlling terminal before exec."""
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


def main() -> int:
    """Bootstrap outside the SDK runtime and settle the child on every exit."""
    root = Path(__file__).resolve().parents[2]
    cli = os.environ.get("WYRD_TEST_INSTALLED_CLI", str(root / "target/debug/wyrd"))
    server = root / "target/debug/wyrd-server"
    version = subprocess.check_output(
        [str(root / "target/debug/wyrd"), "--version"], text=True
    ).split()[1]
    with tempfile.TemporaryDirectory(prefix="wyrd-local-dev-") as working:
        home = Path(working)
        config = home / "wyrd"
        release = config / "server/versions" / version
        release.mkdir(parents=True)
        shutil.copy2(server, release / "wyrd-server")
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
        ):
            env.pop(name, None)
        env.update(
            WYRD_CONFIG_HOME=str(config),
            APP_ENV="development",
            WYRD_DATABASE_URL=env.pop("WYRD_PLATFORM_DATABASE_URL"),
            WYRD_SERVER_BIND=listener(),
            WYRD_GRPC_BIND=listener(),
            WYRD_BIFROST_PEER_BIND_ADDR=listener(),
            WYRD_METRICS_BIND="127.0.0.1:0",
        )
        terminal = env.pop("WYRD_LOCAL_DEV_TERMINAL", "")
        assert terminal in ("", "decline", "interrupt")
        command_env = dict(env)
        master, slave = pty.openpty() if terminal else (-1, -1)
        if terminal:
            command_env["HOME"] = working
            command_env["CODEX_HOME"] = str(home / "codex")
            (home / "codex").mkdir()
            (home / "codex/config.toml").write_text("# isolated detected host\n")
        with tempfile.TemporaryFile(mode="w+") as errors:
            child = subprocess.Popen(
                [cli, "server", "dev", "--tenant", "local"],
                cwd=working,
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
                assert child.stdout is not None
                if terminal:
                    shown = b""
                    disclosed = b""
                    answered = False
                    ready = False
                    deadline = time.monotonic() + 150
                    while not ready:
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
                            else:
                                shown += chunk
                                assert b"wyrd_sk_" not in shown and b"wyrd_global_" not in shown
                                if not answered and b"Connect which hosts" in shown:
                                    answered = True
                                    if terminal == "decline":
                                        os.write(master, b"\n")
                                    else:
                                        child.send_signal(signal.SIGINT)
                                ready = b"Local Wyrd ready at " in shown
                                if not chunk:
                                    assert terminal == "interrupt" and answered
                                    assert child.wait(timeout=50) == 130
                                    host, port = env["WYRD_SERVER_BIND"].rsplit(":", 1)
                                    with socket.socket() as settled:
                                        settled.bind((host, int(port)))
                                    return 0
                    assert answered and b"wyrd_global_" in disclosed
                else:
                    for line in child.stdout:
                        assert "wyrd_sk_" not in line and "wyrd_global_" not in line, (
                            "secret in startup output"
                        )
                        if line.startswith("Local Wyrd ready at "):
                            break
                    else:
                        errors.seek(0)
                        raise RuntimeError("local CLI did not become ready: " + errors.read())
                drain = threading.Thread(target=lambda: child.stdout.read(), daemon=True)
                drain.start()
                env["WYRD_LOCAL_DEV_CONFIG_HOME"] = str(config)
                return subprocess.call(sys.argv[1:], cwd=root, env=env)
            finally:
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
                finally:
                    if terminal:
                        os.close(master)


if __name__ == "__main__":
    raise SystemExit(main())
