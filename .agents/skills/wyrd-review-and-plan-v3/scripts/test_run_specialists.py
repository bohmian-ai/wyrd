"""Tests for bounded external Codex specialist execution."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from types import ModuleType

import pytest


SCRIPT = Path(__file__).with_name("run_specialists.py")


def load_runner() -> ModuleType:
    """Load the runner script as a testable module."""
    spec = importlib.util.spec_from_file_location("run_specialists", SCRIPT)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def current_target() -> str:
    """Return the repository commit used by packet-validation tests."""
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=Path(__file__).resolve().parents[4],
        capture_output=True,
        check=True,
        text=True,
    )
    return result.stdout.strip()


def test_build_command_routes_specialists_to_terra_high(tmp_path: Path) -> None:
    """Every external review specialist uses the agreed fixed model policy."""
    runner = load_runner()

    command = runner.build_command(
        codex_bin="codex",
        repository_root=tmp_path,
        output_schema=tmp_path / "schema.json",
        output_path=tmp_path / "result.json",
    )

    assert command[command.index("--model") + 1] == "gpt-5.6-terra"
    assert command[command.index("--config") + 1] == ('model_reasoning_effort="high"')
    assert "--ephemeral" in command
    assert command[command.index("--sandbox") + 1] == "read-only"
    assert "--ask-for-approval" not in command


def test_run_assignment_publishes_validated_report(tmp_path: Path) -> None:
    """Only schema-valid evidence bound to the assignment reaches review output."""
    runner = load_runner()
    prompt = tmp_path / "specialist.md"
    prompt.write_text("Review correctness.\n", encoding="utf-8")
    input_path = tmp_path / "assignment.txt"
    input_path.write_text("Immutable assignment packet.\n", encoding="utf-8")
    report_path = tmp_path / "review" / "evidence" / "specialists" / "baseline.md"
    prompt_digest = runner.sha256_file(prompt)
    assignment = runner.Assignment(
        assignment_id="baseline-correctness",
        domain="correctness",
        reviewer_id="correctness-session",
        target_sha="abcdef1",
        prompt_path=prompt,
        prompt_sha256=prompt_digest,
        input_path=input_path,
        input_sha256=runner.sha256_file(input_path),
        report_path=report_path,
    )
    result = {
        "assignment_id": assignment.assignment_id,
        "domain": assignment.domain,
        "reviewer_id": assignment.reviewer_id,
        "target_sha": assignment.target_sha,
        "prompt_sha256": assignment.prompt_sha256,
        "status": "completed",
        "candidate_ids": [],
        "report_markdown": "# Clean specialist report",
        "static_limits": [],
    }
    fake_codex = tmp_path / "fake-codex"
    fake_codex.write_text(
        "#!/usr/bin/env python3\n"
        "import json, pathlib, sys\n"
        "args = sys.argv[1:]\n"
        "output = pathlib.Path(args[args.index('--output-last-message') + 1])\n"
        f"output.write_text({json.dumps(json.dumps(result))}, encoding='utf-8')\n",
        encoding="utf-8",
    )
    fake_codex.chmod(fake_codex.stat().st_mode | 0o111)

    outcome = runner.run_assignment(
        assignment,
        repository_root=tmp_path,
        output_schema=tmp_path / "schema.json",
        codex_bin=str(fake_codex),
        timeout_seconds=5,
        retries=0,
    )

    assert outcome.assignment_id == assignment.assignment_id
    assert outcome.attempts == 1
    assert report_path.read_text(encoding="utf-8") == "# Clean specialist report\n"


def test_build_specialist_prompt_binds_verified_lens_to_context(tmp_path: Path) -> None:
    """A packet cannot replace the reviewed lens with only assignment context."""
    runner = load_runner()
    prompt = tmp_path / "specialist.md"
    prompt.write_text("Lens instructions.\n", encoding="utf-8")
    context = tmp_path / "assignment.txt"
    context.write_text("Immutable context.\n", encoding="utf-8")
    assignment = runner.Assignment(
        assignment_id="baseline-correctness",
        domain="correctness",
        reviewer_id="correctness-session",
        target_sha="abcdef1",
        prompt_path=prompt,
        prompt_sha256=runner.sha256_file(prompt),
        input_path=context,
        input_sha256=runner.sha256_file(context),
        report_path=tmp_path / "report.md",
    )

    assert runner.build_specialist_prompt(assignment) == (
        "Lens instructions.\n\n## Immutable assignment context\n\nImmutable context.\n"
    )


def test_validate_result_rejects_identity_mismatch() -> None:
    """A session cannot satisfy another assignment by changing result identity."""
    runner = load_runner()
    assignment = runner.Assignment(
        assignment_id="baseline-security",
        domain="security",
        reviewer_id="security-session",
        target_sha="abcdef1",
        prompt_path=Path("prompt.md"),
        prompt_sha256="a" * 64,
        input_path=Path("input.md"),
        input_sha256="b" * 64,
        report_path=Path("report.md"),
    )
    result = {
        "assignment_id": "baseline-correctness",
        "domain": assignment.domain,
        "reviewer_id": assignment.reviewer_id,
        "target_sha": assignment.target_sha,
        "prompt_sha256": assignment.prompt_sha256,
        "status": "completed",
        "candidate_ids": [],
        "report_markdown": "report",
        "static_limits": [],
    }

    with pytest.raises(runner.IdentityError, match="mismatched assignment_id"):
        runner.validate_result(result, assignment)


def test_load_packet_rejects_duplicate_reviewer_identity(tmp_path: Path) -> None:
    """Independent assignments cannot share one external Codex reviewer identity."""
    runner = load_runner()
    repository_root = Path(__file__).resolve().parents[4]
    review_dir = tmp_path / "review"
    specialist_dir = review_dir / "evidence" / "specialists"
    specialist_dir.mkdir(parents=True)
    prompt = tmp_path / "prompt.md"
    prompt.write_text("prompt\n", encoding="utf-8")
    input_path = tmp_path / "input.md"
    input_path.write_text("input\n", encoding="utf-8")
    common = {
        "domain": "correctness",
        "reviewer_id": "same-session",
        "target_sha": current_target(),
        "prompt_path": str(prompt),
        "prompt_sha256": runner.sha256_file(prompt),
        "input_path": str(input_path),
        "input_sha256": runner.sha256_file(input_path),
    }
    packet = {
        "schema_version": 1,
        "review_id": "review-1",
        "repository_root": str(repository_root),
        "review_dir": str(review_dir),
        "assignments": [
            {
                **common,
                "assignment_id": "baseline-correctness",
                "report_path": str(specialist_dir / "correctness.md"),
            },
            {
                **common,
                "assignment_id": "baseline-security",
                "report_path": str(specialist_dir / "security.md"),
            },
        ],
    }
    packet_path = tmp_path / "packet.json"
    packet_path.write_text(json.dumps(packet), encoding="utf-8")

    with pytest.raises(runner.PacketError, match="reviewer IDs must be unique"):
        runner.load_packet(packet_path)
