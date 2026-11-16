"""Closed-loop Python caller for the L1 Verification benchmark.

The Rust capacity controller starts this process once, sends one JSON
configuration line on stdin, and then one command line per measured window;
each answer is one JSON line on stdout:

- configuration ``{"server_url", "verifier", "arrivals": [even, odd],
  "applications": [{"bundle", "key"}]}`` answers ``{"ready": {...}}`` once
  every application's state is loaded and every worker thread has started;
- ``{"run": {"clients": N, "seconds": S, "rate": R}}`` runs N prestarted
  threads, each calling its own application's public ``observe.verify`` for
  S seconds, and answers ``{"done": {"calls": [...]}}`` once every call in
  flight at the stop has returned. With ``rate`` null each thread calls back
  to back; with a rate, thread i's k-th call waits until slot ``i + k * N``,
  due ``slot / R`` seconds after the window opened, reports the slot's time
  after the window opened as ``due_ns`` and how late it started as
  ``lateness_ns``, and sends every slot due before the window ends, however
  late. The window opens once the recorder snapshot is taken and every
  caller is waiting at its start. A paced call also reports ``woke_ns``, how
  late its thread woke before preparing the call, and every ``done`` carries
  ``collections``, each garbage collection the interpreter ran during the
  window as ``{"start_ns", "duration_ns"}`` after the window opened;
- ``{"heap": {"stem": P}}`` saves the managed heap's live allocations as
  ``P.tracemalloc`` and, from a ``bench`` native module, the native module's
  sampled allocations, allocated and still live, as ``P.pb.gz``; it answers
  ``{"heap": {"paths": [...]}}``.

A ``bench`` build of the native module (``mise run bench:build:sdks``) has a
``_benchmark`` submodule: the driver installs its recorder at startup, so the
shared client's and the binding's ``sdk.verify`` timings record, and every
``done`` also carries ``timings``, the recorder snapshots immediately before
the window's first call and after its last returned; any other build answers
``timings: null``. The configuration's ``profile`` asks for diagnostic heap
captures: ``tracemalloc`` traces the managed heap from startup, and the native
sampler starts with it. CPU profiles are external: the controller starts the
interpreter with ``-X perf``, so ``samply`` names Python frames beside native
ones.

Each call's clock starts immediately before ``verify`` and reads twice: when
it returns, and when its verdict and every requirement's outcome, citations,
and data are materialized from the result, so the result's lazy conversion is
in the second sample only. Run and input preparation come before the clock;
the controller judges the reported results after the window. Run from
``sdks/wyrd-sdk-python`` with ``python -m tests.benchmark_driver``.
"""

from __future__ import annotations

import gc
import importlib.metadata
import itertools
import json
import os
import platform
import sys
import threading
import time
import tracemalloc
from collections.abc import Callable, Iterator
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from wyrd import WyrdError, _wyrd
from wyrd.client import WyrdClient
from wyrd.model import ModelInterface
from wyrd.state import WyrdState
from wyrd.verify import VerificationResult

BENCHMARK: Any = getattr(_wyrd, "_benchmark", None)


def readout() -> Any:
    """This process's benchmark recorder snapshot, or ``None`` without a ``bench`` native module."""
    return None if BENCHMARK is None else json.loads(BENCHMARK.snapshot())


def capture_heap(stem: Path) -> dict[str, Any]:
    """Save the managed heap's live allocations and, from a ``bench`` module, the native ones beside ``stem``."""
    if not tracemalloc.is_tracing():
        raise RuntimeError("the driver is not heap-profiled")
    managed = stem.with_name(f"{stem.name}.tracemalloc")
    tracemalloc.take_snapshot().dump(str(managed))
    paths = [str(managed)]
    if BENCHMARK is not None:
        native = stem.with_name(f"{stem.name}.pb.gz")
        native.write_bytes(BENCHMARK.heap_profile())
        paths.append(str(native))
    return {"paths": paths}


