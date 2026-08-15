#!/usr/bin/env python3
"""Validate the canonical plan and task content schemas."""

from __future__ import annotations

import argparse
import datetime as dt
import re
import shlex
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


BROAD_MISE = re.compile(r"^mise run (?:lints|check|pre-pr)(?:\s|$)")
BROAD_TEST_LANE = re.compile(
    r"^mise run (?:test:(?:unit|shared|wyrd|skald|vala|sql|storage(?::matrix)?)|"
    r"(?:py|python|ts|typescript):test:(?:unit|integration)|"
    r"test:(?:journey|journeys|cluster|fuzz)(?::(?:matrix|all))?)\b"
)
CANONICAL_MATRIX = re.compile(
    r"(?:^|[:_-])(?:journey|journeys|cluster|fuzz)(?::|-|_)(?:matrix|all)(?:$|[:_-])",
    re.IGNORECASE,
)
ESCAPE = re.compile(
    r"(?m)^Exact-lane exception:\s*\n"
    r"- requirement: (?P<requirement>(?:R|AC)[1-9][0-9]*)\s*\n"
    r"- lane_owner: `(?P<owner>[^`]+)`\s*\n"
    r"- narrowing_loss: (?P<loss>\S.+)\s*\n"
    r"(?P<command_line>`(?P<command>[^`\n]+)`)\s*$"
)
KNOWN_COMMANDS = {"cargo", "cd", "git", "mise", "npm", "npx", "pnpm", "pytest", "uv"}
SHELLS = {"bash", "sh", "zsh"}
WRAPPER_PATH = re.compile(r"^(?:\./|\.agents/|scripts/)[A-Za-z0-9_./-]+$")
POSTGRES_WRAPPER = "scripts/postgres/with-test-postgres.sh"
SUBSTITUTION = re.compile(r"\$\(|`|(?<!\w)[<>]\(")
GENERIC_LOSS = re.compile(
    r"^(?:named tests omit acceptance proof|this exact lane is the acceptance contract)\.?$",
    re.IGNORECASE,
)


def _logical_lines(text: str) -> list[str]:
    """Normalize prompts and continuations into deterministic shell recipes."""

    text = re.sub(r"\\\s*\n\s*", " ", text)
    lines: list[str] = []
    for line in text.splitlines():
        candidate = line.strip()
        if not candidate or candidate.startswith("#"):
            continue
        lines.append(re.sub(r"^\$\s+", "", candidate))
    return lines


def _recipes(body: str) -> list[str]:
    """Extract every shell recipe from fences, code spans, and command lists."""

    recipes: list[str] = []
    fenced_ranges: list[tuple[int, int]] = []
    for match in re.finditer(
        r"(?ms)^\s*```(?:bash|sh|shell|zsh)?\s*\n(.*?)^\s*```\s*$",
        body,
    ):
        fenced_ranges.append(match.span())
        recipes.extend(_logical_lines(match.group(1)))
    remainder = body
    for start, end in reversed(fenced_ranges):
        remainder = remainder[:start] + remainder[end:]
    exception_spans = [match.span("command_line") for match in ESCAPE.finditer(remainder)]
    for match in re.finditer(r"`([^`\n]+)`", remainder):
        if any(start <= match.start() and match.end() <= end for start, end in exception_spans):
            continue
        candidate = match.group(1).strip()
        line_start = remainder.rfind("\n", 0, match.start()) + 1
        line_end = remainder.find("\n", match.end())
        if line_end == -1:
            line_end = len(remainder)
        containing_line = remainder[line_start:line_end]
        is_standalone_code = re.fullmatch(
            r"\s*(?:[-*+]\s+|\d+[.)]\s+)?`[^`]+`[.]?\s*",
            containing_line,
        ) is not None
        if _looks_executable(candidate, allow_bare=is_standalone_code):
            recipes.append(candidate)
    for line in remainder.splitlines():
        candidate = re.sub(r"^\s*(?:[-*+]\s+|\d+[.)]\s+)", "", line).strip()
        candidate = re.sub(r"^\$\s+", "", candidate)
        if re.fullmatch(r"`[^`]+`[.]?", candidate):
            continue
        if _looks_executable(candidate) and candidate not in recipes:
            recipes.append(candidate)
    return recipes


