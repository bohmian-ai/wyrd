"""Negative and replay tests for the v3 controller validator."""

from __future__ import annotations

import copy
import hashlib
import subprocess
import tempfile
import unittest
from pathlib import Path

from validate_controller_state import commit_diff, digest, validate, validate_projection

SHA_A = "a" * 40
SHA_B = "b" * 40
SHA_C = "c" * 40
DIGEST_D = "d" * 64
DIGEST_E = "e" * 64
DIGEST_F = "f" * 64
PROOF_FOCUSED = "focused"


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
        "proof_requirements": {PROOF_FOCUSED: {"prerequisites": []}},
        "proof_attempts": {PROOF_FOCUSED: []},
        "accepted_proofs": {},
        "review": None,
        "integrated_commit": None,
        "integrated_parent": None,
        "evidence_commit": None,
    }
    if status in {"accepted", "integrated"}:
        value["proof_attempts"][PROOF_FOCUSED] = [{
            "proof_id": PROOF_FOCUSED, "ordinal": 1, "candidate_sha": SHA_A, "generation": 1,
            "integration_head": SHA_B,
            "command": ["mise", "run", "test:focused"], "cargo_lane": "cargo-1",
            "stateful": False, "exit_code": 0, "selected_count": 3,
            "classification": "pass", "artifact_digest": DIGEST_F,
        }]
        value["accepted_proofs"][PROOF_FOCUSED] = {
            "proof_id": PROOF_FOCUSED, "ordinal": 1, "artifact_digest": DIGEST_F,
        }
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
        accepted["tasks"]["T1"]["proof_attempts"][PROOF_FOCUSED][0]["cargo_lane"] = None
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

    def test_empty_write_set_is_rejected(self) -> None:
        """Reject incomplete scheduling identity that hides source conflicts."""
        state = valid_state()
        state["mutable"]["tasks"]["T1"]["write_set"] = []
        project(state)
        self.assertIn(
            "T1: write_set must be a nonempty duplicate-free list of relative paths",
            validate(state),
        )

    def test_missing_lock_projection_is_rejected(self) -> None:
        """Require semantic-lock identity even when a task owns no locks."""
        state = valid_state()
        del state["mutable"]["tasks"]["T1"]["locks"]
        project(state)
        self.assertIn("T1: locks must be a duplicate-free list", validate(state))

    def test_stale_candidate_generation_is_rejected(self) -> None:
        """Reject proof and review bound to an older generation."""
        state = valid_state()
        state["mutable"]["tasks"]["T1"]["generation"] = 2
        project(state)
        errors = validate(state)
        self.assertIn("T1/focused: proof attempt has stale candidate generation", errors)
        self.assertIn("T1: review has stale candidate generation", errors)

    def test_missing_proof_and_review_are_rejected(self) -> None:
        """Reject integrated work without both acceptance gates."""
        state = valid_state()
        current = state["mutable"]["tasks"]["T1"]
        current["accepted_proofs"] = {}
        current["review"] = None
        project(state)
        errors = validate(state)
        self.assertIn("T1/focused: missing accepted proof", errors)
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
        accepted_event = state["events"][3]["mutable"]["tasks"]["T1"]
        passing = copy.deepcopy(accepted_event["proof_attempts"][PROOF_FOCUSED][0])
        failure = {**passing, "exit_code": 1, "classification": "test", "artifact_digest": DIGEST_D}
        accepted_event["status"] = "candidate"
        accepted_event["proof_attempts"][PROOF_FOCUSED] = [failure]
        accepted_event["accepted_proofs"] = {}
        accepted_event["review"] = None
        integrated_event = state["events"][4]["mutable"]["tasks"]["T1"]
        passing["ordinal"] = 2
        integrated_event["proof_attempts"][PROOF_FOCUSED] = [failure, passing]
        integrated_event["accepted_proofs"][PROOF_FOCUSED] = {
            "proof_id": PROOF_FOCUSED, "ordinal": 2, "artifact_digest": DIGEST_F,
        }
        reseal_with_review_identity(state)
        self.assertTrue(any("T1 source/test failure was rerun without successor candidate" in error for error in validate(state)))

    def test_proof_attempt_history_is_append_only(self) -> None:
        """Reject rewriting an earlier proof attempt in a later event projection."""
        state = valid_state()
        attempts = state["mutable"]["tasks"]["T1"]["proof_attempts"][PROOF_FOCUSED]
        attempts[0]["command"] = ["mise", "run", "rewritten-proof"]
        project(state)
        self.assertTrue(any("proof attempts are not append-only" in error for error in validate(state)))

    def test_infrastructure_retry_then_one_pass_is_valid(self) -> None:
        """Allow auditable setup failure before the sole accepted PASS."""
        state = valid_state()
        for event in state["events"]:
            current = event["mutable"]["tasks"]["T1"]
            if current["status"] in {"accepted", "integrated"}:
                attempts = current["proof_attempts"][PROOF_FOCUSED]
                attempts.insert(0, {**attempts[0], "ordinal": 1, "exit_code": 2, "selected_count": 0, "classification": "infrastructure", "artifact_digest": DIGEST_D})
                attempts[1]["ordinal"] = 2
                current["accepted_proofs"][PROOF_FOCUSED] = {
                    "proof_id": PROOF_FOCUSED, "ordinal": 2, "artifact_digest": DIGEST_F,
                }
        reseal_with_review_identity(state)
        self.assertEqual(validate(state), [])

    def test_multiple_required_proof_streams_are_accepted_independently(self) -> None:
        """Require and accept one sole PASS in each stable proof stream."""
        state = valid_state()
        for event in state["events"]:
            current = event["mutable"]["tasks"]["T1"]
            current["proof_requirements"]["contracts"] = {"prerequisites": []}
            current["proof_attempts"]["contracts"] = []
            if current["status"] in {"accepted", "integrated"}:
                attempt = copy.deepcopy(current["proof_attempts"][PROOF_FOCUSED][0])
                attempt.update({"proof_id": "contracts", "command": ["mise", "run", "codegen:check"], "artifact_digest": DIGEST_D})
                current["proof_attempts"]["contracts"] = [attempt]
                current["accepted_proofs"]["contracts"] = {
                    "proof_id": "contracts", "ordinal": 1, "artifact_digest": DIGEST_D,
                }
        reseal_with_review_identity(state)
        self.assertEqual(validate(state), [])

    def test_missing_one_of_multiple_required_proofs_is_rejected(self) -> None:
        """Reject acceptance when any declared proof stream lacks its PASS."""
        state = valid_state()
        for event in state["events"]:
            current = event["mutable"]["tasks"]["T1"]
            current["proof_requirements"]["contracts"] = {"prerequisites": []}
            current["proof_attempts"]["contracts"] = []
        reseal_with_review_identity(state)
        self.assertIn("T1/contracts: missing accepted proof", validate(state))

    def test_proof_prerequisites_do_not_block_implementation_dispatch(self) -> None:
        """Keep proof ordering separate from implementation dependency readiness."""
        projection = valid_state()["events"][1]["mutable"]
        projection["tasks"]["T2"] = task("ready")
        projection["tasks"]["T2"]["write_set"] = ["crates/b"]
        projection["tasks"]["T1"]["proof_requirements"][PROOF_FOCUSED]["prerequisites"] = ["T2"]
        self.assertEqual(validate_projection(projection, "dispatch"), [])

    def test_proof_requires_integrated_prerequisite_and_fresh_base(self) -> None:
        """Reject proof recorded before its prerequisite or against an older base."""
        state = valid_state()
        state["mutable"]["tasks"]["T2"] = task("ready")
        current = state["mutable"]["tasks"]["T1"]
        current["proof_requirements"][PROOF_FOCUSED]["prerequisites"] = ["T2"]
        current["proof_attempts"][PROOF_FOCUSED][0]["integration_head"] = SHA_C
        project(state)
        errors = validate(state)
        self.assertIn("T1/focused: proof prerequisite is not integrated", errors)
        self.assertIn("T1/focused: candidate predates proof prerequisite integration", errors)

    def test_unknown_proof_stream_is_rejected(self) -> None:
        """Reject attempts that are not declared by stable proof requirements."""
        state = valid_state()
        state["mutable"]["tasks"]["T1"]["proof_attempts"]["unknown"] = []
        project(state)
        self.assertIn("T1: unknown proof ID unknown", validate(state))

    def test_multiple_passes_in_one_stream_are_rejected(self) -> None:
        """Keep each required proof stream bound to exactly one PASS."""
        state = valid_state()
        attempts = state["mutable"]["tasks"]["T1"]["proof_attempts"][PROOF_FOCUSED]
        attempts.append({**attempts[0], "ordinal": 2, "artifact_digest": DIGEST_D})
        project(state)
        self.assertIn("T1/focused: proof stream has multiple PASS attempts", validate(state))

    def test_candidate_refresh_cycle_clears_evidence_and_uses_current_head(self) -> None:
        """Allow the mandated candidate-to-active-to-successor refresh cycle."""
        state = valid_state()
        state["events"] = state["events"][:3]
        stale = state["events"][-1]["mutable"]
        stale["lanes"]["review_owners"] = []
        active = copy.deepcopy(stale)
        current = active["tasks"]["T1"]
        current.update({
            "status": "active", "generation": 2, "base_sha": SHA_B,
            "candidate_sha": None, "candidate_parent": None,
            "diff_digest": None, "patch_id": None, "review": None,
        })
        current["proof_attempts"] = {PROOF_FOCUSED: []}
        current["accepted_proofs"] = {}
        state["events"].append({"sequence": 4, "transition": "refresh", "task_id": "T1", "mutable": active})
        successor = copy.deepcopy(active)
        successor["tasks"]["T1"].update({
            "status": "candidate", "candidate_sha": SHA_C,
            "candidate_parent": SHA_B, "diff_digest": DIGEST_E, "patch_id": SHA_A,
        })
        state["events"].append({"sequence": 5, "transition": "successor", "task_id": "T1", "mutable": successor})
        seal(state)
        state["mutable"] = copy.deepcopy(successor)
        self.assertEqual(validate(state), [])

    def test_successor_candidate_must_clear_all_proof_and_review_evidence(self) -> None:
        """Reject a fresh generation that retained evidence from its predecessor."""
        state = valid_state()
        accepted = state["events"][3]["mutable"]["tasks"]["T1"]
        accepted["candidate_sha"] = SHA_C
        accepted["generation"] = 2
        accepted["base_sha"] = SHA_B
        seal(state)
        self.assertIn("event 4: successor candidate retained proof or review evidence", validate(state))

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

    def test_combined_dependency_and_proof_cycle_is_rejected(self) -> None:
        """Reject deadlock cycles spanning source and proof prerequisite edges."""
        state = valid_state()
        second = task("ready")
        second["dependencies"] = ["T1"]
        second["write_set"] = ["crates/b"]
        state["mutable"]["tasks"]["T2"] = second
        state["mutable"]["tasks"]["T1"]["proof_requirements"][PROOF_FOCUSED]["prerequisites"] = ["T2"]
        project(state)
        self.assertTrue(any("combined dependency/proof cycle" in error for error in validate(state)))

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
                    for attempt in current["proof_attempts"][PROOF_FOCUSED]:
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

            candidate_state = copy.deepcopy(state)
            candidate_state["events"] = candidate_state["events"][:3]
            seal(candidate_state)
            candidate_state["mutable"] = copy.deepcopy(candidate_state["events"][-1]["mutable"])
            self.assertEqual(validate(candidate_state), [])
            candidate_state["mutable"]["tasks"]["T1"]["diff_digest"] = hashlib.sha256(b"candidate-wrong").hexdigest()
            project(candidate_state)
            self.assertIn("T1: candidate diff identity mismatch", validate(candidate_state))

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
