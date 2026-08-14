#!/usr/bin/env python3
"""Validate the canonical plan and task content schemas."""

from __future__ import annotations

import argparse
import datetime as dt
import re
import sys
import tempfile
from pathlib import Path

PLAN_HEADINGS = (
    "Objective",
    "Current state and evidence",
    "Requirements",
    "Non-goals",
    "Constraints",
    "Architecture and design decisions",
    "Domain and data contracts",
    "Interfaces and function contracts",
    "Control flow and pseudocode",
    "Failure and edge-case matrix",
    "Milestones",
    "Task inventory",
    "Global acceptance criteria",
    "Verification strategy",
    "Closeout verification",
    "Risks, migration, and rollout",
    "Execution handoff",
)

TASK_HEADINGS = (
    "Objective",
    "Context",
    "Required changes",
    "Non-goals",
    "Allowed scope",
    "Prohibited changes",
    "Target paths and symbols",
    "Required types and interfaces",
    "Implementation guidance",
    "Control flow and pseudocode",
    "Failure and edge cases",
    "Acceptance criteria",
    "Required tests",
    "Required features",
    "Focused verification",
    "Commands explicitly excluded",
    "Stop and escalate if",
    "Completion evidence",
)

PLAN_METADATA = (
    "Status",
    "Repository origin",
    "Repository revision",
    "REPO_ROOT",
    "PLAN_PATH",
    "Created",
    "Last updated",
    "Plan version",
    "Evidence snapshot",
    "Review",
)

TASK_METADATA = (
    "Status",
    "Repository origin",
    "Repository revision",
    "REPO_ROOT",
    "PLAN_PATH",
    "TASK_PATH",
    "Plan",
    "Milestone",
    "Requirements",
    "Decisions",
    "Depends on",
)

PLAN_METADATA_VALUES = {
    "Status": re.compile(r"Draft|Review Required|Approved"),
    "Repository origin": re.compile(
        r"[A-Za-z0-9.-]+/[A-Za-z0-9._-]+/[A-Za-z0-9._-]+"
    ),
    "Repository revision": re.compile(r"[0-9a-f]{7,40}"),
    "REPO_ROOT": re.compile(r"\$REPO_ROOT"),
    "PLAN_PATH": re.compile(r"\$PLAN_PATH"),
    "Plan version": re.compile(r"[1-9][0-9]*"),
    "Evidence snapshot": re.compile(r"\S.+"),
    "Review": re.compile(r"not required|required|reviews/.+\.md"),
}

TASK_METADATA_VALUES = {
    "Status": re.compile(r"Planned|Ready|Complete|Blocked"),
    "Repository origin": re.compile(
        r"[A-Za-z0-9.-]+/[A-Za-z0-9._-]+/[A-Za-z0-9._-]+"
    ),
    "Repository revision": re.compile(r"[0-9a-f]{7,40}"),
    "REPO_ROOT": re.compile(r"\$REPO_ROOT"),
    "PLAN_PATH": re.compile(r"\$PLAN_PATH"),
    "TASK_PATH": re.compile(r"\$TASK_PATH"),
    "Plan": re.compile(r"\.\./plan\.md"),
    "Milestone": re.compile(r"None|M[1-9][0-9A-Za-z-]*"),
    "Requirements": re.compile(r"R[1-9][0-9]*(?:(?:,\s*|\s*[-–]\s*)R?[1-9][0-9]*)*"),
    "Decisions": re.compile(r"None|D[1-9][0-9]*(?:(?:,\s*|\s*[-–]\s*)D?[1-9][0-9]*)*"),
    "Depends on": re.compile(r"None|(?=.*\bT[1-9]).+"),
}

PLAN_METADATA_EXAMPLE = {
    "Status": "Approved",
    "Repository origin": "github.com/example/project",
    "Repository revision": "0123456789abcdef0123456789abcdef01234567",
    "REPO_ROOT": "$REPO_ROOT",
    "PLAN_PATH": "$PLAN_PATH",
    "Created": "2026-07-29",
    "Last updated": "2026-07-29",
    "Plan version": "1",
    "Evidence snapshot": "example at 0123456789ab; clean target paths",
    "Review": "not required",
}

TASK_METADATA_EXAMPLE = {
    "Status": "Ready",
    "Repository origin": "github.com/example/project",
    "Repository revision": "0123456789abcdef0123456789abcdef01234567",
    "REPO_ROOT": "$REPO_ROOT",
    "PLAN_PATH": "$PLAN_PATH",
    "TASK_PATH": "$TASK_PATH",
    "Plan": "../plan.md",
    "Milestone": "M1",
    "Requirements": "R1",
    "Decisions": "D1",
    "Depends on": "None",
}


class ValidationError(Exception):
    """Represent one or more plan-schema validation failures."""


