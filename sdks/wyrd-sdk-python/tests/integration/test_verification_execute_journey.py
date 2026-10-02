"""Python direct verification journey through ``Verification.execute``.

Registers a Parquet baseline Data Card, PSI and SPC Verifiers fitted from it,
a Custom Drift Verifier, an assertion-only Eval Verifier, and an LLM-judge Eval
Verifier whose JSON-schema Prompt is answered by a local OpenAI-shaped mock.
Each Verifier judges supplied input directly against an unbound Service and
returns passed, failed, or inconclusive judgments attributed to the exact
registered Card versions. Refusals cover an unfitted and a legacy baseline, a
caller without ``evals:run``, a Card-bound caller outside its scope, another
tenant, malformed, oversized, incompatible, and unsupported input, and a failing
judge provider. No execution creates a verification run. A second journey holds
the judge past the 60-second server deadline.
"""

from __future__ import annotations

import json
import threading
import time
from collections.abc import Iterator
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
from wyrd.verification import Verification

from .test_drift_journey import (
    EDGE_VERIFIERS,
    JUDGE_REPLY,
    LATENCY,
    WAIT_SECONDS,
    download,
    eval_verifier,
    subject,
    subject_credential,
    verifier_yaml,
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
    "          expected: {passed: true}\n          max_retries: RETRIES\n"
)
TRACE_TASK = (
    "        trace: {kind: trace_assertion, id: trace, span_selector: '$.spans[0].name', "
    "operator: equals, expected: x}\n"
)


@contextmanager
def judge_provider() -> Iterator[dict[str, Any]]:
    """Serve an OpenAI-shaped judge whose behavior the journey switches.

    ``mode["reply"]`` is ``"pass"`` for a passing structured verdict,
    ``"fail"`` for an HTTP 500, or ``"slow"`` to answer after 65 seconds.
    Yields the mode dictionary, whose ``"url"`` is the provider root.
    """
    mode: dict[str, Any] = {"reply": "pass", "calls": 0}

    class Handler(BaseHTTPRequestHandler):
        """Answer one chat completion according to the current mode."""

        def do_POST(self) -> None:
            self.rfile.read(int(self.headers["content-length"] or 0))
            mode["calls"] += 1
            if mode["reply"] == "slow":
                time.sleep(65)
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