def _looks_executable(candidate: str, *, allow_bare: bool = True) -> bool:
    """Return whether Markdown content claims to be an executable recipe."""

    if not candidate or candidate.endswith(('.', ':')):
        return False
    first = candidate.split(maxsplit=1)[0]
    return (
        first in KNOWN_COMMANDS | SHELLS | {"env", "command"}
        or WRAPPER_PATH.fullmatch(first) is not None
        or any(operator in candidate for operator in ("&&", "||", ";", "|"))
        or (
            allow_bare
            and re.fullmatch(r"[A-Za-z0-9_.-]+", candidate) is not None
        )
    )


def _shell_segments(recipe: str) -> tuple[list[str], str | None]:
    """Split one shell recipe at control and pipe operators with shell quoting."""

    try:
        lexer = shlex.shlex(recipe, posix=True, punctuation_chars=";&|")
        lexer.whitespace_split = True
        lexer.commenters = ""
        tokens = list(lexer)
    except ValueError as error:
        return [], str(error)
    segments: list[str] = []
    current: list[str] = []
    for token in tokens:
        if token and set(token) <= {";", "&", "|"}:
            if current:
                segments.append(shlex.join(current))
                current = []
            continue
        current.append(token)
    if current:
        segments.append(shlex.join(current))
    return segments, None


def _unwrap_segment(segment: str) -> tuple[list[str], str | None]:
    """Return recursively inspectable commands hidden behind shell wrappers."""

    try:
        tokens = shlex.split(segment)
    except ValueError as error:
        return [], str(error)
    while tokens and tokens[0] in {"env", "command"}:
        tokens.pop(0)
        while tokens and ("=" in tokens[0] or tokens[0].startswith("-")):
            tokens.pop(0)
    if not tokens:
        return [], "empty wrapper payload"
    if tokens[0] in SHELLS:
        for index, token in enumerate(tokens[1:], 1):
            if token in {"-c", "-lc"} and index + 1 < len(tokens):
                return [tokens[index + 1]], None
        return [], "shell wrapper must use `-c` or `-lc` with a payload"
    if tokens[0] == "mise" and tokens[1:3] == ["exec", "--"]:
        return [shlex.join(tokens[3:])], None
    if tokens[0] == POSTGRES_WRAPPER and "--" in tokens:
        index = tokens.index("--")
        return [shlex.join(tokens[index + 1 :])], None
    return [shlex.join(tokens)], None


def _cargo_has_package(command: str) -> bool:
    """Return whether a Cargo command selects an explicit affected package."""

    return re.search(r"(?:^|\s)(?:-p|--package)(?:\s+|=)\S+", command) is not None


def _cargo_subcommand(tokens: list[str]) -> tuple[str | None, int, str | None]:
    """Resolve a Cargo subcommand after supported toolchain and global options."""

    boolean_flags = {
        "-V", "--version", "--list", "-v", "-vv", "-vvv", "-q", "--quiet",
        "--locked", "--offline", "--frozen", "-h", "--help",
    }
    value_flags = {"--explain", "--color", "-C", "--config", "-Z"}
    index = 1
    if index < len(tokens) and tokens[index].startswith("+"):
        index += 1
    while index < len(tokens):
        token = tokens[index]
        aliases = {"t": "test", "c": "check", "b": "build"}
        if token in {"test", "clippy", "check", "build"} | aliases.keys():
            return aliases.get(token, token), index, None
        if token in boolean_flags or re.fullmatch(r"-v+", token):
            index += 1
        elif token in value_flags:
            if index + 1 >= len(tokens):
                return None, index, f"unsupported Cargo global option layout `{token}`"
            index += 2
        elif any(token.startswith(f"{flag}=") for flag in {"--explain", "--color", "--config"}):
            index += 1
        elif (
            token.startswith("-C") and len(token) > 2
        ) or (token.startswith("-Z") and len(token) > 2):
            index += 1
        elif token.startswith("-"):
            return None, index, f"unsupported Cargo global option layout `{token}`"
        else:
            return token, index, None
    return None, index, None


