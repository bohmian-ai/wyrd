#!/usr/bin/env python3
"""Validate ephemeral Wyrd plan-controller transitions and gates.

The controller is the only caller. This script validates ephemeral facts at
dispatch, acceptance, transition, and closeout boundaries; it does not create a
ledger, persist state, or replace the canonical plan and task packets. Those
remain the durable source of truth, so a failed run here never loses progress.
"""

from __future__ import annotations

import argparse
import re
import sys

STATES = (
    "READY",
    "IMPLEMENTING",
    "REVIEWING",
    "REMEDIATING",
    "ACCEPTED",
    "EXTERNAL_BLOCKED",
)

TRANSITIONS = {
    ("READY", "IMPLEMENTING"),
    ("IMPLEMENTING", "REVIEWING"),
    ("IMPLEMENTING", "REMEDIATING"),
    ("REVIEWING", "ACCEPTED"),
    ("REVIEWING", "REMEDIATING"),
    ("REMEDIATING", "IMPLEMENTING"),
    ("REMEDIATING", "REVIEWING"),
}

REVIEW_VERDICTS = (
    "APPROVE",
    "RESUME_IMPLEMENTATION",
    "ORCHESTRATOR_DECISION_REQUIRED",
    "REVIEW_BLOCKED",
)

SHA = re.compile(r"[0-9a-f]{7,40}")


def fail(message: str) -> int:
    """Print one controller validation failure and return a failing exit code."""

    print(message, file=sys.stderr)
    return 1


def validate_transition(arguments: argparse.Namespace) -> int:
    """Validate one controller-state transition.

    `EXTERNAL_BLOCKED` is reachable from any nonterminal state but requires a
    named unavailable authority, so that genuine external blockage cannot be
    claimed as a shortcut past a difficult task.
    """

    if arguments.to_state == "EXTERNAL_BLOCKED":
        if arguments.from_state in ("ACCEPTED", "EXTERNAL_BLOCKED"):
            return fail(
                f"invalid transition: {arguments.from_state} -> EXTERNAL_BLOCKED"
            )
        if not arguments.external_authority:
            return fail(
                "EXTERNAL_BLOCKED requires --external-authority naming the "
                "unavailable credential, permission, infrastructure, or action"
            )
        print(f"valid transition: {arguments.from_state} -> EXTERNAL_BLOCKED")
        return 0

    if (arguments.from_state, arguments.to_state) not in TRANSITIONS:
        return fail(
            f"invalid transition: {arguments.from_state} -> {arguments.to_state}"
        )

    print(f"valid transition: {arguments.from_state} -> {arguments.to_state}")
    return 0


def parse_dependency(value: str) -> tuple[str, str]:
    """Parse a dependency argument as TASK=STATE."""

    task, separator, state = value.partition("=")
    if not separator or not task or state not in STATES:
        raise argparse.ArgumentTypeError(
            "dependency must be TASK=STATE with a valid controller state"
        )
    return task, state


def validate_dispatch(arguments: argparse.Namespace) -> int:
    """Validate that a Ready task may be dispatched to an implementor.

    Every named dependency must already be `ACCEPTED`; the controller keeps one
    active write task, so dispatching over an unaccepted dependency would build
    on an unreviewed base.
    """

    if arguments.state != "READY":
        return fail("dispatch requires --state READY")

    incomplete = [
        f"{task}={state}" for task, state in arguments.dependency if state != "ACCEPTED"
    ]
    if incomplete:
        return fail("dispatch dependencies are not ACCEPTED: " + ", ".join(incomplete))

    print(f"valid dispatch: {arguments.task}")
    return 0