def register_judge(cards: Cards, root: Path, retries: int = 0) -> CardRef:
    """Register the LLM-judge Eval Verifier over a JSON-schema judge Prompt.

    ``retries`` is the judge task's ``max_retries``.
    """
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
    return register(
        cards,
        root,
        "py-exec-judge",
        eval_verifier("py-exec-judge", JUDGE_TASK.replace("RETRIES", str(retries))),
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


def request(verifier: CardRef, target: CardRef, input: dict[str, Any]) -> dict[str, Any]:
    """Build an execute request of ``verifier`` over ``target``."""
    return {"verifier_uid": str(verifier.uid), "subject_card_uid": str(target.uid), "input": input}


def samples(**columns: list[Any]) -> dict[str, Any]:
    """Drift samples input with one column per keyword."""
    return {"kind": "drift_samples", "columns": columns}


def record(**context: Any) -> dict[str, Any]:
    """Eval record input carrying ``context``."""
    return {"kind": "eval_record", "context": context}


def refused(verification: Verification, body: dict[str, Any]) -> str:
    """Execute ``body`` expecting a refusal and return its stable code."""
    with pytest.raises(WyrdError) as error:
        verification.execute(body)
    return error.value.code


@pytest.mark.integration
def test_verifiers_judge_supplied_input_directly(tmp_path: Path) -> None:
    """Every Verifier kind judges supplied input with exact attribution and no durable run."""
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
        psi = register(
            cards, tmp_path, "py-exec-psi", verifier_yaml("py-exec-psi", baseline, "Psi")
        )
        spc = register(
            cards, tmp_path, "py-exec-spc", verifier_yaml("py-exec-spc", baseline, "Spc")
        )
        legacy = register(
            cards, tmp_path, "py-exec-legacy", verifier_yaml("py-exec-legacy", baseline, "Psi")
        )
        # SPC cannot fit the text-valued tier column, so its baseline never becomes ready.
        unfitted = register(
            cards,
            tmp_path,
            "py-exec-unfitted",
            verifier_yaml("py-exec-unfitted", baseline, "Spc").replace("[latency]", "[tier]"),
        )
        custom = register(
            cards,
            tmp_path,
            "py-exec-custom",
            "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: py-exec-custom\n"
            "  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n"
            f"    spec:\n{EDGE_VERIFIERS['py-edge-custom']}",
        )
        asserted = register(
            cards, tmp_path, "py-exec-assert", eval_verifier("py-exec-assert", ASSERT_TASK)
        )
        traced = register(
            cards, tmp_path, "py-exec-trace", eval_verifier("py-exec-trace", TRACE_TASK)
        )
        judged = register_judge(cards, tmp_path)
        target = subject(cards, tmp_path, "py-exec-subject")
        status = tmp_path / "status"
        for verifier in (psi, spc, legacy):
            wait_baseline(server, admin, verifier, status, "ready")
        wait_baseline(server, admin, unfitted, status, "failed")
        server.retire_fitted_format(str(legacy.uid))

        verification = Verification(
            server_url=server.base_url, credential=subject_credential(server, target)
        )
        steady = [float((row * 37) % 100) for row in range(100)]
        shifted = [150.0 + row for row in range(120)]
        calm = [48.0, 49.0, 50.0, 51.0, 52.0] * 2
        cases = [
            (psi, samples(latency=steady), "drift_psi", "passed"),
            (psi, samples(latency=shifted), "drift_psi", "failed"),
            (psi, samples(latency=[*steady[:-1], None]), "drift_psi", "inconclusive"),
            (spc, samples(latency=calm), "drift_spc", "passed"),
            (spc, samples(latency=shifted), "drift_spc", "failed"),
            (custom, samples(score=[1.0, 2.0]), "drift_custom", "passed"),
            (custom, samples(score=[5.0, 5.0]), "drift_custom", "failed"),
            (asserted, record(answer="yes"), "eval_assertion", "passed"),
            (asserted, record(answer="no"), "eval_assertion", "failed"),
            (judged, record(answer="yes"), "eval_llm_judge", "passed"),
        ]
        for verifier, input, kind, verdict in cases:
            response = verification.execute(request(verifier, target, input))
            assert (response["kind"], response["verdict"]) == (kind, verdict), response
            for served, registered in (
                (response["verifier"], verifier),
                (response["subject"], target),
            ):
                assert (served["uid"], served["version"]) == (
                    str(registered.uid),
                    registered.version,
                ), response
            assert list(response["detail"]) == [kind.split("_")[0]], response
            assert response["execution_id"], response
        assert judge_mode["calls"] == 1, "the judge called the local provider once"

        judge_mode["reply"] = "fail"
        wide = {f"c{index}": [1.0] for index in range(65)}
        refusals = [
            (request(unfitted, target, samples(tier=["gold"] * 5)), "409_BASELINE_NOT_READY"),
            (request(legacy, target, samples(latency=steady)), "409_BASELINE_LEGACY"),
            ({**request(custom, target, samples(score=[1.0])), "x": 1}, "400_INPUT_INVALID"),
            (request(custom, target, samples(score=[1.0, "a"])), "400_INPUT_INVALID"),
            (request(custom, target, samples(**wide)), "413_INPUT_TOO_LARGE"),
            (request(custom, target, samples(score=[1.0] * 100_001)), "413_INPUT_TOO_LARGE"),
            (request(asserted, target, record(answer="a" * 256 * 1024)), "413_INPUT_TOO_LARGE"),
            (request(psi, target, record(answer="yes")), "422_INPUT_INCOMPATIBLE"),
            (request(psi, target, samples(other=steady)), "422_INPUT_INCOMPATIBLE"),
            (request(asserted, target, record(other="yes")), "422_INPUT_INCOMPATIBLE"),
            (request(traced, target, record(answer="yes")), "422_INPUT_UNSUPPORTED"),
            (request(judged, target, record(answer="yes")), "502_DEPENDENCY_FAILED"),
            (request(target, target, record(answer="yes")), "404_TARGET_NOT_FOUND"),
        ]
        for body, code in refusals:
            assert refused(verification, body) == f"WYRD_VERIFICATION_{code}", body

        allowed = request(asserted, target, record(answer="yes"))
        reader = Verification(
            server_url=server.base_url,
            credential=server.bootstrap_service(["reader"], name="py-exec-reader"),
        )
        assert refused(reader, allowed) == "WYRD_PERMISSION_403_DENIED_RBAC"
        other = subject(cards, tmp_path, "py-exec-other")
        bound = Verification(
            server_url=server.base_url,
            credential=subject_credential(server, other),
        )
        assert refused(bound, allowed) == "WYRD_PERMISSION_403_DENIED_RBAC"
        assert bound.execute(request(asserted, other, record(answer="yes")))["verdict"] == "passed"
        foreign = Verification(
            server_url=server.base_url,
            credential=server.bootstrap_service_in_tenant(
                server.seed_tenant("py-exec-foreign"), ["admin"], name="py-exec-foreign"
            ),
        )
        assert refused(foreign, allowed) == "WYRD_VERIFICATION_404_TARGET_NOT_FOUND"

        assert server.verification_runs() == [], "direct execution never creates a run"


@pytest.mark.integration
def test_direct_judge_past_the_deadline_times_out(tmp_path: Path) -> None:
    """A judge slower than the 60-second deadline refuses the execution with 504."""
    with (
        judge_provider() as judge_mode,
        WyrdTestServer(verification_runtime=True, provider_base_url=judge_mode["url"]) as server,
    ):
        admin = server.bootstrap_service(["admin"], name="py-exec-slow-admin")
        cards = Cards(server_url=server.base_url, credential=admin)
        # Each judge attempt hits the 30-second provider timeout; three exceed the deadline.
        judged = register_judge(cards, tmp_path, retries=2)
        target = subject(cards, tmp_path, "py-exec-slow-subject")
        judge_mode["reply"] = "slow"
        verification = Verification(
            server_url=server.base_url, credential=subject_credential(server, target)
        )
        body = request(judged, target, record(answer="yes"))
        assert refused(verification, body) == "WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT"
        assert server.verification_runs() == [], "a timed-out execution leaves no run"