def _canonical_mise(tokens: list[str]) -> tuple[str, str | None]:
    """Normalize supported mise global flags and task invocation aliases."""

    boolean_flags = {
        "-q",
        "--quiet",
        "--silent",
        "--no-config",
        "--no-env",
        "--no-hooks",
        "--yes",
        "-y",
        "--raw",
        "--locked",
    }
    value_flags = {"-C", "--cd", "-E", "--env", "-j", "--jobs", "--output"}
    index = 1
    while index < len(tokens) and tokens[index].startswith("-"):
        option = tokens[index]
        if option in boolean_flags or re.fullmatch(r"-v+", option):
            index += 1
        elif option in value_flags and index + 1 < len(tokens):
            index += 2
        elif any(
            option.startswith(f"{flag}=")
            for flag in {"--cd", "--env", "--jobs", "--output"}
        ):
            index += 1
        elif option.startswith(("-C", "-E", "-j")) and len(option) > 2:
            index += 1
        else:
            return shlex.join(tokens), f"unsupported mise global option layout `{option}`"
    remaining = tokens[index:]
    if not remaining or remaining[0] == "exec":
        return shlex.join(tokens), None
    if remaining[0] == "tasks":
        if len(remaining) < 2 or remaining[1] not in {"run", "r"}:
            return shlex.join(tokens), "unsupported `mise tasks` invocation layout"
        remaining = remaining[1:]
    if remaining[0] in {"run", "r"}:
        remaining = remaining[1:]
        task_boolean_flags = {
            "-c", "--continue-on-error", "-f", "--force", "-n", "--dry-run",
            "-q", "--quiet", "-r", "--raw", "-S", "--silent",
        }
        task_value_flags = {
            "-C", "--cd", "-j", "--jobs", "-o", "--output", "-s", "--shell",
            "-t", "--tool", "--affected-base", "--affected-head", "--allow-env",
            "--allow-net",
        }
        while remaining and remaining[0].startswith("-"):
            option = remaining[0]
            if option in task_boolean_flags:
                remaining = remaining[1:]
            elif option in task_value_flags and len(remaining) > 1:
                remaining = remaining[2:]
            elif any(
                option.startswith(f"{flag}=")
                for flag in {
                    "--cd", "--jobs", "--output", "--shell", "--tool",
                    "--affected-base", "--affected-head", "--allow-env", "--allow-net",
                }
            ):
                remaining = remaining[1:]
            else:
                return shlex.join(tokens), f"unsupported mise task option layout `{option}`"
    if not remaining:
        return shlex.join(tokens), "mise task invocation is missing a task name"
    return shlex.join(["mise", "run", *remaining]), None


def _canonical_npx(tokens: list[str]) -> tuple[list[str], str | None]:
    """Normalize supported NPX launcher flags before the package command."""

    boolean_flags = {"--yes", "-y", "--workspaces", "--include-workspace-root"}
    value_flags = {"--package", "-p", "--workspace", "-w"}
    index = 1
    while index < len(tokens) and tokens[index].startswith("-"):
        option = tokens[index]
        if option in boolean_flags:
            index += 1
        elif option in value_flags and index + 1 < len(tokens):
            index += 2
        elif option in {"--call", "-c"} and index + 1 < len(tokens):
            try:
                return ["npx", *shlex.split(tokens[index + 1])], None
            except ValueError as error:
                return tokens, f"invalid npx `--call` payload: {error}"
        elif any(option.startswith(f"{flag}=") for flag in {"--package", "--workspace"}):
            index += 1
        else:
            return tokens, f"unsupported npx launcher option layout `{option}`"
    return ["npx", *tokens[index:]], None


