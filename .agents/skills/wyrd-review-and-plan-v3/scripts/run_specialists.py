#!/usr/bin/env python3
"""Run immutable Wyrd review assignments in bounded Codex CLI sessions."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SPECIALIST_MODEL = "gpt-5.6-terra"
SPECIALIST_REASONING = "high"
DEFAULT_MAX_PARALLEL = 7
DEFAULT_TIMEOUT_SECONDS = 1_800
DEFAULT_RETRIES = 1
ASSIGNMENT_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
CANDIDATE_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*/C\d{3}$")


class RunnerError(Exception):
    """Base failure raised by the specialist subprocess runner."""


class PacketError(RunnerError):
    """The immutable runner packet is malformed or contradictory."""


class SpecialistError(RunnerError):
    """One specialist session failed to produce acceptable evidence."""


class IdentityError(SpecialistError):
    """A specialist result does not match its immutable assignment identity."""


@dataclass(frozen=True)
class Assignment:
    """One immutable specialist invocation and its evidence destination."""

    assignment_id: str
    domain: str
    reviewer_id: str
    target_sha: str
    prompt_path: Path
    prompt_sha256: str
    input_path: Path
    input_sha256: str
    report_path: Path


@dataclass(frozen=True)
class RunnerPacket:
    """Validated execution inputs shared by one bounded specialist run."""

    review_id: str
    repository_root: Path
    review_dir: Path
    assignments: tuple[Assignment, ...]


@dataclass(frozen=True)
class SpecialistOutcome:
    """Accepted result metadata returned to the terminal review root."""

    assignment_id: str
    reviewer_id: str
    report_path: str
    candidate_ids: tuple[str, ...]
    attempts: int


def sha256_file(path: Path) -> str:
    """Return the lowercase SHA-256 digest for one immutable input file."""
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_string(value: Any, *, field: str) -> str:
    """Return a non-empty string or raise a packet validation error."""
    if not isinstance(value, str) or not value.strip():
        raise PacketError(f"`{field}` must be a non-empty string")
    return value


def require_absolute_file(value: Any, *, field: str) -> Path:
    """Resolve one required absolute regular-file path from packet input."""
    path = Path(require_string(value, field=field))
    if not path.is_absolute() or not path.is_file():
        raise PacketError(f"`{field}` must name an existing absolute file: {path}")
    return path.resolve()


def require_absolute_directory(value: Any, *, field: str) -> Path:
    """Resolve one required absolute directory path from packet input."""
    path = Path(require_string(value, field=field))
    if not path.is_absolute() or not path.is_dir():
        raise PacketError(f"`{field}` must name an existing absolute directory: {path}")
    return path.resolve()


def require_digest(path: Path, value: Any, *, field: str) -> str:
    """Verify a packet digest against the current immutable file bytes."""
    expected = require_string(value, field=field).removeprefix("sha256:").lower()
    if not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise PacketError(f"`{field}` must contain a SHA-256 digest")
    actual = sha256_file(path)
    if actual != expected:
        raise PacketError(
            f"`{field}` does not match {path}: expected {expected}, found {actual}"
        )
    return expected


def parse_assignment(
    raw: Any,
    *,
    index: int,
    review_dir: Path,
) -> Assignment:
    """Validate one assignment object and bind its immutable prompt inputs."""
    if not isinstance(raw, dict):
        raise PacketError(f"assignment {index} must be an object")
    prefix = f"assignments[{index}]"
    assignment_id = require_string(
        raw.get("assignment_id"), field=f"{prefix}.assignment_id"
    )
    if ASSIGNMENT_ID.fullmatch(assignment_id) is None:
        raise PacketError(f"invalid assignment ID: {assignment_id}")
    prompt_path = require_absolute_file(
        raw.get("prompt_path"), field=f"{prefix}.prompt_path"
    )
    input_path = require_absolute_file(
        raw.get("input_path"), field=f"{prefix}.input_path"
    )
    report_path = Path(
        require_string(raw.get("report_path"), field=f"{prefix}.report_path")
    )
    if not report_path.is_absolute():
        raise PacketError(f"`{prefix}.report_path` must be absolute")
    report_path = report_path.resolve()
    specialist_root = (review_dir / "evidence" / "specialists").resolve()
    if report_path.parent != specialist_root or report_path.suffix != ".md":
        raise PacketError(
            f"`{prefix}.report_path` must be a direct Markdown child of "
            f"{specialist_root}"
        )
    return Assignment(
        assignment_id=assignment_id,
        domain=require_string(raw.get("domain"), field=f"{prefix}.domain"),
        reviewer_id=require_string(
            raw.get("reviewer_id"), field=f"{prefix}.reviewer_id"
        ),
        target_sha=require_string(raw.get("target_sha"), field=f"{prefix}.target_sha"),
        prompt_path=prompt_path,
        prompt_sha256=require_digest(
            prompt_path,
            raw.get("prompt_sha256"),
            field=f"{prefix}.prompt_sha256",
        ),
        input_path=input_path,
        input_sha256=require_digest(
            input_path,
            raw.get("input_sha256"),
            field=f"{prefix}.input_sha256",
        ),
        report_path=report_path,
    )


def load_packet(path: Path) -> RunnerPacket:
    """Load and validate one complete specialist-runner packet.

    Raises:
        PacketError: If the JSON, paths, digests, or assignment identities are
            invalid.
    """
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PacketError(f"could not read runner packet {path}: {error}") from error
    if not isinstance(raw, dict) or raw.get("schema_version") != 1:
        raise PacketError("runner packet must be an object with schema_version 1")
    repository_root = require_absolute_directory(
        raw.get("repository_root"), field="repository_root"
    )
    review_dir = require_absolute_directory(raw.get("review_dir"), field="review_dir")
    assignments_raw = raw.get("assignments")
    if not isinstance(assignments_raw, list) or not assignments_raw:
        raise PacketError("`assignments` must be a non-empty list")
    assignments = tuple(
        parse_assignment(item, index=index, review_dir=review_dir)
        for index, item in enumerate(assignments_raw)
    )
    assignment_ids = {assignment.assignment_id for assignment in assignments}
    reviewer_ids = {assignment.reviewer_id for assignment in assignments}
    report_paths = {assignment.report_path for assignment in assignments}
    if len(assignment_ids) != len(assignments):
        raise PacketError("assignment IDs must be unique")
    if len(reviewer_ids) != len(assignments):
        raise PacketError("reviewer IDs must be unique")
    if len(report_paths) != len(assignments):
        raise PacketError("report paths must be unique")
    target_shas = {assignment.target_sha for assignment in assignments}
    if len(target_shas) != 1:
        raise PacketError("all assignments must bind the same target SHA")
    verify_target(repository_root, next(iter(target_shas)))
    return RunnerPacket(
        review_id=require_string(raw.get("review_id"), field="review_id"),
        repository_root=repository_root,
        review_dir=review_dir,
        assignments=assignments,
    )


def verify_target(repository_root: Path, target_sha: str) -> None:
    """Require the assignment target to resolve to a committed Git object."""
    result = subprocess.run(
        ["git", "cat-file", "-e", f"{target_sha}^{{commit}}"],
        cwd=repository_root,
        capture_output=True,
        check=False,
        text=True,
    )
    if result.returncode != 0:
        raise PacketError(f"target SHA does not resolve to a commit: {target_sha}")


def build_command(
    *,
    codex_bin: str,
    repository_root: Path,
    output_schema: Path,
    output_path: Path,
) -> list[str]:
    """Build the fixed read-only Terra-high Codex specialist invocation."""
    return [
        codex_bin,
        "exec",
        "--ephemeral",
        "--cd",
        str(repository_root),
        "--sandbox",
        "read-only",
        "--model",
        SPECIALIST_MODEL,
        "--config",
        f'model_reasoning_effort="{SPECIALIST_REASONING}"',
        "--output-schema",
        str(output_schema),
        "--output-last-message",
        str(output_path),
        "-",
    ]


def validate_result(raw: Any, assignment: Assignment) -> dict[str, Any]:
    """Validate a structured specialist result against immutable identity."""
    if not isinstance(raw, dict):
        raise SpecialistError("specialist result must be a JSON object")
    identity = {
        "assignment_id": assignment.assignment_id,
        "domain": assignment.domain,
        "reviewer_id": assignment.reviewer_id,
        "target_sha": assignment.target_sha,
        "prompt_sha256": assignment.prompt_sha256,
    }
    for field, expected in identity.items():
        if raw.get(field) != expected:
            raise IdentityError(
                f"{assignment.assignment_id} returned mismatched {field}: "
                f"expected {expected!r}, found {raw.get(field)!r}"
            )
    if raw.get("status") != "completed":
        raise SpecialistError(
            f"{assignment.assignment_id} returned non-completed status"
        )
    report = raw.get("report_markdown")
    if not isinstance(report, str) or not report.strip():
        raise SpecialistError(f"{assignment.assignment_id} returned an empty report")
    candidate_ids = raw.get("candidate_ids")
    if not isinstance(candidate_ids, list) or any(
        not isinstance(value, str) or CANDIDATE_ID.fullmatch(value) is None
        for value in candidate_ids
    ):
        raise SpecialistError(
            f"{assignment.assignment_id} returned invalid candidate IDs"
        )
    if len(set(candidate_ids)) != len(candidate_ids):
        raise SpecialistError(
            f"{assignment.assignment_id} returned duplicate candidate IDs"
        )
    static_limits = raw.get("static_limits")
    if not isinstance(static_limits, list) or any(
        not isinstance(value, str) for value in static_limits
    ):
        raise SpecialistError(
            f"{assignment.assignment_id} returned invalid static limits"
        )
    return raw


def write_report(path: Path, report: str) -> None:
    """Atomically publish one accepted specialist report."""
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(report.rstrip() + "\n", encoding="utf-8")
    temporary.replace(path)


def build_specialist_prompt(assignment: Assignment) -> str:
    """Bind verified lens instructions to one immutable assignment context."""
    try:
        lens = assignment.prompt_path.read_text(encoding="utf-8").strip()
        context = assignment.input_path.read_text(encoding="utf-8").strip()
    except OSError as error:
        raise SpecialistError(
            f"could not read {assignment.assignment_id} prompt or context: {error}"
        ) from error
    if not lens:
        raise SpecialistError(f"{assignment.assignment_id} has an empty lens prompt")
    if not context:
        raise SpecialistError(
            f"{assignment.assignment_id} has an empty assignment context"
        )
    return f"{lens}\n\n## Immutable assignment context\n\n{context}\n"


def run_assignment(
    assignment: Assignment,
    *,
    repository_root: Path,
    output_schema: Path,
    codex_bin: str,
    timeout_seconds: int,
    retries: int,
) -> SpecialistOutcome:
    """Run one specialist with bounded retries and publish accepted evidence.

    Raises:
        IdentityError: If returned identity contradicts the assignment. This is
            never retried.
        SpecialistError: If every bounded infrastructure or output attempt
            fails.
    """
    prompt = build_specialist_prompt(assignment)
    failures: list[str] = []
    for attempt in range(1, retries + 2):
        with tempfile.TemporaryDirectory(prefix="wyrd-review-specialist-") as temp:
            output_path = Path(temp) / "result.json"
            command = build_command(
                codex_bin=codex_bin,
                repository_root=repository_root,
                output_schema=output_schema,
                output_path=output_path,
            )
            try:
                completed = subprocess.run(
                    command,
                    input=prompt,
                    capture_output=True,
                    check=False,
                    text=True,
                    timeout=timeout_seconds,
                )
            except subprocess.TimeoutExpired:
                failures.append(f"attempt {attempt}: timeout after {timeout_seconds}s")
                continue
            except OSError as error:
                failures.append(f"attempt {attempt}: could not launch Codex: {error}")
                continue
            if completed.returncode != 0:
                excerpt = completed.stderr.strip()[-2_000:]
                failures.append(
                    f"attempt {attempt}: exit {completed.returncode}: {excerpt}"
                )
                continue
            try:
                raw = json.loads(output_path.read_text(encoding="utf-8"))
                result = validate_result(raw, assignment)
            except IdentityError:
                raise
            except (OSError, json.JSONDecodeError, SpecialistError) as error:
                failures.append(f"attempt {attempt}: invalid output: {error}")
                continue
            write_report(assignment.report_path, result["report_markdown"])
            return SpecialistOutcome(
                assignment_id=assignment.assignment_id,
                reviewer_id=assignment.reviewer_id,
                report_path=str(assignment.report_path),
                candidate_ids=tuple(result["candidate_ids"]),
                attempts=attempt,
            )
    raise SpecialistError(
        f"{assignment.assignment_id} exhausted its retry budget: "
        + " | ".join(failures)
    )


def run_packet(
    packet: RunnerPacket,
    *,
    output_schema: Path,
    codex_bin: str,
    max_parallel: int,
    timeout_seconds: int,
    retries: int,
) -> tuple[SpecialistOutcome, ...]:
    """Run all assignments with bounded concurrency and fail closed."""
    outcomes: list[SpecialistOutcome] = []
    errors: list[str] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max_parallel) as executor:
        futures = {
            executor.submit(
                run_assignment,
                assignment,
                repository_root=packet.repository_root,
                output_schema=output_schema,
                codex_bin=codex_bin,
                timeout_seconds=timeout_seconds,
                retries=retries,
            ): assignment
            for assignment in packet.assignments
        }
        for future in concurrent.futures.as_completed(futures):
            assignment = futures[future]
            try:
                outcomes.append(future.result())
            except RunnerError as error:
                errors.append(f"{assignment.assignment_id}: {error}")
    if errors:
        raise SpecialistError("specialist run failed:\n- " + "\n- ".join(errors))
    return tuple(sorted(outcomes, key=lambda outcome: outcome.assignment_id))


def parse_args() -> argparse.Namespace:
    """Parse the specialist runner command-line contract."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("packet", type=Path)
    parser.add_argument(
        "--codex-bin",
        default="codex",
        help="Codex CLI executable used for ephemeral specialist sessions",
    )
    parser.add_argument(
        "--max-parallel",
        type=int,
        default=DEFAULT_MAX_PARALLEL,
    )
    parser.add_argument(
        "--timeout-seconds",
        type=int,
        default=DEFAULT_TIMEOUT_SECONDS,
    )
    parser.add_argument("--retries", type=int, default=DEFAULT_RETRIES)
    return parser.parse_args()


def main() -> int:
    """Run the CLI workflow and emit its compact completion manifest."""
    args = parse_args()
    if not 1 <= args.max_parallel <= 32:
        raise SystemExit("--max-parallel must be between 1 and 32")
    if args.timeout_seconds < 1:
        raise SystemExit("--timeout-seconds must be positive")
    if not 0 <= args.retries <= 2:
        raise SystemExit("--retries must be between 0 and 2")
    skill_root = Path(__file__).resolve().parents[1]
    output_schema = skill_root / "references" / "schemas" / "specialist-result.json"
    try:
        if not output_schema.is_file():
            raise PacketError(f"missing specialist output schema: {output_schema}")
        packet = load_packet(args.packet.expanduser().resolve())
        outcomes = run_packet(
            packet,
            output_schema=output_schema,
            codex_bin=args.codex_bin,
            max_parallel=args.max_parallel,
            timeout_seconds=args.timeout_seconds,
            retries=args.retries,
        )
    except RunnerError as error:
        print(str(error), file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "review_id": packet.review_id,
                "model": SPECIALIST_MODEL,
                "reasoning_effort": SPECIALIST_REASONING,
                "assignments": [outcome.__dict__ for outcome in outcomes],
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
