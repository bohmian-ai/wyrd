"""Python packaging coverage for the shared Rust CLI implementation."""

import json
import sys

from wyrd.cli import run_wyrd_cli


def _run(monkeypatch, args: list[str]) -> int:
    """Invoke the packaged no-argument entrypoint with an isolated argv."""
    monkeypatch.setattr(sys, "argv", args)
    return run_wyrd_cli()


def test_packaged_cli_preserves_help_and_usage_codes(monkeypatch) -> None:
    """The installed entrypoint preserves success and usage outcomes."""
    assert _run(monkeypatch, ["wyrd", "--help"]) == 0
    assert _run(monkeypatch, ["wyrd", "dev", "bootstrap"]) == 64


def test_packaged_cli_preserves_runtime_codes(tmp_path, monkeypatch) -> None:
    """Shared dispatch preserves generic failure and evaluation mismatch codes."""
    spec = tmp_path / "eval.json"
    spec.write_text(
        json.dumps(
            {
                "apiVersion": "wyrd/v1",
                "kind": "Eval",
                "metadata": {
                    "name": "cli-exit",
                    "version": "1.0.0",
                    "space": "tests",
                    "labels": {},
                    "annotations": {},
                },
                "spec": {
                    "tasks": {
                        "ok_check": {
                            "kind": "assertion",
                            "id": "ok_check",
                            "context_path": "$.ok",
                            "operator": "equals",
                            "expected": True,
                        }
                    },
                    "pass_gate": {"kind": "all_pass"},
                },
                "relationships": {
                    "outbound": [],
                    "outbound_refs": [],
                    "inbound": [],
                    "inbound_refs": [],
                },
            }
        ),
        encoding="utf-8",
    )
    common = [
        "wyrd",
        "eval",
        "run",
        "--eval",
        str(spec),
        "--subject",
        "tests/Agent/agent-under-test@1.0.0",
        "--out",
        str(tmp_path / "out"),
    ]
    assert (
        _run(
            monkeypatch,
            [*common, "--records", str(tmp_path / "missing-records.jsonl")],
        )
        == 1
    )

    records = tmp_path / "records.jsonl"
    records.write_text(
        json.dumps(
            {
                "record_id": "00000000-0000-0000-0000-000000000002",
                "run_id": "run-records",
                "eval_ref": {
                    "kind": "Eval",
                    "name": "cli-exit",
                    "version": "1.0.0",
                    "space": "tests",
                },
                "context": {"ok": False},
                "created_at": "2026-01-01T00:00:00Z",
            }
        )
        + "\n",
        encoding="utf-8",
    )
    assert (
        _run(
            monkeypatch,
            [
                *common,
                "--records",
                str(records),
            ],
        )
        == 2
    )