def _cargo_test_has_filter(command: str) -> bool:
    """Return whether Cargo test names an exact positional test-name filter."""

    cargo_part, separator, harness_part = command.partition(" -- ")
    tokens = shlex.split(cargo_part)
    subcommand, index, error = _cargo_subcommand(tokens)
    if error is not None:
        return False
    if subcommand != "test":
        return True
    index += 1
    options_with_values = {
        "-p",
        "--package",
        "--features",
        "--test",
        "--bin",
        "--example",
        "--manifest-path",
        "-j",
        "--jobs",
    }
    while index < len(tokens):
        token = tokens[index]
        if token in options_with_values:
            index += 2
            continue
        if token.startswith("-"):
            index += 1
            continue
        return True
    if separator:
        harness_tokens = shlex.split(harness_part)
        harness_options_with_values = {
            "--color",
            "--format",
            "--logfile",
            "--test-threads",
        }
        index = 0
        while index < len(harness_tokens):
            token = harness_tokens[index]
            if token in harness_options_with_values:
                index += 2
                continue
            if token.startswith("-"):
                index += 1
                continue
            return True
    return False


def _segment_defect(command: str) -> str | None:
    """Return the focus-policy defect for one fully unwrapped command segment."""

    try:
        tokens = shlex.split(command)
    except ValueError as error:
        return f"invalid shell quoting: {error}"
    if not tokens:
        return "empty command segment"
    executable = tokens[0]
    if executable not in KNOWN_COMMANDS and WRAPPER_PATH.fullmatch(executable) is None:
        return "unknown alias or function; use an explicit repository path or supported command"
    if executable == "cd":
        if len(tokens) == 2 and re.fullmatch(r"[A-Za-z0-9_./-]+", tokens[1]):
            return None
        return "focused verification `cd` must name one explicit repository-relative path"
    if WRAPPER_PATH.fullmatch(executable) and executable != POSTGRES_WRAPPER:
        return "opaque repository script requires a structured exact-lane exception"
    if executable == POSTGRES_WRAPPER:
        return "Postgres wrapper must include `--` and a focused payload"
    if executable == "mise":
        canonical_command, mise_error = _canonical_mise(tokens)
        if mise_error is not None:
            return mise_error
    else:
        canonical_command = command
    if BROAD_MISE.search(canonical_command):
        return "aggregate `mise run lints|check|pre-pr` belongs to parent closeout"
    if BROAD_TEST_LANE.search(canonical_command):
        return (
            "workspace, crate-family, or unfiltered language test lane belongs "
            "to parent closeout"
        )
    canonical_tokens = shlex.split(canonical_command)
    if executable == "mise" and len(canonical_tokens) >= 3 and CANONICAL_MATRIX.search(canonical_tokens[2]):
        return "canonical journey/cluster/fuzz matrix belongs to parent closeout"
    if executable == "cargo":
        if re.search(r"(?:^|\s)--workspace(?:\s|$)", command):
            return "task-level Cargo must not select the workspace"
        if re.search(r"(?:^|\s)--all-features(?:\s|$)", command):
            return "task-level `--all-features` is not earned feature selection"
        subcommand, _, cargo_error = _cargo_subcommand(tokens)
        if cargo_error is not None:
            return cargo_error
        if subcommand == "clippy" and not _cargo_has_package(command):
            return "task-level Clippy must select an affected package with `-p`"
        if subcommand == "test":
            if not _cargo_has_package(command):
                return "task-level Cargo test must select an affected package with `-p`"
            if not _cargo_test_has_filter(command):
                return "task-level Cargo test must name an exact positional test filter"
    if executable == "npx":
        normalized_npx, npx_error = _canonical_npx(tokens)
        if npx_error is not None:
            return npx_error
        command = shlex.join(normalized_npx)
    if re.search(r"(?:pytest|py:test:integration)", command) and not re.search(
        r"(?:\.py::\S+|\s-k\s+\S+)", command
    ):
        return "Python verification must name an exact test node or `-k` filter"
    if re.search(
        r"(?:typescript.*(?:test|integration)|\bts:(?:test|integration)|"
        r"pnpm.*(?:test|integration)|npm.*(?:test|integration)|"
        r"(?:pnpm|npx)\s+vitest)",
        command,
    ) and not re.search(
        r"(?:\.test\.[cm]?[jt]s|\.spec\.[cm]?[jt]s|"
        r"--testNamePattern|\s-t\s+\S+)",
        command,
    ):
        return "TypeScript verification must name an affected test file or test-name filter"
    return None


