#!/usr/bin/env python3
"""Validate one decisive, evidence-bound v3 material-advisory result."""

from __future__ import annotations

import json
import sys
from typing import Any


def nonempty_strings(value: Any) -> bool:
    """Return whether a value is a nonempty list of nonempty strings."""
    return isinstance(value, list) and bool(value) and all(
        isinstance(item, str) and item.strip() for item in value
    )


def validate(result: dict[str, Any]) -> list[str]:
    """Return errors for an advisory result that cannot support a root decision."""
    errors: list[str] = []
    recommendation = result.get("recommendation")
    if not isinstance(recommendation, str) or not recommendation.strip():
        errors.append("recommendation must select one concrete decision")
    if result.get("selected_option_count") != 1:
        errors.append("selected_option_count must equal one")
    for field in ("rationale", "impact_cone"):
        if not isinstance(result.get(field), str) or not result[field].strip():
            errors.append(f"{field} must be nonempty")
    for field in ("rejected_alternatives", "evidence", "authorities"):
        if not nonempty_strings(result.get(field)):
            errors.append(f"{field} must be a nonempty list of nonempty strings")
    boundary = result.get("plan_boundary")
    required = {"changes_objective", "changes_dag", "changes_cross_task_ownership"}
    if (
        not isinstance(boundary, dict)
        or set(boundary) != required
        or not all(isinstance(boundary[key], bool) for key in required)
    ):
        errors.append("plan_boundary must contain exactly three boolean change flags")
    if result.get("delegates_decision") is not False:
        errors.append("delegates_decision must be false")
    return sorted(errors)


def main() -> int:
    """Validate one JSON artifact and return a shell-compatible status."""
    if len(sys.argv) != 2:
        print("usage: validate_advisory_result.py RESULT.json", file=sys.stderr)
        return 2
    with open(sys.argv[1], encoding="utf-8") as handle:
        result = json.load(handle)
    errors = validate(result)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("advisory result valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