class BenchModel(ModelInterface):
    """Stand-in for the benchmark Model's ``Custom`` loader, which does not exist offline."""

    def __init__(self) -> None:
        """Start with the empty holder slot the Model holder expects after load."""
        super().__init__()
        self.model: object = None

    def save(self, path: Path, save_kwargs: dict[str, object] | None = None) -> None:
        """Never called: the benchmark only verifies the pulled Model Card."""
        raise NotImplementedError

    def load(self, path: Path, load_kwargs: dict[str, object] | None = None) -> None:
        """Read nothing and publish an identity function as the loaded model."""
        self.model = lambda value: value


@dataclass(frozen=True)
class Call:
    """One answered call: its samples when acknowledged, or its stable error code."""

    sequence: int
    returned_ns: int
    ready_ns: int
    late: bool
    error: str | None = None
    execution_id: str | None = None
    outcome: dict[str, Any] | None = None
    lateness_ns: int | None = None
    due_ns: int | None = None
    woke_ns: int | None = None

    def encode(self) -> str:
        """The call as the controller reads it, as one JSON object."""
        return json.dumps(
            {
                "sequence": self.sequence,
                "returned_ns": self.returned_ns,
                "ready_ns": self.ready_ns,
                "late": self.late,
                "error": self.error,
                "execution_id": self.execution_id,
                "outcome": self.outcome,
                "lateness_ns": self.lateness_ns,
                "due_ns": self.due_ns,
                "woke_ns": self.woke_ns,
            }
        )


def materialize(result: VerificationResult) -> dict[str, Any]:
    """Read the verdict and each requirement's outcome, citations, and data once."""
    requirements = result.requirements
    for requirement in requirements:
        requirement["outcome"]
        requirement.get("citations")
        requirement.get("data")
    return {"verdict": result.verdict, "requirements": requirements}


def timed(
    verify: Callable[[], VerificationResult],
    clock: Callable[[], int] = time.perf_counter_ns,
) -> tuple[VerificationResult, dict[str, Any], int, int]:
    """Call ``verify``, then materialize its result, timing both from just before the call."""
    sent = clock()
    result = verify()
    returned = clock() - sent
    outcome = materialize(result)
    return result, outcome, returned, clock() - sent