def _recipe_defects(recipe: str) -> list[tuple[str, str]]:
    """Recursively inspect every executable segment within one shell recipe."""

    pending = [recipe]
    defects: list[tuple[str, str]] = []
    while pending:
        current = pending.pop(0)
        substitution = SUBSTITUTION.search(current)
        if substitution is not None:
            defects.append(
                (
                    substitution.group(0),
                    "executable substitution is unauditable in focused verification",
                )
            )
            continue
        segments, split_error = _shell_segments(current)
        if split_error is not None:
            defects.append((current, f"invalid shell recipe: {split_error}"))
            continue
        for segment in segments:
            unwrapped, unwrap_error = _unwrap_segment(segment)
            if unwrap_error is not None:
                defects.append((segment, unwrap_error))
                continue
            if len(unwrapped) != 1 or unwrapped[0] != segment:
                pending.extend(unwrapped)
                continue
            defect = _segment_defect(segment)
            if defect is not None:
                defects.append((segment, defect))
    return defects


def _focused_verification_errors(
    path: Path,
    body: str,
    requirement_ids: set[str],
) -> list[str]:
    """Reject task-level commands that prove substantially unrelated surfaces."""

    errors: list[str] = []
    exception_matches = list(ESCAPE.finditer(body))
    if body.count("Exact-lane exception:") != len(exception_matches):
        errors.append(
            f"{path}: every exact-lane exception must use the complete structured form"
        )
    exception_commands: dict[str, re.Match[str]] = {}
    seen_losses: set[str] = set()
    for match in exception_matches:
        command = match.group("command")
        requirement = match.group("requirement")
        owner = match.group("owner")
        loss = match.group("loss").strip()
        if command in exception_commands:
            errors.append(f"{path}: exact-lane exception command is reused: `{command}`")
        exception_commands[command] = match
        if requirement not in requirement_ids:
            errors.append(
                f"{path}: exact-lane exception references unknown `{requirement}`"
            )
        if not (owner.startswith("mise.toml:") or WRAPPER_PATH.fullmatch(owner)):
            errors.append(
                f"{path}: exact-lane exception lane_owner must name `mise.toml:<task>` "
                "or an explicit repository path"
            )
        elif owner.startswith("mise.toml:"):
            lane = owner.split(":", 1)[1]
            try:
                normalized_exception_command, _ = _canonical_mise(
                    shlex.split(command)
                )
            except ValueError:
                normalized_exception_command = command
            if re.match(
                rf"^mise run {re.escape(lane)}(?:\s|$)",
                normalized_exception_command,
            ) is None:
                errors.append(
                    f"{path}: exact-lane exception command does not match "
                    f"lane_owner `{owner}`"
                )
        elif owner not in command:
            errors.append(
                f"{path}: exact-lane exception command does not invoke "
                f"lane_owner `{owner}`"
            )
        normalized_loss = re.sub(r"\s+", " ", loss.lower().rstrip("."))
        if len(loss.split()) < 8 or GENERIC_LOSS.fullmatch(loss):
            errors.append(
                f"{path}: exact-lane exception needs a concrete `narrowing_loss`"
            )
        if normalized_loss in seen_losses:
            errors.append(
                f"{path}: duplicate exact-lane exception `narrowing_loss`: {loss}"
            )
        seen_losses.add(normalized_loss)

    recipes = _recipes(body)
    if re.search(
        r"(?mi)^Affected (?:crate|crates|package|packages|surface|surfaces):\s*\S+",
        body,
    ) is None:
        errors.append(
            f"{path}: `## Focused verification` must declare explicit affected "
            "packages or surfaces"
        )
    recipes.extend(exception_commands)
    for recipe in recipes:
        defects = _recipe_defects(recipe)
        if not defects:
            if recipe in exception_commands:
                errors.append(
                    f"{path}: exact-lane exception is unnecessary for focused command "
                    f"`{recipe}`"
                )
            continue
        if recipe in exception_commands:
            segments, split_error = _shell_segments(recipe)
            if split_error is not None or len(segments) != 1:
                errors.append(
                    f"{path}: exact-lane exception must bind one command segment"
                )
                continue
            if any(
                "unknown alias or function" in defect
                or "invalid shell" in defect
                or "empty" in defect
                or "substitution" in defect
                for _, defect in defects
            ):
                errors.append(
                    f"{path}: exact-lane exception cannot authorize an unauditable "
                    "or malformed command"
                )
            continue
        for segment, defect in defects:
            errors.append(
                f"{path}: `## Focused verification` rejects segment `{segment}`: "
                f"{defect}"
            )
    return errors


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

    if (
        expected_headings == TASK_HEADINGS
        and metadata.get("Status") == "Ready"
        and "Focused verification" in bodies
    ):
        requirement_ids = set(
            re.findall(r"\bR[1-9][0-9]*\b", metadata.get("Requirements", ""))
        )
        requirement_ids.update(
            re.findall(
                r"\bAC[1-9][0-9]*\b",
                bodies.get("Acceptance criteria", ""),
            )
        )
        errors.extend(
            _focused_verification_errors(
                path,
                bodies["Focused verification"],
                requirement_ids,
            )
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
    bodies["Focused verification"] = (
        "Affected package: `example`.\n\n"
        "- `mise exec -- cargo test --locked -p example exact_behavior -- --nocapture`"
    )
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

        task_path.write_text(original, encoding="utf-8")
        invalid_commands = (
            "mise run lints",
            "mise lints",
            "mise r lints",
            "mise -q run lints",
            "mise --quiet run lints",
            "mise run --quiet lints",
            "mise tasks run lints",
            "mise tasks r lints",
            "mise run check",
            "mise run pre-pr",
            "mise run test:shared",
            "mise test:vala:integration",
            "mise --quiet run test:vala:integration",
            "mise run test:journey:matrix",
            "mise run test:cluster:all",
            "mise run test:fuzz:matrix",
            "mise run py:test:integration",
            "pnpm test:integration",
            "npx vitest run",
            "npx --yes vitest run",
            "npx --package vitest vitest run",
            "npx -c 'vitest run'",
            "mise exec -- cargo test --workspace exact_behavior",
            "cargo +stable test -p example",
            "cargo t -p example",
            "cargo +stable t -p example",
            "cargo --quiet t -p example",
            "mise exec -- cargo test -p example --all-features exact_behavior",
            "mise exec -- cargo test -p example",
            "mise exec -- cargo test -p example --test api",
            "mise exec -- cargo clippy --locked",
            "cargo test -p example exact_behavior && mise run lints",
            "cargo test -p example exact_behavior; mise run pre-pr",
            "cargo test -p example exact_behavior || mise run check",
            "cargo test -p example exact_behavior | mise run test:shared",
            "env RUST_LOG=debug command cargo test -p example exact_behavior && lint-all",
            "bash -lc 'cargo test -p example exact_behavior && mise run lints'",
            "lint-all",
            "scripts/checks/run-all.sh -- cargo test -p example exact_behavior",
            "cargo test -p example $(mise run pre-pr)",
            "cargo test -p example $(printf '%s' $(mise run pre-pr))",
            "cargo test -p example exact_behavior <(mise run pre-pr)",
            "cargo test -p example exact_behavior >(mise run pre-pr)",
            "bash -lc 'cargo test -p example $(mise run pre-pr)'",
        )
        valid_command = "mise exec -- cargo test --locked -p example exact_behavior -- --nocapture"
        for command in invalid_commands:
            candidate = original.replace(valid_command, command)
            task_path.write_text(candidate, encoding="utf-8")
            if not validate_plan_directory(plan_dir):
                print(
                    f"self-test failed: broad command `{command}` was accepted",
                    file=sys.stderr,
                )
                return 1

        prompted_continuation = original.replace(
            f"- `{valid_command}`",
            "```bash\n"
            "$ cargo test -p example exact_behavior \\\n"
            "  && mise run \\\n"
            "  pre-pr\n"
            "```",
        )
        task_path.write_text(prompted_continuation, encoding="utf-8")
        prompted_errors = validate_plan_directory(plan_dir)
        if not any("mise run pre-pr" in error for error in prompted_errors):
            print(
                "self-test failed: prompted continuation bypass was accepted",
                file=sys.stderr,
            )
            return 1

        backtick_substitution = original.replace(
            f"- `{valid_command}`",
            "```bash\n"
            "$ cargo test -p example exact_behavior \"`mise run pre-pr`\"\n"
            "```",
        )
        task_path.write_text(backtick_substitution, encoding="utf-8")
        substitution_errors = validate_plan_directory(plan_dir)
        if not any("substitution" in error for error in substitution_errors):
            print(
                "self-test failed: quoted backtick substitution was accepted",
                file=sys.stderr,
            )
            return 1

        valid_commands = (
            "mise exec -- cargo test --locked -p example exact_behavior -- --nocapture",
            "mise exec -- cargo test --locked -p example --test api -- exact_behavior --test-threads=1",
            "cargo +stable --locked test -p example exact_behavior",
            "cargo t -p example exact_behavior",
            "cargo +stable --quiet t -p example exact_behavior",
            "mise exec -- cargo test --locked -p example --test api exact_behavior",
            "uv run pytest tests/test_api_journey.py::test_exact_behavior",
            "pnpm vitest run tests/api.test.ts",
            "npx vitest run tests/api.test.ts",
            "npx --yes vitest run tests/api.test.ts",
            "npx --package vitest vitest run tests/api.test.ts",
            "npx -c 'vitest run tests/api.test.ts'",
            "mise exec -- cargo test --locked -p wyrd-mcp exact_tool_journey",
            "scripts/postgres/with-test-postgres.sh -- bash -lc "
            "'cargo test -p wyrd-sql --test postgres exact_round_trip'",
            "env RUST_LOG=debug command cargo test -p example exact_behavior",
            "mise run codegen:check",
            "mise --quiet run codegen:check",
            "mise run --quiet codegen:check",
            "mise tasks run codegen:check",
            "mise tasks r codegen:check",
            "mise run docs:check",
            "mise run check:client-tier",
            "mise run fmt",
            "mise run ts:build",
        )
        for command in valid_commands:
            task_path.write_text(
                original.replace(valid_command, command),
                encoding="utf-8",
            )
            errors = validate_plan_directory(plan_dir)
            if errors:
                print(
                    f"self-test failed: focused command `{command}` was rejected",
                    file=sys.stderr,
                )
                for error in errors:
                    print(error, file=sys.stderr)
                return 1

        completed_with_legacy_gate = _replace_metadata(
            original.replace(valid_command, "mise run pre-pr"),
            "Status",
            "Ready",
            "Complete",
        )
        task_path.write_text(completed_with_legacy_gate, encoding="utf-8")
        if validate_plan_directory(plan_dir):
            print(
                "self-test failed: completed task was retroactively policy-checked",
                file=sys.stderr,
            )
            return 1

        escaped = original.replace(
            f"- `{valid_command}`",
            "Exact-lane exception:\n"
            "- requirement: AC1\n"
            "- lane_owner: `mise.toml:test:journey:matrix`\n"
            "- narrowing_loss: Named tests cannot prove cross-language startup, "
            "routing, and teardown in one lane.\n"
            "`mise run test:journey:matrix`",
        )
        task_path.write_text(escaped, encoding="utf-8")
        if validate_plan_directory(plan_dir):
            print("self-test failed: documented exact-lane exception was rejected", file=sys.stderr)
            return 1

        boundary_exception = original.replace(
            f"- `{valid_command}`",
            "Exact-lane exception:\n"
            "- requirement: AC1\n"
            "- lane_owner: `scripts/checks/check-client-boundary.sh`\n"
            "- narrowing_loss: Direct package commands cannot reproduce the "
            "boundary script's cross-manifest dependency audit.\n"
            "`scripts/checks/check-client-boundary.sh`",
        )
        task_path.write_text(boundary_exception, encoding="utf-8")
        boundary_errors = validate_plan_directory(plan_dir)
        if boundary_errors:
            print(
                "self-test failed: exception-bound boundary script was rejected",
                file=sys.stderr,
            )
            for error in boundary_errors:
                print(error, file=sys.stderr)
            return 1

        escape_abuse = (
            "Exact-lane exception:\n"
            "- requirement: AC99\n"
            "- lane_owner: `unknown-owner`\n"
            "- narrowing_loss: Named tests omit acceptance proof.\n"
            "`mise run test:journey:matrix`"
        )
        task_path.write_text(
            original.replace(f"- `{valid_command}`", escape_abuse),
            encoding="utf-8",
        )
        if not validate_plan_directory(plan_dir):
            print("self-test failed: malformed exact-lane exception was accepted", file=sys.stderr)
            return 1

        duplicate_escape = escaped.replace(
            "`mise run test:journey:matrix`",
            "`mise run test:journey:matrix`\n\n"
            "Exact-lane exception:\n"
            "- requirement: AC1\n"
            "- lane_owner: `mise.toml:test:journey:matrix`\n"
            "- narrowing_loss: Named tests cannot prove cross-language startup, "
            "routing, and teardown in one lane.\n"
            "`mise run test:journey:matrix`",
            1,
        )
        task_path.write_text(duplicate_escape, encoding="utf-8")
        if not validate_plan_directory(plan_dir):
            print("self-test failed: reused exact-lane marker was accepted", file=sys.stderr)
            return 1

        mismatched_owner = escaped.replace(
            "mise.toml:test:journey:matrix",
            "mise.toml:test:cluster:all",
        )
        task_path.write_text(mismatched_owner, encoding="utf-8")
        if not validate_plan_directory(plan_dir):
            print("self-test failed: mismatched lane owner was accepted", file=sys.stderr)
            return 1

        orphan_escape = original.replace(
            f"- `{valid_command}`",
            "Exact-lane exception:\n"
            "- requirement: AC1\n"
            "- lane_owner: `mise.toml:test:journey:matrix`\n"
            "- narrowing_loss: Named tests cannot prove cross-language startup, "
            "routing, and teardown in one lane.\n\n"
            "Some prose.\n\n`mise run test:journey:matrix`",
        )
        task_path.write_text(orphan_escape, encoding="utf-8")
        if not validate_plan_directory(plan_dir):
            print("self-test failed: orphan exact-lane exception was accepted", file=sys.stderr)
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