PLAN_FILE = re.compile(r"(?:[a-z0-9][a-z0-9-]*)/(?:active|archive)/(?:[a-z0-9][a-z0-9-]*)/plan\.md$")
TASK_FILE = re.compile(r"(?:[a-z0-9][a-z0-9-]*)/(?:active|archive)/(?:[a-z0-9][a-z0-9-]*)/tasks/[0-9][0-9A-Za-z.-]*-[a-z0-9][a-z0-9-]*\.md$")
REVIEW_FILE = re.compile(r"(?:[a-z0-9][a-z0-9-]*)/(?:active|archive)/(?:[a-z0-9][a-z0-9-]*)/reviews/[a-z0-9][a-z0-9-]*\.md$")


def valid_repository_artifact(path: str) -> bool:
    """Return whether a repository-relative file has an allowed artifact path."""
    return any(pattern.fullmatch(path) for pattern in (PLAN_FILE, TASK_FILE, REVIEW_FILE))


def validate_repository(root: Path) -> list[str]:
    """Validate exact artifact placement and every immediate plan directory."""
    errors: list[str] = []
    owners = sorted(
        path
        for path in root.iterdir()
        if path.is_dir() and ((path / "active").is_dir() or (path / "archive").is_dir())
    )
    for owner in owners:
        for path in sorted(owner.rglob("*")):
            if path.is_file() and not valid_repository_artifact(
                path.relative_to(root).as_posix()
            ):
                errors.append(f"{path}: invalid shared-plan artifact path")
        for state in ("active", "archive"):
            state_root = owner / state
            if state_root.is_dir():
                for plan in sorted(path for path in state_root.iterdir() if path.is_dir()):
                    errors.extend(validate_plan_directory(plan))
    return errors


def _metadata(text: str, first_heading_offset: int) -> dict[str, str]:
    """Extract colon-delimited metadata before the first level-two heading."""

    values: dict[str, str] = {}
    for line in text[:first_heading_offset].splitlines():
        if ":" not in line or line.startswith("#"):
            continue
        key, value = line.split(":", 1)
        values[key.strip()] = value.strip()
    return values


def _sections(text: str) -> tuple[list[str], dict[str, str], int]:
    """Return ordered level-two headings, their bodies, and first offset."""

    matches = list(re.finditer(r"(?m)^## ([^\n]+)\s*$", text))
    if not matches:
        raise ValidationError("contains no level-two sections")

    headings = [match.group(1).strip() for match in matches]
    bodies: dict[str, str] = {}
    for index, match in enumerate(matches):
        end = matches[index + 1].start() if index + 1 < len(matches) else len(text)
        bodies[headings[index]] = text[match.end() : end].strip()
    return headings, bodies, matches[0].start()


def _validate_document(
    path: Path,
    expected_headings: tuple[str, ...],
    expected_metadata: tuple[str, ...],
    metadata_values: dict[str, re.Pattern[str]],
) -> list[str]:
    """Validate one plan or task document and return human-readable errors."""

    errors: list[str] = []
    text = path.read_text(encoding="utf-8")

    try:
        headings, bodies, first_offset = _sections(text)
    except ValidationError as error:
        return [f"{path}: {error}"]

    normalized_headings = tuple(headings)
    if expected_headings == TASK_HEADINGS:
        while (
            len(normalized_headings) > len(expected_headings)
            and normalized_headings[-1] == "Completion evidence"
        ):
            normalized_headings = normalized_headings[:-1]
    if normalized_headings != expected_headings:
        errors.append(
            f"{path}: level-two headings must be exactly, in order: "
            + " | ".join(expected_headings)
        )

    metadata = _metadata(text, first_offset)
    for key in expected_metadata:
        if not metadata.get(key):
            errors.append(f"{path}: missing non-empty metadata field `{key}:`")

    for key, pattern in metadata_values.items():
        value = metadata.get(key)
        if value and pattern.fullmatch(value) is None:
            errors.append(f"{path}: invalid `{key}: {value}`")

    for key in ("Created", "Last updated"):
        value = metadata.get(key)
        if not value:
            continue
        try:
            dt.date.fromisoformat(value)
        except ValueError:
            errors.append(f"{path}: invalid `{key}: {value}`; expected YYYY-MM-DD")

    for heading in expected_headings:
        if heading not in bodies:
            continue
        body = bodies[heading]
        if not body:
            errors.append(
                f"{path}: section `## {heading}` is empty; use `Not applicable` "
                "with a reason when needed"
            )
        else:
            first_line = body.splitlines()[0].strip()
            if (
                first_line.startswith("Not applicable")
                and re.fullmatch(
                    r"Not applicable: \S.+",
                    first_line,
                )
                is None
            ):
                errors.append(
                    f"{path}: section `## {heading}` must use "
                    "`Not applicable: <reason>`"
                )

    return errors