def validate_accept(arguments: argparse.Namespace) -> int:
    """Validate task acceptance evidence.

    Acceptance requires a terminal COMPLETE implementation, passing focused
    proof, an APPROVE verdict, zero unresolved findings, a resolvable diff
    base, and a fully cited traceability matrix. The citation check is the
    machine-checkable half of the controller's review audit: a matrix row with
    no source citation cannot establish conformance, so an APPROVE carrying one
    is rejected here rather than trusted.
    """

    if arguments.state != "REVIEWING":
        return fail("acceptance requires --state REVIEWING")
    if arguments.implementation != "COMPLETE":
        return fail("acceptance requires --implementation COMPLETE")
    if arguments.focused_verification != "PASS":
        return fail("acceptance requires --focused-verification PASS")
    if arguments.review != "APPROVE":
        return fail("acceptance requires --review APPROVE")
    if arguments.unresolved_findings != 0:
        return fail("acceptance requires --unresolved-findings 0")
    if not SHA.fullmatch(arguments.diff_base):
        return fail("acceptance requires a 7-40 character lowercase hex --diff-base")
    if arguments.matrix_rows < 1:
        return fail("acceptance requires --matrix-rows >= 1")
    if arguments.cited_rows > arguments.matrix_rows:
        return fail("--cited-rows cannot exceed --matrix-rows")
    if arguments.cited_rows != arguments.matrix_rows:
        uncited = arguments.matrix_rows - arguments.cited_rows
        return fail(
            f"acceptance requires every traceability row cited: {uncited} of "
            f"{arguments.matrix_rows} rows lack a path:line citation; reject "
            "this APPROVE and re-review"
        )

    print(f"valid acceptance: {arguments.task}")
    return 0


def parse_task(value: str) -> tuple[str, str]:
    """Parse a closeout task argument as TASK=STATE."""

    return parse_dependency(value)


def validate_closeout(arguments: argparse.Namespace) -> int:
    """Validate whole-plan closeout evidence.

    Closeout demands every task accepted, passing integrated proof, an APPROVE
    from the fresh plan-wide review, and no unresolved finding.
    """

    if not arguments.task:
        return fail("closeout requires at least one --task TASK=ACCEPTED")

    incomplete = [
        f"{task}={state}" for task, state in arguments.task if state != "ACCEPTED"
    ]
    if incomplete:
        return fail("closeout tasks are not ACCEPTED: " + ", ".join(incomplete))
    if arguments.integrated_verification != "PASS":
        return fail("closeout requires --integrated-verification PASS")
    if arguments.final_review != "APPROVE":
        return fail("closeout requires --final-review APPROVE")
    if arguments.unresolved_findings != 0:
        return fail("closeout requires --unresolved-findings 0")

    print("valid plan closeout")
    return 0


def parser() -> argparse.ArgumentParser:
    """Build the controller-state command-line parser."""

    root = argparse.ArgumentParser()
    commands = root.add_subparsers(dest="command", required=True)

    transition = commands.add_parser("transition")
    transition.add_argument("--from-state", choices=STATES, required=True)
    transition.add_argument("--to-state", choices=STATES, required=True)
    transition.add_argument("--external-authority")
    transition.set_defaults(handler=validate_transition)

    dispatch = commands.add_parser("dispatch")
    dispatch.add_argument("--task", required=True)
    dispatch.add_argument("--state", choices=STATES, required=True)
    dispatch.add_argument(
        "--dependency",
        action="append",
        default=[],
        type=parse_dependency,
    )
    dispatch.set_defaults(handler=validate_dispatch)

    accept = commands.add_parser("accept")
    accept.add_argument("--task", required=True)
    accept.add_argument("--state", choices=STATES, required=True)
    accept.add_argument(
        "--implementation",
        choices=("COMPLETE", "BLOCKED"),
        required=True,
    )
    accept.add_argument(
        "--focused-verification",
        choices=("PASS", "FAIL", "UNVERIFIED"),
        required=True,
    )
    accept.add_argument("--review", choices=REVIEW_VERDICTS, required=True)
    accept.add_argument("--unresolved-findings", type=int, required=True)
    accept.add_argument("--diff-base", required=True)
    accept.add_argument(
        "--matrix-rows",
        type=int,
        required=True,
        help="Total requirement/AC rows in the reviewer's traceability matrix.",
    )
    accept.add_argument(
        "--cited-rows",
        type=int,
        required=True,
        help="Rows carrying a concrete path:line citation into the delta.",
    )
    accept.set_defaults(handler=validate_accept)

    closeout = commands.add_parser("closeout")
    closeout.add_argument("--task", action="append", type=parse_task, default=[])
    closeout.add_argument(
        "--integrated-verification",
        choices=("PASS", "FAIL", "UNVERIFIED"),
        required=True,
    )
    closeout.add_argument("--final-review", choices=REVIEW_VERDICTS, required=True)
    closeout.add_argument("--unresolved-findings", type=int, required=True)
    closeout.set_defaults(handler=validate_closeout)

    return root


def main() -> int:
    """Validate the requested controller boundary."""

    arguments = parser().parse_args()
    return arguments.handler(arguments)


if __name__ == "__main__":
    raise SystemExit(main())
