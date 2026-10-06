"""Python direct verification journey through ``run.observe.verify``.

Registers a Parquet baseline Data Card, PSI and SPC Verifiers fitted from it,
a Custom Drift Verifier, an assertion-only Eval Verifier, and an LLM-judge Eval
Verifier whose JSON-schema Prompt is answered by a local OpenAI-shaped mock.
A Service binds every Verifier at its root, and its hydrated bundle's root Run
view judges supplied input directly with each one, returning passed, failed,
or inconclusive judgments attributed to the exact registered Card versions.
Refusals cover an unfitted baseline, a caller without ``evals:run``, a
Card-bound caller outside its scope, another tenant, an unbound Verifier name,
malformed, oversized, incompatible, and unsupported input, and a failing judge
provider.
"""

from __future__ import annotations

import json
import threading
import time
from collections.abc import Iterator, Sequence
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

import polars as pl
import pytest
from wyrd import WyrdError
from wyrd.cards import CardRef, Cards
from wyrd.data import DataCard, PolarsInterface
from wyrd.prompt import Prompt
from wyrd.testing import WyrdTestServer

from .test_drift_journey import (
    CUSTOM_DRIFT,
    JUDGE_REPLY,
    LATENCY,
    WAIT_SECONDS,
    client_environment,
    download,
    eval_verifier,
    subject_credential,
    verifier_yaml,
    verify_as,
)

ASSERT_TASK = (
    "        answer: {kind: assertion, id: answer, context_path: $.answer, "
    'operator: equals, expected: "yes"}\n'
)
JUDGE_TASK = (
    "        judge:\n          kind: llm_judge\n          id: judge\n"
    "          judge_ref: {prompt: ./judge-prompt.json, tool_names: [], "
    "run_config: {max_iterations: 1}}\n"
    "          context_path: $.answer\n          operator: equals\n"
    "          expected: {passed: true}\n          max_retries: 0\n"
)
TRACE_TASK = (
    "        trace: {kind: trace_assertion, id: trace, span_selector: '$.spans[0].name', "
    "operator: equals, expected: x}\n"
)
VERIFIERS = (
    "py-exec-psi",
    "py-exec-spc",
    "py-exec-unfitted",
    "py-exec-custom",
    "py-exec-assert",
    "py-exec-trace",
    "py-exec-judge",
)