def validate_plan_directory(plan_dir: Path) -> list[str]:
    """Validate one canonical plan directory and all of its task packets."""

    errors: list[str] = []
    plan_path = plan_dir / "plan.md"
    task_dir = plan_dir / "tasks"

    if not plan_path.is_file():
        errors.append(f"{plan_dir}: missing `plan.md`")
        return errors

    errors.extend(
        _validate_document(
            plan_path,
            PLAN_HEADINGS,
            PLAN_METADATA,
            PLAN_METADATA_VALUES,
        )
    )

    task_paths = sorted(task_dir.glob("[0-9][0-9]*-*.md")) if task_dir.is_dir() else []
    if not task_paths:
        errors.append(f"{plan_dir}: expected at least one `tasks/<id>-<slug>.md`")
        return errors

    plan_text = plan_path.read_text(encoding="utf-8")
    plan_metadata = _metadata(plan_text, _sections(plan_text)[2])
    plan_status = plan_metadata.get("Status")

    for task_path in task_paths:
        errors.extend(
            _validate_document(
                task_path,
                TASK_HEADINGS,
                TASK_METADATA,
                TASK_METADATA_VALUES,
            )
        )
        relative = task_path.relative_to(plan_dir).as_posix()
        if relative not in plan_text:
            errors.append(
                f"{plan_path}: task inventory does not reference `{relative}`"
            )

        task_text = task_path.read_text(encoding="utf-8")
        task_metadata = _metadata(
            task_text,
            _sections(task_text)[2],
        )
        task_status = task_metadata.get("Status")
        for key in ("Repository origin", "Repository revision"):
            if task_metadata.get(key) != plan_metadata.get(key):
                errors.append(
                    f"{task_path}: `{key}` must match {plan_path}"
                )
        if task_status == "Ready" and plan_status != "Approved":
            errors.append(
                f"{task_path}: task cannot be `Ready` while plan status is "
                f"`{plan_status or 'missing'}`"
            )

    return errors


def _valid_plan_text() -> str:
    """Build a minimal valid plan for the self-test."""

    metadata = "\n".join(
        f"{key}: {PLAN_METADATA_EXAMPLE[key]}" for key in PLAN_METADATA
    )
    bodies = {heading: "Content." for heading in PLAN_HEADINGS}
    bodies["Requirements"] = "- R1. Observable result."
    bodies[
        "Architecture and design decisions"
    ] = "### D1: Use the existing owner\n\nKeep ownership unchanged."
    bodies["Task inventory"] = "| Task | Packet |\n|---|---|\n| T1 | `tasks/01-example.md` |"
    bodies[
        "Risks, migration, and rollout"
    ] = "Not applicable: the example has no durable or deployment change."
    sections = "\n\n".join(
        f"## {heading}\n\n{bodies[heading]}" for heading in PLAN_HEADINGS
    )
    return f"# Example\n\n{metadata}\n\n{sections}\n\ntasks/01-example.md\n"


def _valid_task_text() -> str:
    """Build a minimal valid task for the self-test."""

    metadata = "\n".join(
        f"{key}: {TASK_METADATA_EXAMPLE[key]}" for key in TASK_METADATA
    )
    bodies = {heading: "Content." for heading in TASK_HEADINGS}
    bodies["Acceptance criteria"] = "- AC1. Observable result."
    sections = "\n\n".join(
        f"## {heading}\n\n{bodies[heading]}" for heading in TASK_HEADINGS
    )
    return f"# T1: Example\n\n{metadata}\n\n{sections}\n"


def _replace_metadata(text: str, key: str, current: str, replacement: str) -> str:
    """Replace one exact metadata line in a self-test document."""

    return text.replace(
        f"{key}: {current}",
        f"{key}: {replacement}",
        1,
    )


def _published_example(reference: Path, section: str) -> str:
    """Extract the first Markdown artifact example after a named section."""

    text = reference.read_text(encoding="utf-8")
    _, section_text = text.split(f"## {section}", 1)
    match = re.search(
        r"(?P<fence>`{3,})markdown\n(?P<body>.*?)\n(?P=fence)",
        section_text,
        re.DOTALL,
    )
    if match is None:
        raise ValidationError(f"{reference}: missing Markdown example in `{section}`")
    return match.group("body") + "\n"


