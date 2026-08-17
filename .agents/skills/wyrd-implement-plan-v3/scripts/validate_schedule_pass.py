#!/usr/bin/env python3
"""Validate one work-conserving v3 controller scheduling pass."""

from __future__ import annotations

import json
import sys
from typing import Any

REASONS = {
    "source_conflict",
    "semantic_conflict",
    "approved_serialization",
    "cargo_lane",
    "stateful_lane",
    "agent_capacity",
}


def unique_strings(value: Any) -> bool:
    """Return whether a value is a duplicate-free list of nonempty strings."""
    return (
        isinstance(value, list)
        and len(value) == len(set(value))
        and all(isinstance(item, str) and item.strip() for item in value)
    )


def validate(report: dict[str, Any]) -> list[str]:
    """Return errors for an incomplete or internally inconsistent schedule pass."""
    errors: list[str] = []
    collections: dict[str, list[str]] = {}
    for field in (
        "eligible",
        "implementors",
        "proof_lanes",
        "reviewers",
        "advisors",
        "frozen",
        "dependency_blocked",
    ):
        value = report.get(field)
        if not unique_strings(value):
            errors.append(f"{field} must be a duplicate-free list of nonempty task IDs")
            collections[field] = []
        else:
            collections[field] = value
    if len(collections["implementors"]) > 3:
        errors.append("at most three implementors may be allocated")
    if len(collections["reviewers"]) > 2:
        errors.append("at most two reviewers may be allocated")

    idle = report.get("selected_but_idle")
    if not isinstance(idle, dict):
        errors.append("selected_but_idle must map task IDs to concrete reasons")
        idle = {}
    for task_id, detail in idle.items():
        if task_id not in collections["eligible"]:
            errors.append(f"{task_id}: idle task is not eligible")
        reason = detail.get("reason") if isinstance(detail, dict) else None
        if reason not in REASONS:
            errors.append(f"{task_id}: invalid selected-but-idle reason")
        if reason == "agent_capacity" and len(collections["implementors"]) < 3:
            errors.append(f"{task_id}: agent capacity claimed with a free implementor slot")
        if reason in {"source_conflict", "semantic_conflict"}:
            conflicts_with = detail.get("conflicts_with") if isinstance(detail, dict) else None
            if not isinstance(conflicts_with, str) or not conflicts_with.strip():
                errors.append(f"{task_id}: conflict reason must identify conflicts_with")

    accounted = set(collections["implementors"]) | set(idle)
    for task_id in collections["eligible"]:
        if task_id not in accounted:
            errors.append(f"{task_id}: eligible task is neither dispatched nor explicitly idle")
    for task_id in collections["implementors"]:
        if task_id not in collections["eligible"]:
            errors.append(f"{task_id}: implementor task is not eligible")
    if set(collections["implementors"]) & set(idle):
        errors.append("a task cannot be both dispatched and selected-but-idle")
    return sorted(errors)


def main() -> int:
    """Validate one JSON scheduling report and return a shell-compatible status."""
    if len(sys.argv) != 2:
        print("usage: validate_schedule_pass.py REPORT.json", file=sys.stderr)
        return 2
    with open(sys.argv[1], encoding="utf-8") as handle:
        report = json.load(handle)
    errors = validate(report)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("schedule pass valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
