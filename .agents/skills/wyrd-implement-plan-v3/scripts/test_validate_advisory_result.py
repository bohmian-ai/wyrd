"""Tests for decisive v3 material-advisory results."""

from __future__ import annotations

import unittest

from validate_advisory_result import validate


def valid_result() -> dict:
    """Build one complete advisory recommendation."""
    return {
        "recommendation": "Bound encoded Iceberg allocation before decode.",
        "selected_option_count": 1,
        "rationale": "The bound preserves task-local memory admission.",
        "rejected_alternatives": ["Unbounded decode", "Planner-owned choice"],
        "evidence": ["crates/vala/example.rs:42"],
        "authorities": ["architecture/wyrd-design.md"],
        "impact_cone": "T6 only",
        "plan_boundary": {
            "changes_objective": False,
            "changes_dag": False,
            "changes_cross_task_ownership": False,
        },
        "delegates_decision": False,
    }


class AdvisoryResultTests(unittest.TestCase):
    """Reject advisory output that fails to make one recommendation."""

    def test_complete_recommendation_is_valid(self) -> None:
        """Accept one evidence-bound recommendation with an explicit boundary."""
        self.assertEqual(validate(valid_result()), [])

    def test_alternatives_without_selection_are_rejected(self) -> None:
        """Reject an advisor that enumerates choices without choosing one."""
        result = valid_result()
        result["recommendation"] = ""
        result["selected_option_count"] = 0
        errors = validate(result)
        self.assertIn("recommendation must select one concrete decision", errors)
        self.assertIn("selected_option_count must equal one", errors)

    def test_delegating_to_planning_is_rejected(self) -> None:
        """Reject an advisor that asks another workflow to make the decision."""
        result = valid_result()
        result["delegates_decision"] = True
        self.assertIn("delegates_decision must be false", validate(result))


if __name__ == "__main__":
    unittest.main()