def self_test() -> int:
    """Exercise valid and invalid schema paths without persistent fixtures."""

    with tempfile.TemporaryDirectory() as temporary:
        plan_dir = Path(temporary)
        task_dir = plan_dir / "tasks"
        task_dir.mkdir()
        (plan_dir / "plan.md").write_text(
            _valid_plan_text(),
            encoding="utf-8",
        )
        (task_dir / "01-example.md").write_text(_valid_task_text(), encoding="utf-8")

        if validate_plan_directory(plan_dir):
            print("self-test failed: valid fixture was rejected", file=sys.stderr)
            return 1

        task_path = task_dir / "01-example.md"
        ready_task = task_path.read_text(encoding="utf-8")
        for status in ("Planned", "Ready", "Complete", "Blocked"):
            task_path.write_text(
                _replace_metadata(ready_task, "Status", "Ready", status),
                encoding="utf-8",
            )
            if validate_plan_directory(plan_dir):
                print(
                    f"self-test failed: `{status}` task fixture was rejected",
                    file=sys.stderr,
                )
                return 1
        task_path.write_text(ready_task, encoding="utf-8")

        plan_path = plan_dir / "plan.md"

        references = Path(__file__).resolve().parent.parent / "references"
        plan_path.write_text(
            _published_example(references / "plan-format.md", "Compact example"),
            encoding="utf-8",
        )
        task_path.unlink()
        published_task_path = task_dir / "01-example.md"
        published_task_path.write_text(
            _published_example(
                references / "task-packet-format.md",
                "Complete example",
            ),
            encoding="utf-8",
        )
        published_errors = validate_plan_directory(plan_dir)
        if published_errors:
            for error in published_errors:
                print(error, file=sys.stderr)
            print("self-test failed: published examples were rejected", file=sys.stderr)
            return 1

        published_task_path.unlink()
        plan_path.write_text(_valid_plan_text(), encoding="utf-8")
        task_path.write_text(_valid_task_text(), encoding="utf-8")

        for key, current in PLAN_METADATA_EXAMPLE.items():
            if key == "Evidence snapshot":
                continue
            original = plan_path.read_text(encoding="utf-8")
            plan_path.write_text(
                _replace_metadata(original, key, current, "invalid"),
                encoding="utf-8",
            )
            if not validate_plan_directory(plan_dir):
                print(
                    f"self-test failed: invalid plan metadata `{key}` was accepted",
                    file=sys.stderr,
                )
                return 1
            plan_path.write_text(original, encoding="utf-8")

        for key, current in TASK_METADATA_EXAMPLE.items():
            original = task_path.read_text(encoding="utf-8")
            task_path.write_text(
                _replace_metadata(original, key, current, "invalid"),
                encoding="utf-8",
            )
            if not validate_plan_directory(plan_dir):
                print(
                    f"self-test failed: invalid task metadata `{key}` was accepted",
                    file=sys.stderr,
                )
                return 1
            task_path.write_text(original, encoding="utf-8")

        original = task_path.read_text(encoding="utf-8")
        mismatched_revision = _replace_metadata(
            original,
            "Repository revision",
            TASK_METADATA_EXAMPLE["Repository revision"],
            "abcdef0",
        )
        task_path.write_text(mismatched_revision, encoding="utf-8")
        if not validate_plan_directory(plan_dir):
            print(
                "self-test failed: task repository revision mismatch was accepted",
                file=sys.stderr,
            )
            return 1
        task_path.write_text(original, encoding="utf-8")

        invalid_heading = original.replace(
            "## Required tests\n\nContent.\n\n",
            "",
        )
        task_path.write_text(invalid_heading, encoding="utf-8")
        if not validate_plan_directory(plan_dir):
            print(
                "self-test failed: missing task heading was accepted", file=sys.stderr
            )
            return 1
        task_path.write_text(original, encoding="utf-8")

        invalid_not_applicable = original.replace(
            "## Failure and edge cases\n\nContent.",
            "## Failure and edge cases\n\nNot applicable",
        )
        task_path.write_text(invalid_not_applicable, encoding="utf-8")
        if not validate_plan_directory(plan_dir):
            print(
                "self-test failed: unexplained `Not applicable` was accepted",
                file=sys.stderr,
            )
            return 1

    if valid_repository_artifact("wyrd/active/example/tasks/HANDOFF.md"):
        print("self-test failed: non-numbered task artifact was accepted", file=sys.stderr)
        return 1
    if valid_repository_artifact("wyrd/misc/example/plan.md"):
        print("self-test failed: plan outside active/archive was accepted", file=sys.stderr)
        return 1

    print("self-test passed")
    return 0


def main() -> int:
    """Run schema validation or the built-in self-test."""

    parser = argparse.ArgumentParser()
    parser.add_argument("plan_dir", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--repository-root", type=Path)
    arguments = parser.parse_args()

    if arguments.self_test:
        return self_test()
    if arguments.repository_root is not None:
        errors = validate_repository(arguments.repository_root)
        if errors:
            for error in errors:
                print(error, file=sys.stderr)
            return 1
        print(f"valid shared plan repository: {arguments.repository_root}")
        return 0
    if arguments.plan_dir is None:
        parser.error("plan_dir is required unless --self-test is used")

    errors = validate_plan_directory(arguments.plan_dir)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1

    print(f"valid plan artifacts: {arguments.plan_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
