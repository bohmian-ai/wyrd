#!/usr/bin/env python3
"""Reconstruct Wyrd plan-controller state from durable artifacts alone.

The controller keeps no ledger: the canonical plan, task packets, commit
history, and current diff are the only authority, and conversation history is
explicitly not. That makes resume possible across a session boundary but easy
to get subtly wrong when re-derived by hand. This script performs the
reconstruction deterministically so a resuming controller reads state instead
of inferring it.

It is read-only. It never edits a task, creates a commit, or touches the
worktree.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

TASK_STATUS = re.compile(r"^Status:[ \t]*(Planned|Ready|Complete|Blocked)[ \t]*$", re.M)
PLAN_STATUS = re.compile(
    r"^Status:[ \t]*(Draft|Review Required|Approved)[ \t]*$", re.M
)
DEPENDS_ON = re.compile(r"^Depends on:[ \t]*(.+?)[ \t]*$", re.M)
RISK_TIER = re.compile(r"^Assigned model:[ \t]*(Luna|Terra|Sol)[ \t]*$", re.M)
TASK_ID = re.compile(r"^#[ \t]*(T[0-9]+):", re.M)


class ResumeError(Exception):
    """A durable artifact required for reconstruction is missing or unreadable."""


def git(worktree: Path, *arguments: str) -> str:
    """Run one read-only git command inside the execution worktree.

    # Errors

    Raises `ResumeError` when git is unavailable or returns a nonzero status,
    which for the commands used here means the worktree or ref is not resolvable.
    """

    try:
        completed = subprocess.run(
            ("git", "-C", str(worktree), *arguments),
            capture_output=True,
            text=True,
            check=True,
        )
    except FileNotFoundError as error:
        raise ResumeError("git is not available on PATH") from error
    except subprocess.CalledProcessError as error:
        raise ResumeError(
            f"git {' '.join(arguments)} failed: {error.stderr.strip()}"
        ) from error
    return completed.stdout.strip()


def read_task(path: Path) -> dict[str, object]:
    """Parse the durable metadata a controller needs from one task packet.

    # Errors

    Raises `ResumeError` when the packet is missing or has no parsable `Status`,
    because a task whose status cannot be read cannot be safely resumed.
    """

    if not path.is_file():
        raise ResumeError(f"task packet not found: {path}")
    text = path.read_text(encoding="utf-8")

    status = TASK_STATUS.search(text)
    if not status:
        raise ResumeError(f"task packet has no parsable Status: {path}")

    identifier = TASK_ID.search(text)
    depends = DEPENDS_ON.search(text)
    tier = RISK_TIER.search(text)

    raw_depends = depends.group(1) if depends else "None"
    dependencies = (
        []
        if raw_depends.strip() == "None"
        else [entry.strip() for entry in raw_depends.split(",") if entry.strip()]
    )

    return {
        "id": identifier.group(1) if identifier else path.stem,
        "packet": str(path),
        "status": status.group(1),
        "dependsOn": dependencies,
        "riskTier": tier.group(1) if tier else None,
    }


def controller_state(task: dict[str, object], has_changes: bool) -> str:
    """Map a canonical task status plus diff evidence to a controller state.

    The canonical statuses collapse several controller states: `Ready` covers
    IMPLEMENTING, REVIEWING, and REMEDIATING alike. Uncommitted changes in the
    worktree distinguish a task already under way from one not yet dispatched,
    which is the difference between resuming an implementor and starting one.
    """

    status = task["status"]
    if status == "Complete":
        return "ACCEPTED"
    if status == "Blocked":
        return "EXTERNAL_BLOCKED"
    if status == "Planned":
        return "NOT_READY"
    return "IMPLEMENTING" if has_changes else "READY"


def reconstruct(arguments: argparse.Namespace) -> dict[str, object]:
    """Rebuild controller state from the plan directory and execution worktree.

    # Errors

    Raises `ResumeError` when the plan directory, plan file, tasks directory, or
    a referenced task packet cannot be read, or when git cannot resolve the
    worktree. A partial reconstruction is never returned; the controller must
    surface the failure rather than resume on a guess.
    """

    plan_directory = Path(arguments.plan_dir).resolve()
    plan_file = plan_directory / "implementation-plan.md"
    tasks_directory = plan_directory / "tasks"

    if not plan_file.is_file():
        raise ResumeError(f"plan not found: {plan_file}")
    if not tasks_directory.is_dir():
        raise ResumeError(f"tasks directory not found: {tasks_directory}")

    plan_status = PLAN_STATUS.search(plan_file.read_text(encoding="utf-8"))

    tasks = [read_task(path) for path in sorted(tasks_directory.glob("*.md"))]
    if not tasks:
        raise ResumeError(f"no task packets in {tasks_directory}")

    worktree = Path(arguments.worktree).resolve()
    branch = git(worktree, "rev-parse", "--abbrev-ref", "HEAD")
    head = git(worktree, "rev-parse", "HEAD")
    dirty = bool(git(worktree, "status", "--porcelain"))

    commits: list[str] = []
    if arguments.baseline:
        raw = git(worktree, "log", "--oneline", f"{arguments.baseline}..HEAD")
        commits = raw.splitlines() if raw else []

    accepted = {task["id"] for task in tasks if task["status"] == "Complete"}

    active = None
    for task in tasks:
        if task["status"] != "Complete":
            active = dict(task)
            active["controllerState"] = controller_state(task, dirty)
            active["dependenciesAccepted"] = all(
                dependency in accepted for dependency in task["dependsOn"]
            )
            break

    return {
        "planDir": str(plan_directory),
        "planStatus": plan_status.group(1) if plan_status else None,
        "branch": branch,
        "head": head,
        "worktreeDirty": dirty,
        "commitsSinceBaseline": commits,
        "tasks": tasks,
        "activeTask": active,
        "complete": active is None,
        # Prior agents never survive a session boundary. Any implementor or
        # reviewer from the previous run is gone; the controller respawns from
        # these durable artifacts rather than trying to reattach.
        "priorAgentsAssumedDead": True,
    }


def main() -> int:
    """Print reconstructed controller state as JSON.

    # Errors

    Returns 1 and prints the reason to stderr when reconstruction fails.
    """

    parser = argparse.ArgumentParser(
        description="Reconstruct Wyrd controller state from durable artifacts."
    )
    parser.add_argument("--plan-dir", required=True)
    parser.add_argument("--worktree", required=True)
    parser.add_argument(
        "--baseline",
        help="Execution-baseline SHA. When given, lists commits since it.",
    )
    arguments = parser.parse_args()

    try:
        print(json.dumps(reconstruct(arguments), indent=2))
    except ResumeError as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