@contextmanager
def judge_provider() -> Iterator[dict[str, Any]]:
    """Serve an OpenAI-shaped judge whose behavior the journey switches.

    ``mode["reply"]`` is ``"pass"`` for a passing structured verdict or
    ``"fail"`` for an HTTP 500. Yields the mode dictionary, whose ``"url"`` is
    the provider root.
    """
    mode: dict[str, Any] = {"reply": "pass", "calls": 0}

    class Handler(BaseHTTPRequestHandler):
        """Answer one chat completion according to the current mode."""

        def do_POST(self) -> None:
            self.rfile.read(int(self.headers["content-length"] or 0))
            mode["calls"] += 1
            failing = mode["reply"] == "fail"
            self.send_response(500 if failing else 200)
            self.send_header("content-type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps({"error": "down"} if failing else JUDGE_REPLY).encode())

        def log_message(self, *args: object) -> None:
            """Keep the test output free of per-request access lines."""

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    mode["url"] = f"http://127.0.0.1:{server.server_address[1]}"
    try:
        yield mode
    finally:
        server.shutdown()
        thread.join()


def register(cards: Cards, root: Path, name: str, body: str) -> CardRef:
    """Write ``body`` as ``name.yaml`` under ``root`` and register it."""
    path = root / f"{name}.yaml"
    path.write_text(body, encoding="utf-8")
    return cards.register_from_path(str(path)).root


def register_judge(cards: Cards, root: Path) -> CardRef:
    """Register the LLM-judge Eval Verifier over a JSON-schema judge Prompt."""
    judge = Prompt.openai_chat(
        "gpt-test",
        messages=["Grade the answer ${answer}."],
        variables=["answer"],
        output={"passed": bool},
    )
    card = {
        "apiVersion": "wyrd/v1",
        "kind": "Prompt",
        "metadata": {"name": "py-exec-judge-prompt", "version": "1.0.0", "space": "default"},
        "spec": json.loads(judge.model_dump_json()),
    }
    (root / "judge-prompt.json").write_text(json.dumps(card), encoding="utf-8")
    return register(cards, root, "py-exec-judge", eval_verifier("py-exec-judge", JUDGE_TASK))


def bound_subject(cards: Cards, root: Path, name: str, verifiers: Sequence[str]) -> CardRef:
    """Register a Service named ``name`` whose root binds each of ``verifiers``.

    Each binding runs on a daily schedule with no Operator, so the journey's
    direct judgments are the only executions it observes.
    """
    bindings = "".join(
        f"    - verifier: {{kind: Verifier, name: {verifier}, version: 1.0.0, space: default}}\n"
        '      runs_on: {kind: schedule, cron: "0 0 * * *"}\n'
        for verifier in verifiers
    )
    return register(
        cards,
        root,
        name,
        f"apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: {name}\n"
        f"  version: 1.0.0\n  space: default\nspec:\n  verified_by:\n{bindings}",
    )


def wait_baseline(server: WyrdTestServer, admin: str, verifier: CardRef, root: Path, want: str):
    """Poll a Verifier's served baseline state until it reaches ``want``."""
    deadline = time.monotonic() + WAIT_SECONDS
    for attempt in range(10_000):
        status = download(server, admin, "Verifier", str(verifier.uid), root / f"{attempt}")
        state = status["verification"]["baseline"]["state"]
        if state == want:
            return
        assert time.monotonic() < deadline, f"{verifier.name} stayed {state}, wanted {want}"
        time.sleep(0.2)


def refused(bundle: Path, credential: str, verifier: str, input: Any) -> str:
    """Judge as ``verify_as`` does, expecting a refusal, and return its stable code."""
    with pytest.raises(WyrdError) as error:
        verify_as(bundle, credential, verifier, input)
    return error.value.code


def rows(**columns: Sequence[Any]) -> list[dict[str, Any]]:
    """Drift feature rows from equal-length ``columns``; a ``None`` sample is omitted."""
    return [
        {name: value for name, value in zip(columns, row, strict=True) if value is not None}
        for row in zip(*columns.values(), strict=True)
    ]


@pytest.mark.integration
def test_verifiers_judge_supplied_input_directly(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Every Verifier kind judges supplied input through ``observe.verify`` with exact attribution."""
    with (
        judge_provider() as judge_mode,
        WyrdTestServer(verification_runtime=True, provider_base_url=judge_mode["url"]) as server,
    ):
        admin = server.bootstrap_service(["admin"], name="py-exec-admin")
        cards = Cards(server_url=server.base_url, credential=admin)
        baseline = cards.data.register(
            DataCard(
                PolarsInterface(data=pl.DataFrame({"latency": LATENCY, "tier": ["gold"] * 100})),
                space="default",
                name="py-exec-data",
                version="1.0.0",
            )
        ).root
        registered = {
            "py-exec-psi": register(
                cards, tmp_path, "py-exec-psi", verifier_yaml("py-exec-psi", baseline, "Psi")
            ),
            "py-exec-spc": register(
                cards, tmp_path, "py-exec-spc", verifier_yaml("py-exec-spc", baseline, "Spc")
            ),
            # SPC cannot fit the text-valued tier column, so its baseline never becomes ready.
            "py-exec-unfitted": register(
                cards,
                tmp_path,
                "py-exec-unfitted",
                verifier_yaml("py-exec-unfitted", baseline, "Spc").replace("[latency]", "[tier]"),
            ),
            "py-exec-custom": register(
                cards,
                tmp_path,
                "py-exec-custom",
                "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: py-exec-custom\n"
                "  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n"
                f"    spec:\n{CUSTOM_DRIFT}",
            ),
            "py-exec-assert": register(
                cards, tmp_path, "py-exec-assert", eval_verifier("py-exec-assert", ASSERT_TASK)
            ),
            "py-exec-trace": register(
                cards, tmp_path, "py-exec-trace", eval_verifier("py-exec-trace", TRACE_TASK)
            ),
            "py-exec-judge": register_judge(cards, tmp_path),
        }
        status = tmp_path / "status"
        for name in ("py-exec-psi", "py-exec-spc"):
            wait_baseline(server, admin, registered[name], status, "ready")
        wait_baseline(server, admin, registered["py-exec-unfitted"], status, "failed")
        target = bound_subject(cards, tmp_path, "py-exec-subject", VERIFIERS)
        bundle = tmp_path / "bundle"
        download(server, admin, "Service", str(target.uid), bundle)
        client_environment(monkeypatch, tmp_path)
        credential = subject_credential(server, target)

        steady = [float((row * 37) % 100) for row in range(100)]
        shifted = [150.0 + row for row in range(120)]
        calm = [48.0, 49.0, 50.0, 51.0, 52.0] * 2
        cases: list[tuple[str, Any, str, str]] = [
            ("py-exec-psi", rows(latency=steady), "drift_psi", "passed"),
            ("py-exec-psi", rows(latency=shifted), "drift_psi", "failed"),
            (
                "py-exec-psi",
                rows(latency=[*steady[:-1], None], tier=["gold"] * 100),
                "drift_psi",
                "inconclusive",
            ),
            ("py-exec-spc", rows(latency=calm), "drift_spc", "passed"),
            ("py-exec-spc", rows(latency=shifted), "drift_spc", "failed"),
            ("py-exec-custom", rows(score=[1.0, 2.0]), "drift_custom", "passed"),
            ("py-exec-custom", rows(score=[5.0, 5.0]), "drift_custom", "failed"),
            ("py-exec-assert", {"answer": "yes"}, "eval_assertion", "passed"),
            ("py-exec-assert", {"answer": "no"}, "eval_assertion", "failed"),
            ("py-exec-judge", {"answer": "yes"}, "eval_llm_judge", "passed"),
        ]
        for name, input, kind, verdict in cases:
            judgment = verify_as(bundle, credential, name, input)
            label = (name, verdict, judgment.summary)
            assert (judgment.kind, judgment.verdict) == (kind, verdict), label
            assert judgment.passed == (verdict == "passed"), label
            for served, expected in (
                (judgment.verifier, registered[name]),
                (judgment.subject, target),
            ):
                assert (str(served.uid), served.version) == (
                    str(expected.uid),
                    expected.version,
                ), label
            assert list(judgment.detail) == [kind.split("_")[0]], label
            assert judgment.execution_id, label
        assert judge_mode["calls"] == 1, "the judge called the local provider once"

        judge_mode["reply"] = "fail"
        wide = {f"c{index}": [1.0] for index in range(65)}
        refusals: list[tuple[str, Any, str]] = [
            (
                "py-exec-unfitted",
                rows(tier=["gold"] * 5),
                "WYRD_VERIFICATION_409_BASELINE_NOT_READY",
            ),
            ("py-exec-custom", rows(score=[1.0, "a"]), "WYRD_VERIFICATION_400_INPUT_INVALID"),
            ("py-exec-custom", rows(**wide), "WYRD_VERIFICATION_413_INPUT_TOO_LARGE"),
            (
                "py-exec-custom",
                rows(score=[1.0] * 100_001),
                "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
            ),
            (
                "py-exec-assert",
                {"answer": "a" * 256 * 1024},
                "WYRD_VERIFICATION_413_INPUT_TOO_LARGE",
            ),
            ("py-exec-psi", {"answer": "yes"}, "WYRD_SDK_400_INVALID_OBSERVATION"),
            ("py-exec-psi", rows(other=steady), "WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE"),
            ("py-exec-assert", {"other": "yes"}, "WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE"),
            ("py-exec-trace", {"answer": "yes"}, "WYRD_VERIFICATION_422_INPUT_UNSUPPORTED"),
            ("py-exec-judge", {"answer": "yes"}, "WYRD_VERIFICATION_502_DEPENDENCY_FAILED"),
            ("py-exec-subject", {"answer": "yes"}, "WYRD_SDK_404_UNKNOWN_VERIFIER"),
        ]
        for name, input, code in refusals:
            assert refused(bundle, credential, name, input) == code, name

        allowed = {"answer": "yes"}
        reader = server.bootstrap_service(["reader"], name="py-exec-reader")
        assert (
            refused(bundle, reader, "py-exec-assert", allowed) == "WYRD_PERMISSION_403_DENIED_RBAC"
        )
        other = bound_subject(cards, tmp_path, "py-exec-other", ["py-exec-assert"])
        bound = subject_credential(server, other)
        assert (
            refused(bundle, bound, "py-exec-assert", allowed) == "WYRD_PERMISSION_403_DENIED_RBAC"
        )
        other_bundle = tmp_path / "other-bundle"
        download(server, admin, "Service", str(other.uid), other_bundle)
        own = verify_as(other_bundle, bound, "py-exec-assert", allowed)
        assert own.verdict == "passed", own.summary
        foreign = server.bootstrap_service_in_tenant(
            server.seed_tenant("py-exec-foreign"), ["admin"], name="py-exec-foreign"
        )
        assert (
            refused(bundle, foreign, "py-exec-assert", allowed)
            == "WYRD_VERIFICATION_404_TARGET_NOT_FOUND"
        )
