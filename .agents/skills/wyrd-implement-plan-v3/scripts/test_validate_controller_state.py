"""Negative and replay tests for the v3 controller validator."""

from __future__ import annotations

import copy
import hashlib
import subprocess
import tempfile
import unittest
from pathlib import Path

from validate_controller_state import commit_diff, digest, validate

SHA_A = "a" * 40
SHA_B = "b" * 40
SHA_C = "c" * 40
DIGEST_D = "d" * 64
DIGEST_E = "e" * 64
DIGEST_F = "f" * 64


def task(status: str = "ready") -> dict:
    """Build one complete task projection for a selected status."""
    value = {
        "generation": 1,
        "digest": DIGEST_D,
        "revision": 1,
        "dependencies": [],
        "locks": [],
        "write_set": ["crates/a"],
        "status": status,
        "base_sha": SHA_B,
        "candidate_sha": SHA_A if status in {"candidate", "accepted", "integrated"} else None,
        "candidate_parent": SHA_B if status in {"candidate", "accepted", "integrated"} else None,
        "diff_digest": DIGEST_E if status in {"candidate", "accepted", "integrated"} else None,
        "patch_id": SHA_C if status in {"candidate", "accepted", "integrated"} else None,
        "proof_attempts": [],
        "accepted_proof": None,
        "review": None,
        "integrated_commit": None,
        "integrated_parent": None,
        "evidence_commit": None,
    }
    if status in {"accepted", "integrated"}:
        value["proof_attempts"] = [{
            "ordinal": 1, "candidate_sha": SHA_A, "generation": 1,
            "command": ["mise", "run", "test:focused"], "cargo_lane": "cargo-1",
            "stateful": False, "exit_code": 0, "selected_count": 3,
            "classification": "pass", "artifact_digest": DIGEST_F,
        }]
        value["accepted_proof"] = {"ordinal": 1, "artifact_digest": DIGEST_F}
    if status == "integrated":
        value["integrated_commit"] = SHA_C
        value["integrated_parent"] = SHA_B
        value["evidence_commit"] = SHA_B
    return value


def seal(state: dict) -> dict:
    """Rebuild the event hash chain and final replay projection."""
    previous = "0" * 64
    for event in state["events"]:
        event["previous_hash"] = previous
        event.pop("hash", None)
        event["hash"] = digest(event)
        previous = event["hash"]
    return state


def valid_state(lane_count: int = 2) -> dict:
    """Build a valid integrated snapshot with zero, one, or two Cargo lanes."""
    ready_projection = {
        "initial_head": SHA_B,
        "integration_head": SHA_B,
        "evidence_commit": None,
        "integration_order": [],
        "lanes": {"cargo": [], "stateful_owner": None, "review_owners": []},
        "tasks": {"T1": task("ready")},
    }
    for index in range(lane_count):
        ready_projection["lanes"]["cargo"].append({
            "name": f"cargo-{index + 1}", "worktree": f"/tmp/w{index + 1}",
            "target_dir": f"/tmp/t{index + 1}", "owner": None,
        })
    first = {"sequence": 1, "transition": "initialize", "task_id": None,
             "previous_hash": "0" * 64, "mutable": ready_projection}
    first["hash"] = digest(first)
    active = copy.deepcopy(ready_projection)
    active["tasks"]["T1"]["status"] = "active"
    second = {"sequence": 2, "transition": "dispatch", "task_id": "T1",
              "previous_hash": first["hash"], "mutable": active}
    second["hash"] = digest(second)
    candidate = copy.deepcopy(active)
    candidate["tasks"]["T1"] = task("candidate")
    candidate["lanes"]["review_owners"] = ["T1"]
    third = {"sequence": 3, "transition": "candidate", "task_id": "T1",
             "previous_hash": second["hash"], "mutable": candidate}
    third["hash"] = digest(third)
    accepted = copy.deepcopy(candidate)
    accepted["tasks"]["T1"] = task("accepted")
    if lane_count == 0:
        accepted["tasks"]["T1"]["proof_attempts"][0]["cargo_lane"] = None
    accepted["tasks"]["T1"]["review"] = {
        "verdict": "APPROVE", "candidate_sha": SHA_A, "generation": 1,
        "controller_event_hash": third["hash"], "result_digest": DIGEST_E,
    }
    accepted["lanes"]["review_owners"] = []
    fourth = {"sequence": 4, "transition": "accept", "task_id": "T1",
              "previous_hash": third["hash"], "mutable": accepted}
    fourth["hash"] = digest(fourth)
    integrated = copy.deepcopy(accepted)
    integrated["tasks"]["T1"]["status"] = "integrated"
    integrated["tasks"]["T1"]["integrated_commit"] = SHA_C
    integrated["tasks"]["T1"]["integrated_parent"] = SHA_B
    integrated["tasks"]["T1"]["evidence_commit"] = SHA_B
    integrated["lanes"]["review_owners"] = []
    integrated["integration_order"] = ["T1"]
    integrated["integration_head"] = SHA_C
    integrated["evidence_commit"] = SHA_B
    fifth = {"sequence": 5, "transition": "integrate", "task_id": "T1",
             "previous_hash": fourth["hash"], "mutable": integrated}
    fifth["hash"] = digest(fifth)
    return {"version": 1, "controller_id": "controller-test", "mutable": copy.deepcopy(integrated),
            "events": [first, second, third, fourth, fifth]}


