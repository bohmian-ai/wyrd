"""Fixed client-only workloads for macOS Instruments Allocations recordings.

Each workload exercises one operation family offline and checks its result,
mirroring the Rust and TypeScript workloads:

- ``authoring chain8|chain32|shared``: capture and validate a Service chain of
  8 or 32 Cards, or 32 parents sharing one child, each rooted at a 64 KiB
  Prompt.
- ``hydration BUNDLE``: hydrate the prepared state bundle at ``BUNDLE``.
- ``access typed|agent|run BUNDLE``: hydrate once, then repeat 1,000 typed
  lookups, Agent/Prompt navigations, or run/Card-view openings.

Run from ``sdks/wyrd-sdk-python`` with ``python -m tests.profile_client``.
"""

import sys
from pathlib import Path

from wyrd.state import WyrdState
from wyrd.verify import Agent, Component, Service

from tests.unit.state.support import TinyDataInterface, TinyModelInterface

ACCESS_ITERATIONS = 1_000


def agent() -> Agent:
    payload = "x" * (64 * 1024)
    return Agent(
        "profile-agent",
        "1.0.0",
        prompt={
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": f"{payload} {{{{report}}}}"}],
        },
        execution_route={"kind": "wyrd_gateway", "deployment": "openai"},
    )


def chain(length: int) -> Service:
    service = Service("chain-0", "1.0.0", components=[Component("agent", agent())])
    for index in range(1, length):
        service = Service(f"chain-{index}", "1.0.0", components=[Component("inner", service)])
    return service


def shared_child() -> Service:
    child = Service("child", "1.0.0", components=[Component("agent", agent())])
    return Service(
        "parents",
        "1.0.0",
        components=[
            Component(
                f"parent-{index}",
                Service(f"parent-{index}", "1.0.0", components=[Component("child", child)]),
            )
            for index in range(32)
        ],
    )


def hydrate(bundle: str) -> WyrdState:
    return WyrdState.from_path(
        Path(bundle),
        interfaces={
            "model": TinyModelInterface(),
            "backup": TinyModelInterface(),
            "training_data": TinyDataInterface(),
        },
    )


def typed(state: WyrdState) -> None:
    for _ in range(ACCESS_ITERATIONS):
        assert state.prompt("triage_prompt") is state.get("triage_prompt")
        state.model("model")
        state.verifier("ok_check")


def agent_navigation(state: WyrdState) -> None:
    prompt = state.prompt("triage_prompt")
    for _ in range(ACCESS_ITERATIONS):
        assert state.agent("agent_triage").prompt() is prompt
        assert state.get("agent_triage") is state.agent("agent_triage")


def runs(state: WyrdState) -> None:
    for _ in range(ACCESS_ITERATIONS):
        run = state.run("agent_triage")
        assert run.for_card("triage_prompt").run_id == run.run_id


def main(arguments: list[str]) -> None:
    match arguments:
        case ["authoring", "chain8"]:
            chain(8).validate()
        case ["authoring", "chain32"]:
            chain(32).validate()
        case ["authoring", "shared"]:
            shared_child().validate()
        case ["hydration", bundle]:
            hydrate(bundle)
        case ["access", "typed", bundle]:
            typed(hydrate(bundle))
        case ["access", "agent", bundle]:
            agent_navigation(hydrate(bundle))
        case ["access", "run", bundle]:
            runs(hydrate(bundle))
        case _:
            raise SystemExit(
                "usage: profile_client authoring chain8|chain32|shared | hydration BUNDLE"
                " | access typed|agent|run BUNDLE"
            )


if __name__ == "__main__":
    main(sys.argv[1:])
