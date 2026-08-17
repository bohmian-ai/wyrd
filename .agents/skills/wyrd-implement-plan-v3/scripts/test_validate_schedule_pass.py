"""Tests for work-conserving v3 scheduling reports."""

from __future__ import annotations

import unittest

from validate_schedule_pass import validate


def valid_report() -> dict:
    """Build a report that dispatches both independent eligible tasks."""
    return {
        "eligible": ["T6", "T7"],
        "implementors": ["T6", "T7"],
        "proof_lanes": [],
        "reviewers": [],
        "advisors": [],
        "frozen": [],
        "dependency_blocked": ["T8"],
        "selected_but_idle": {},
    }


class SchedulePassTests(unittest.TestCase):
    """Reject silent idling and ambiguous resource accounting."""

    def test_parallel_dispatch_is_valid(self) -> None:
        """Accept dispatch of both independent tasks in the approved schedule."""
        self.assertEqual(validate(valid_report()), [])

    def test_silent_idle_task_is_rejected(self) -> None:
        """Reject an eligible T7 omitted while only T6 has an implementor."""
        report = valid_report()
        report["implementors"] = ["T6"]
        self.assertIn(
            "T7: eligible task is neither dispatched nor explicitly idle",
            validate(report),
        )

    def test_false_capacity_reason_is_rejected(self) -> None:
        """Reject capacity as a reason while fewer than three workers run."""
        report = valid_report()
        report["implementors"] = ["T6"]
        report["selected_but_idle"] = {"T7": {"reason": "agent_capacity"}}
        self.assertIn(
            "T7: agent capacity claimed with a free implementor slot",
            validate(report),
        )

    def test_reviewers_are_counted_separately(self) -> None:
        """Allow reviewers without treating them as implementation allocations."""
        report = valid_report()
        report["reviewers"] = ["T5"]
        self.assertEqual(validate(report), [])


if __name__ == "__main__":
    unittest.main()