def project(state: dict) -> None:
    """Copy current mutable state into the last event and reseal the chain."""
    state["events"][-1]["mutable"] = copy.deepcopy(state["mutable"])
    seal(state)


def reseal_with_review_identity(state: dict) -> None:
    """Reseal events and bind downstream reviews to the candidate event."""
    seal(state)
    candidate_hash = state["events"][2]["hash"]
    for event in state["events"][3:]:
        review = event["mutable"]["tasks"]["T1"].get("review")
        if review:
            review["controller_event_hash"] = candidate_hash
    seal(state)
    state["mutable"] = copy.deepcopy(state["events"][-1]["mutable"])


class ControllerStateTests(unittest.TestCase):
    """Exercise replay, scheduling, identity, and proof invariants."""

    def test_zero_one_and_two_cargo_lanes_are_valid(self) -> None:
        """Allow every configured lane count within the bounded protocol."""
        for count in (0, 1, 2):
            self.assertEqual(validate(valid_state(count)), [])

    def test_three_lanes_are_rejected(self) -> None:
        """Reject a third Cargo lane."""
        state = valid_state()
        state["mutable"]["lanes"]["cargo"].append({"name": "cargo-3", "worktree": "/tmp/w3", "target_dir": "/tmp/t3", "owner": None})
        project(state)
        self.assertIn("at most two Cargo lanes are permitted", validate(state))

    def test_hash_chain_tampering_is_rejected(self) -> None:
        """Reject an event whose content changed after hashing."""
        state = valid_state()
        state["events"][0]["transition"] = "tampered"
        self.assertTrue(any("hash mismatch" in error or "breaks chain" in error for error in validate(state)))

    def test_historical_projection_lane_violation_is_rejected(self) -> None:
        """Reject a transient third lane even when final state is safe."""
        state = valid_state()
        state["events"][0]["mutable"]["lanes"]["cargo"].append({
            "name": "cargo-3", "worktree": "/tmp/w3", "target_dir": "/tmp/t3", "owner": None,
        })
        reseal_with_review_identity(state)
        self.assertIn("event 1: at most two Cargo lanes are permitted", validate(state))

    def test_replay_projection_drift_is_rejected(self) -> None:
        """Reject mutable state not represented by the final event."""
        state = valid_state()
        state["mutable"]["integration_head"] = SHA_A
        self.assertIn("top-level mutable state does not match replayed final event", validate(state))

    def test_invalid_transition_is_rejected(self) -> None:
        """Reject a direct candidate-to-integrated event transition."""
        state = valid_state()
        state["events"].pop(3)
        state["events"][3]["sequence"] = 4
        seal(state)
        self.assertIn("event 4: invalid transition candidate->integrated", validate(state))

    def test_sha_and_diff_syntax_are_rejected(self) -> None:
        """Reject malformed Git and content identities."""
        state = valid_state()
        state["mutable"]["tasks"]["T1"]["candidate_sha"] = "not-a-sha"
        state["mutable"]["tasks"]["T1"]["diff_digest"] = "bad"
        project(state)
        errors = validate(state)
        self.assertIn("T1: invalid candidate_sha", errors)
        self.assertIn("T1: invalid diff identity", errors)

    def test_stale_candidate_generation_is_rejected(self) -> None:
        """Reject proof and review bound to an older generation."""
        state = valid_state()
        state["mutable"]["tasks"]["T1"]["generation"] = 2
        project(state)
        errors = validate(state)
        self.assertIn("T1: proof attempt has stale candidate generation", errors)
        self.assertIn("T1: review has stale candidate generation", errors)

    def test_missing_proof_and_review_are_rejected(self) -> None:
        """Reject integrated work without both acceptance gates."""
        state = valid_state()
        current = state["mutable"]["tasks"]["T1"]
        current["accepted_proof"] = None
        current["review"] = None
        project(state)
        errors = validate(state)
        self.assertIn("T1: missing accepted proof", errors)
        self.assertIn("T1: missing exact APPROVE review", errors)

    def test_review_controller_identity_is_rejected(self) -> None:
        """Reject approval made against an unknown controller snapshot."""
        state = valid_state()
        state["mutable"]["tasks"]["T1"]["review"]["controller_event_hash"] = "9" * 64
        project(state)
        self.assertIn("T1: reviewer controller-state identity is invalid", validate(state))

    def test_source_failure_rerun_is_rejected(self) -> None:
        """Require a successor candidate after a source/test failure."""
        state = valid_state()
        attempts = state["mutable"]["tasks"]["T1"]["proof_attempts"]
        attempts.insert(0, {**attempts[0], "ordinal": 1, "exit_code": 1, "selected_count": 1, "classification": "test", "artifact_digest": DIGEST_D})
        attempts[1]["ordinal"] = 2
        state["mutable"]["tasks"]["T1"]["accepted_proof"] = {"ordinal": 2, "artifact_digest": DIGEST_F}
        project(state)
        self.assertIn("T1: source/test failure was rerun without successor candidate", validate(state))

    def test_infrastructure_retry_then_one_pass_is_valid(self) -> None:
        """Allow auditable setup failure before the sole accepted PASS."""
        state = valid_state()
        attempts = state["mutable"]["tasks"]["T1"]["proof_attempts"]
        attempts.insert(0, {**attempts[0], "ordinal": 1, "exit_code": 2, "selected_count": 0, "classification": "infrastructure", "artifact_digest": DIGEST_D})
        attempts[1]["ordinal"] = 2
        state["mutable"]["tasks"]["T1"]["accepted_proof"] = {"ordinal": 2, "artifact_digest": DIGEST_F}
        project(state)
        self.assertEqual(validate(state), [])

    def test_write_and_semantic_lock_conflicts_are_rejected(self) -> None:
        """Reject simultaneously allocated tasks sharing paths or locks."""
        state = valid_state()
        state["mutable"]["tasks"]["T1"] = task("candidate")
        state["mutable"]["tasks"]["T1"]["locks"] = ["schema:cards"]
        second = task("candidate")
        second["write_set"] = ["crates/a/src"]
        second["locks"] = ["schema:cards"]
        state["mutable"]["tasks"]["T2"] = second
        state["mutable"]["integration_order"] = []
        state["mutable"]["integration_head"] = SHA_B
        state["mutable"]["evidence_commit"] = None
        project(state)
        errors = validate(state)
        self.assertIn("write-set overlap: T1 and T2", errors)
        self.assertIn("semantic lock conflict: T1 and T2", errors)

    def test_dependency_cycle_and_order_are_rejected(self) -> None:
        """Reject cyclic dependencies and reversed integration order."""
        state = valid_state()
        state["mutable"]["tasks"]["T2"] = copy.deepcopy(state["mutable"]["tasks"]["T1"])
        state["mutable"]["tasks"]["T1"]["dependencies"] = ["T2"]
        state["mutable"]["tasks"]["T2"]["dependencies"] = ["T1"]
        state["mutable"]["integration_order"] = ["T1", "T2"]
        project(state)
        errors = validate(state)
        self.assertTrue(any("dependency cycle" in error for error in errors))
        self.assertTrue(any("integration order violates" in error for error in errors))

    def test_lane_conflicts_are_rejected(self) -> None:
        """Reject duplicate worktrees, targets, and owners."""
        state = valid_state()
        lanes = state["mutable"]["lanes"]["cargo"]
        state["mutable"]["tasks"]["T1"]["status"] = "accepted"
        lanes[0]["owner"] = "T1"
        lanes[1]["owner"] = "T1"
        lanes[1]["worktree"] = lanes[0]["worktree"]
        lanes[1]["target_dir"] = lanes[0]["target_dir"]
        project(state)
        errors = validate(state)
        self.assertIn("Cargo lane worktrees must be distinct and nonempty", errors)
        self.assertIn("Cargo target directories must be distinct and nonempty", errors)
        self.assertIn("a task cannot own multiple Cargo lanes", errors)

    def test_repository_ancestry_and_diff_identity_are_enforced(self) -> None:
        """Validate real candidate ancestry and reject a corrupted diff digest."""
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.name", "State Test"], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "user.email", "state@example.invalid"], check=True)
            source = repo / "value.txt"
            source.write_text("base\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "add", "value.txt"], check=True)
            subprocess.run(["git", "-C", str(repo), "commit", "-qm", "base"], check=True)
            base = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
            source.write_text("candidate\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(repo), "commit", "-qam", "candidate"], check=True)
            candidate = subprocess.check_output(["git", "-C", str(repo), "rev-parse", "HEAD"], text=True).strip()
            diff_digest, patch_id = commit_diff(str(repo), base, candidate)

            state = valid_state()
            state["repo_root"] = str(repo)
            for event in state["events"]:
                event["mutable"]["initial_head"] = base
                if event["mutable"]["tasks"]["T1"]["status"] != "integrated":
                    event["mutable"]["integration_head"] = base
                current = event["mutable"]["tasks"]["T1"]
                if current["status"] in {"candidate", "accepted", "integrated"}:
                    current["base_sha"] = base
                    current["candidate_sha"] = candidate
                    current["candidate_parent"] = base
                    current["diff_digest"] = diff_digest
                    current["patch_id"] = patch_id
                    for attempt in current["proof_attempts"]:
                        attempt["candidate_sha"] = candidate
                    if current["review"]:
                        current["review"]["candidate_sha"] = candidate
                if current["status"] == "integrated":
                    current["integrated_commit"] = candidate
                    current["integrated_parent"] = base
                    event["mutable"]["integration_head"] = candidate
            seal(state)
            candidate_event_hash = state["events"][2]["hash"]
            state["events"][3]["mutable"]["tasks"]["T1"]["review"]["controller_event_hash"] = candidate_event_hash
            state["events"][4]["mutable"]["tasks"]["T1"]["review"]["controller_event_hash"] = candidate_event_hash
            seal(state)
            state["mutable"] = copy.deepcopy(state["events"][-1]["mutable"])
            self.assertEqual(validate(state), [])

            serial_state = copy.deepcopy(state)
            for event in serial_state["events"]:
                event["mutable"]["initial_head"] = candidate
            reseal_with_review_identity(serial_state)
            self.assertIn("T1: integrated commit is not the direct serial successor", validate(serial_state))

            state["mutable"]["tasks"]["T1"]["diff_digest"] = hashlib.sha256(b"wrong").hexdigest()
            project(state)
            self.assertIn("T1: candidate diff identity mismatch", validate(state))


if __name__ == "__main__":
    unittest.main()