class Driver:
    """The benchmark's loaded applications and prestarted worker threads."""

    def __init__(self, configuration: dict[str, Any]) -> None:
        """Load every application's pulled state under its own key and start every worker."""
        if BENCHMARK is not None:
            BENCHMARK.install()
        if configuration.get("profile", {}).get("heap", False):
            tracemalloc.start()
            if BENCHMARK is not None:
                BENCHMARK.heap_start()
        self.verifier: str = configuration["verifier"]
        self.arrivals: list[dict[str, Any]] = configuration["arrivals"]
        self.states = [
            WyrdState.from_path(
                Path(application["bundle"]),
                WyrdClient(server_url=configuration["server_url"], credential=application["key"]),
                interfaces={"model": BenchModel()},
            )
            for application in configuration["applications"]
        ]
        self.workers = ThreadPoolExecutor(max_workers=len(self.states))
        started = threading.Barrier(len(self.states) + 1)
        for _ in self.states:
            self.workers.submit(started.wait)
        started.wait()

    def identity(self) -> dict[str, Any]:
        """The runtime and native module this process measures."""
        return {
            "language": "python",
            "runtime": f"{platform.python_implementation()} {platform.python_version()}",
            "sdk": importlib.metadata.version("wyrd-ai"),
            "native": _wyrd.__file__,
            "pid": os.getpid(),
            "workers": len(self.states),
        }

    def run(self, clients: int, seconds: float, rate: int | None) -> str:
        """Run ``clients`` callers for ``seconds``, closed loop or paced at ``rate``, and answer the JSON ``done`` body: every call, those in flight at the stop included, the recorder around them, and the window's garbage collections.

        Each call is encoded to JSON the moment it finishes, so the window keeps strings, which the
        garbage collector never scans, rather than thousands of call and outcome containers whose
        collections would stall every waiting caller.
        """
        if not 1 <= clients <= len(self.states):
            raise ValueError(f"{clients} clients exceed the {len(self.states)} loaded applications")
        window: dict[str, int] = {}

        def open_window() -> None:
            window["opened"] = time.monotonic_ns()
            window["stop"] = window["opened"] + int(seconds * 1e9)

        start = threading.Barrier(clients + 1, action=open_window)
        sequences = itertools.count()
        lock = threading.Lock()

        def next_sequence() -> int:
            with lock:
                return next(sequences)

        def client(index: int) -> list[str]:
            state = self.states[index]
            calls: list[str] = []
            start.wait()
            opened, stop = window["opened"], window["stop"]
            for k in itertools.count():
                due = woke = None
                if rate is not None:
                    due = opened + (index + k * clients) * 1_000_000_000 // rate
                    if due >= stop:
                        break
                    wait = due - time.monotonic_ns()
                    if wait > 0:
                        time.sleep(wait / 1e9)
                    woke = max(0, time.monotonic_ns() - due)
                elif time.monotonic_ns() >= stop:
                    break
                sequence = next_sequence()
                observe = state.run("model").observe
                arrival = dict(self.arrivals[sequence % 2], sequence=str(sequence))
                lateness = None if due is None else max(0, time.monotonic_ns() - due)
                offset = None if due is None else due - opened
                sent = time.perf_counter_ns()
                try:
                    result, outcome, returned, ready = timed(
                        lambda: observe.verify(self.verifier, arrival)  # noqa: B023
                    )
                except WyrdError as error:
                    elapsed = time.perf_counter_ns() - sent
                    calls.append(
                        Call(
                            sequence,
                            elapsed,
                            elapsed,
                            False,
                            error=error.code,
                            lateness_ns=lateness,
                            due_ns=offset,
                            woke_ns=woke,
                        ).encode()
                    )
                    continue
                calls.append(
                    Call(
                        sequence,
                        returned,
                        ready,
                        time.monotonic_ns() >= stop,
                        execution_id=str(result.execution_id),
                        outcome=outcome,
                        lateness_ns=lateness,
                        due_ns=offset,
                        woke_ns=woke,
                    ).encode()
                )
            return calls

        collections: list[tuple[int, int]] = []
        started: list[int] = []

        def collected(phase: str, _info: dict[str, int]) -> None:
            if phase == "start":
                started.append(time.monotonic_ns())
            elif started:
                begun = started.pop()
                collections.append((begun, time.monotonic_ns() - begun))

        before = readout()
        futures = [self.workers.submit(client, index) for index in range(clients)]
        gc.callbacks.append(collected)
        try:
            start.wait()
            calls = [call for future in futures for call in future.result()]
        finally:
            gc.callbacks.remove(collected)
        after = readout()
        opened = window["opened"]
        timings = None if before is None else {"before": before, "after": after}
        window_collections = [
            {"start_ns": begun - opened, "duration_ns": duration}
            for begun, duration in collections
            if begun >= opened
        ]
        return (
            f'{{"calls": [{",".join(calls)}], "timings": {json.dumps(timings)}, '
            f'"collections": {json.dumps(window_collections)}}}'
        )


def lines(stream: Any) -> Iterator[dict[str, Any]]:
    """Each JSON command line of ``stream`` until it closes."""
    for line in stream:
        if line.strip():
            yield json.loads(line)


def answer(message: dict[str, Any]) -> None:
    """Write one JSON answer line to the controller."""
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def main() -> None:
    """Serve the controller's configuration and window commands until stdin closes."""
    commands = lines(sys.stdin)
    driver = Driver(next(commands))
    answer({"ready": driver.identity()})
    for command in commands:
        if "heap" in command:
            answer({"heap": capture_heap(Path(command["heap"]["stem"]))})
            continue
        run = command["run"]
        done = driver.run(run["clients"], run["seconds"], run.get("rate"))
        sys.stdout.write(f'{{"done": {done}}}\n')
        sys.stdout.flush()
    driver.workers.shutdown()


if __name__ == "__main__":
    main()
