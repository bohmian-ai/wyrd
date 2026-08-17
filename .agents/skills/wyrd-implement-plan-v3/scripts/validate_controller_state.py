#!/usr/bin/env python3
"""Validate the hash-chained Wyrd v3 controller state machine."""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import PurePosixPath
from typing import Any

ACTIVE = {"active", "candidate", "accepted"}
STATUSES = {"ready", "active", "candidate", "accepted", "integrated", "frozen"}
VERDICTS = {"APPROVE", "RESUME_IMPLEMENTATION", "ORCHESTRATOR_DECISION_REQUIRED", "REVIEW_BLOCKED"}
TRANSITIONS = {
    ("ready", "active"), ("active", "candidate"), ("candidate", "active"),
    ("candidate", "accepted"), ("accepted", "integrated"),
    ("ready", "frozen"), ("active", "frozen"), ("candidate", "frozen"),
    ("accepted", "frozen"), ("frozen", "ready"),
}
HEX = re.compile(r"^[0-9a-f]+$")


def canonical(value: Any) -> bytes:
    """Encode JSON deterministically for event and artifact identity."""
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def digest(value: Any) -> str:
    """Return the SHA-256 digest of a canonical JSON value."""
    return hashlib.sha256(canonical(value)).hexdigest()


def is_digest(value: Any) -> bool:
    """Return whether a value is a lowercase SHA-256 digest."""
    return isinstance(value, str) and len(value) == 64 and bool(HEX.fullmatch(value))


def is_git_sha(value: Any) -> bool:
    """Return whether a value is a supported lowercase Git object ID."""
    return isinstance(value, str) and len(value) in {40, 64} and bool(HEX.fullmatch(value))


def overlaps(left: str, right: str) -> bool:
    """Return whether normalized repository paths overlap by ancestry."""
    a, b = PurePosixPath(left), PurePosixPath(right)
    return a == b or a in b.parents or b in a.parents


def git(repo: str, *args: str, input_text: str | None = None) -> str:
    """Run one read-only Git query and return stripped standard output."""
    return subprocess.run(
        ["git", "-C", repo, *args], input=input_text, text=True,
        capture_output=True, check=True,
    ).stdout.strip()


def commit_diff(repo: str, parent: str, commit: str) -> tuple[str, str]:
    """Return content digest and stable patch ID for one commit delta."""
    patch = subprocess.run(
        ["git", "-C", repo, "diff", "--binary", parent, commit],
        capture_output=True, check=True,
    ).stdout
    diff_digest = hashlib.sha256(patch).hexdigest()
    patch_id = subprocess.run(
        ["git", "-C", repo, "patch-id", "--stable"], input=patch,
        capture_output=True, check=True,
    ).stdout.decode().split()[0]
    return diff_digest, patch_id


def validate_projection(mutable: dict[str, Any], label: str) -> list[str]:
    """Validate scheduling invariants on one reconstructed projection."""
    errors: list[str] = []
    tasks = mutable.get("tasks", {})
    if not isinstance(tasks, dict):
        return [f"{label}: tasks must be an object"]
    active = [(task_id, task) for task_id, task in tasks.items() if task.get("status") in ACTIVE]
    if len(active) > 3:
        errors.append(f"{label}: more than three implementor allocations are active")
    for task_id, task in active:
        for dependency in task.get("dependencies", []):
            if dependency not in tasks or tasks[dependency].get("status") != "integrated":
                errors.append(f"{label}: {task_id} dependency {dependency} is not integrated")
    for index, (left_id, left) in enumerate(active):
        for right_id, right in active[index + 1:]:
            if any(overlaps(a, b) for a in left.get("write_set", []) for b in right.get("write_set", [])):
                errors.append(f"{label}: write-set overlap: {left_id} and {right_id}")
            if set(left.get("locks", [])) & set(right.get("locks", [])):
                errors.append(f"{label}: semantic lock conflict: {left_id} and {right_id}")
    lanes = mutable.get("lanes", {})
    cargo = lanes.get("cargo", []) if isinstance(lanes, dict) else []
    if len(cargo) > 2:
        errors.append(f"{label}: at most two Cargo lanes are permitted")
    worktrees = [lane.get("worktree") for lane in cargo]
    targets = [lane.get("target_dir") for lane in cargo]
    if any(not value for value in worktrees) or len(set(worktrees)) != len(worktrees):
        errors.append(f"{label}: Cargo lane worktrees must be distinct and nonempty")
    if any(not value for value in targets) or len(set(targets)) != len(targets):
        errors.append(f"{label}: Cargo target directories must be distinct and nonempty")
    owners = [lane.get("owner") for lane in cargo if lane.get("owner")]
    if len(set(owners)) != len(owners):
        errors.append(f"{label}: a task cannot own multiple Cargo lanes")
    for owner in owners:
        if owner not in tasks or tasks[owner].get("status") not in ACTIVE:
            errors.append(f"{label}: Cargo lane owner {owner} is not active")
    stateful_owner = lanes.get("stateful_owner") if isinstance(lanes, dict) else None
    if stateful_owner and (stateful_owner not in tasks or tasks[stateful_owner].get("status") not in ACTIVE):
        errors.append(f"{label}: stateful lane owner {stateful_owner} is not active")
    review_owners = lanes.get("review_owners", []) if isinstance(lanes, dict) else []
    if len(review_owners) > 2 or len(set(review_owners)) != len(review_owners):
        errors.append(f"{label}: review ownership must contain at most two distinct tasks")
    for owner in review_owners:
        if owner not in tasks or tasks[owner].get("status") not in {"candidate", "accepted"}:
            errors.append(f"{label}: review owner {owner} is not a reviewable candidate")
    return errors


def validate(state: dict[str, Any]) -> list[str]:
    """Return deterministic diagnostics for a controller state document."""
    errors: list[str] = []
    if state.get("version") != 1:
        errors.append("version must be 1")
    if not isinstance(state.get("controller_id"), str) or not state["controller_id"]:
        errors.append("controller_id must be nonempty")
    mutable = state.get("mutable")
    if not isinstance(mutable, dict):
        return errors + ["mutable must be an object"]

    events = state.get("events", [])
    previous_hash = "0" * 64
    prior_projection: dict[str, Any] | None = None
    initial_head: str | None = None
    failed_candidates: set[tuple[str, Any, Any]] = set()
    for sequence, event in enumerate(events, 1):
        if event.get("sequence") != sequence:
            errors.append(f"event sequence must be contiguous at {sequence}")
        if event.get("previous_hash") != previous_hash:
            errors.append(f"event {sequence}: previous_hash breaks chain")
        unhashed = {key: value for key, value in event.items() if key != "hash"}
        expected_hash = digest(unhashed)
        if event.get("hash") != expected_hash:
            errors.append(f"event {sequence}: hash mismatch")
        projection = event.get("mutable")
        if not isinstance(projection, dict):
            errors.append(f"event {sequence}: mutable projection missing")
        if prior_projection is None and isinstance(projection, dict):
            initial_head = projection.get("initial_head")
            for task_id, task in projection.get("tasks", {}).items():
                if task.get("status") != "ready" or task.get("candidate_sha") is not None:
                    errors.append(f"event 1: {task_id} must initialize ready without candidate")
        elif isinstance(projection, dict):
            if projection.get("initial_head") != initial_head:
                errors.append(f"event {sequence}: initial_head changed after initialization")
            before_tasks = prior_projection.get("tasks", {})
            after_tasks = projection.get("tasks", {})
            for task_id in set(before_tasks) | set(after_tasks):
                before_task = before_tasks.get(task_id, {})
                after_task = after_tasks.get(task_id, {})
                before, after = before_task.get("status"), after_task.get("status")
                requirements_revised = (
                    before == "frozen" and after == "ready"
                    and after_task.get("revision", 0) > before_task.get("revision", 0)
                )
                if before_task.get("proof_requirements") != after_task.get("proof_requirements") and not requirements_revised:
                    errors.append(f"event {sequence}: {task_id} proof requirements changed")
                if before != after and (before, after) not in TRANSITIONS:
                    errors.append(f"event {sequence}: invalid transition {before}->{after}")
                if before_task.get("candidate_sha") is not None and before_task.get("candidate_sha") != after_task.get("candidate_sha"):
                    if after_task.get("generation", 0) <= before_task.get("generation", 0):
                        errors.append(f"event {sequence}: candidate changed without successor generation")
                    attempts = after_task.get("proof_attempts", {})
                    retained_attempts = isinstance(attempts, dict) and any(attempts.values())
                    if retained_attempts or after_task.get("accepted_proofs") or after_task.get("review") is not None:
                        errors.append(f"event {sequence}: successor candidate retained proof or review evidence")
                if before == "candidate" and after == "active" and after_task.get("candidate_sha") is not None:
                    errors.append(f"event {sequence}: stale candidate was not cleared before refresh")
                if before == "active" and after == "candidate" and before_task.get("candidate_sha") is None:
                    if after_task.get("base_sha") != projection.get("integration_head"):
                        errors.append(f"event {sequence}: successor candidate does not use current integration head")
                    attempts = after_task.get("proof_attempts", {})
                    if (isinstance(attempts, dict) and any(attempts.values())) or after_task.get("accepted_proofs") or after_task.get("review") is not None:
                        errors.append(f"event {sequence}: fresh successor candidate retained proof or review evidence")
                before_attempts = before_task.get("proof_attempts", {})
                after_attempts = after_task.get("proof_attempts", {})
                requirements = after_task.get("proof_requirements", {})
                if isinstance(before_attempts, dict) and isinstance(after_attempts, dict) and isinstance(requirements, dict):
                    for proof_id in set(before_attempts) | set(after_attempts):
                        proof_attempts = after_attempts.get(proof_id, [])
                        old_attempts = before_attempts.get(proof_id, [])
                        if not isinstance(proof_attempts, list) or not isinstance(old_attempts, list):
                            continue
                        same_candidate = (
                            before_task.get("candidate_sha") == after_task.get("candidate_sha")
                            and before_task.get("generation") == after_task.get("generation")
                        )
                        if same_candidate and (
                            len(proof_attempts) < len(old_attempts)
                            or proof_attempts[:len(old_attempts)] != old_attempts
                        ):
                            errors.append(f"event {sequence}: {task_id}/{proof_id} proof attempts are not append-only")
                        for attempt in proof_attempts[len(old_attempts):]:
                            candidate_key = (task_id, after_task.get("candidate_sha"), after_task.get("generation"))
                            if candidate_key in failed_candidates:
                                errors.append(f"event {sequence}: {task_id} source/test failure was rerun without successor candidate")
                            if attempt.get("classification") in {"source", "test"} and attempt.get("exit_code") != 0:
                                failed_candidates.add(candidate_key)
                            prerequisites = requirements.get(proof_id, {}).get("prerequisites", [])
                            if any(after_tasks.get(item, {}).get("status") != "integrated" for item in prerequisites):
                                errors.append(f"event {sequence}: {task_id}/{proof_id} proof prerequisite is not integrated")
                            if prerequisites and attempt.get("integration_head") != projection.get("integration_head"):
                                errors.append(f"event {sequence}: {task_id}/{proof_id} proof did not use current integration head")
                if before == "frozen" and after == "ready":
                    if (after_task.get("revision", 0), after_task.get("generation", 0)) <= (before_task.get("revision", 0), before_task.get("generation", 0)):
                        errors.append(f"event {sequence}: frozen task resumed without canonical revision")
        prior_projection = projection
        if isinstance(projection, dict):
            errors.extend(validate_projection(projection, f"event {sequence}"))
        previous_hash = expected_hash
    if not events or prior_projection != mutable:
        errors.append("top-level mutable state does not match replayed final event")

    tasks = mutable.get("tasks", {})
    if not isinstance(tasks, dict):
        return errors + ["mutable.tasks must be an object"]
    errors.extend(validate_projection(mutable, "final"))
    if not is_git_sha(mutable.get("initial_head")):
        errors.append("invalid initial accepted integration head")
    integration_order = mutable.get("integration_order", [])
    if len(integration_order) != len(set(integration_order)):
        errors.append("integration_order contains duplicates")

    def visit(task_id: str, trail: set[str], combined: bool = False) -> None:
        if task_id in trail:
            label = "combined dependency/proof cycle" if combined else "dependency cycle"
            errors.append(f"{label} includes {task_id}")
            return
        task = tasks.get(task_id, {})
        edges = list(task.get("dependencies", []))
        if combined:
            requirements = task.get("proof_requirements", {})
            if isinstance(requirements, dict):
                edges.extend(
                    prerequisite
                    for requirement in requirements.values()
                    if isinstance(requirement, dict)
                    for prerequisite in requirement.get("prerequisites", [])
                )
        for dependency in edges:
            if dependency in tasks:
                visit(dependency, trail | {task_id}, combined)

    for task_id in tasks:
        visit(task_id, set())
        visit(task_id, set(), True)

    active: list[tuple[str, dict[str, Any]]] = []
    for task_id, task in tasks.items():
        status = task.get("status")
        if status not in STATUSES:
            errors.append(f"{task_id}: invalid status {status!r}")
            continue
        if status in ACTIVE:
            active.append((task_id, task))
        if not isinstance(task.get("generation"), int) or task["generation"] < 1:
            errors.append(f"{task_id}: generation must be positive")
        if not isinstance(task.get("revision"), int) or task["revision"] < 1:
            errors.append(f"{task_id}: revision must be positive")
        if not is_digest(task.get("digest")):
            errors.append(f"{task_id}: invalid task digest")
        dependencies = task.get("dependencies")
        if (
            not isinstance(dependencies, list)
            or len(dependencies) != len(set(dependencies))
        ):
            errors.append(f"{task_id}: dependencies must be a duplicate-free list")
            dependencies = []
        locks = task.get("locks")
        if not isinstance(locks, list) or len(locks) != len(set(locks)):
            errors.append(f"{task_id}: locks must be a duplicate-free list")
        write_set = task.get("write_set")
        if (
            not isinstance(write_set, list)
            or not write_set
            or len(write_set) != len(set(write_set))
            or not all(
                isinstance(path, str)
                and path
                and not PurePosixPath(path).is_absolute()
                for path in write_set
            )
        ):
            errors.append(
                f"{task_id}: write_set must be a nonempty duplicate-free list of relative paths"
            )
        for dependency in task.get("dependencies", []):
            if dependency not in tasks:
                errors.append(f"{task_id}: unknown dependency {dependency}")
            elif status != "ready" and tasks[dependency].get("status") != "integrated":
                errors.append(f"{task_id}: dependency {dependency} is not integrated")
        candidate = task.get("candidate_sha")
        generation = task.get("generation")
        if status in {"candidate", "accepted", "integrated"}:
            for field in ("base_sha", "candidate_sha", "candidate_parent"):
                if not is_git_sha(task.get(field)):
                    errors.append(f"{task_id}: invalid {field}")
            if not is_digest(task.get("diff_digest")) or not is_git_sha(task.get("patch_id")):
                errors.append(f"{task_id}: invalid diff identity")

        requirements = task.get("proof_requirements")
        attempts_by_proof = task.get("proof_attempts")
        accepted_by_proof = task.get("accepted_proofs")
        if not isinstance(requirements, dict) or not requirements:
            errors.append(f"{task_id}: proof_requirements must be a nonempty object")
            requirements = {}
        if not isinstance(attempts_by_proof, dict):
            errors.append(f"{task_id}: proof_attempts must be keyed by proof ID")
            attempts_by_proof = {}
        if not isinstance(accepted_by_proof, dict):
            errors.append(f"{task_id}: accepted_proofs must be keyed by proof ID")
            accepted_by_proof = {}
        unknown_streams = (set(attempts_by_proof) | set(accepted_by_proof)) - set(requirements)
        for proof_id in sorted(unknown_streams):
            errors.append(f"{task_id}: unknown proof ID {proof_id}")
        for proof_id, requirement in requirements.items():
            if not isinstance(proof_id, str) or not proof_id:
                errors.append(f"{task_id}: proof ID must be nonempty")
            prerequisites = requirement.get("prerequisites") if isinstance(requirement, dict) else None
            if not isinstance(prerequisites, list) or len(prerequisites) != len(set(prerequisites)):
                errors.append(f"{task_id}/{proof_id}: prerequisites must be a duplicate-free list")
                prerequisites = []
            for prerequisite in prerequisites:
                if prerequisite not in tasks:
                    errors.append(f"{task_id}/{proof_id}: unknown proof prerequisite {prerequisite}")
                elif prerequisite == task_id:
                    errors.append(f"{task_id}/{proof_id}: proof prerequisite cannot name its own task")
            attempts = attempts_by_proof.get(proof_id, [])
            if not isinstance(attempts, list):
                errors.append(f"{task_id}/{proof_id}: proof attempts must be a list")
                attempts = []
            passing = []
            if attempts and status not in {"candidate", "accepted", "integrated"}:
                errors.append(f"{task_id}/{proof_id}: proof attempts exist without a candidate")
            for ordinal, attempt in enumerate(attempts, 1):
                if attempt.get("proof_id") != proof_id:
                    errors.append(f"{task_id}/{proof_id}: attempt proof ID mismatch")
                if attempt.get("ordinal") != ordinal:
                    errors.append(f"{task_id}/{proof_id}: proof attempt ordinals are not contiguous")
                if attempt.get("candidate_sha") != candidate or attempt.get("generation") != generation:
                    errors.append(f"{task_id}/{proof_id}: proof attempt has stale candidate generation")
                if not is_git_sha(attempt.get("integration_head")):
                    errors.append(f"{task_id}/{proof_id}: proof attempt lacks integration-head identity")
                if any(tasks.get(item, {}).get("status") != "integrated" for item in prerequisites):
                    errors.append(f"{task_id}/{proof_id}: proof prerequisite is not integrated")
                if prerequisites and attempt.get("integration_head") != task.get("base_sha"):
                    errors.append(f"{task_id}/{proof_id}: candidate predates proof prerequisite integration")
                if not isinstance(attempt.get("command"), list) or not attempt["command"] or not all(isinstance(arg, str) for arg in attempt["command"]):
                    errors.append(f"{task_id}/{proof_id}: proof attempt lacks full command argv")
                if not is_digest(attempt.get("artifact_digest")):
                    errors.append(f"{task_id}/{proof_id}: invalid proof artifact digest")
                classification = attempt.get("classification")
                if classification not in {"pass", "infrastructure", "setup", "source", "test"}:
                    errors.append(f"{task_id}/{proof_id}: invalid proof classification")
                if "cargo_lane" not in attempt or not isinstance(attempt.get("stateful"), bool):
                    errors.append(f"{task_id}/{proof_id}: proof attempt lacks lane identity")
                if classification == "pass" and attempt.get("exit_code") != 0:
                    errors.append(f"{task_id}/{proof_id}: PASS proof has nonzero exit")
                if classification != "pass" and attempt.get("exit_code") == 0:
                    errors.append(f"{task_id}/{proof_id}: failed proof classification has zero exit")
                if attempt.get("exit_code") == 0 and classification == "pass":
                    passing.append(ordinal)
            if len(passing) > 1:
                errors.append(f"{task_id}/{proof_id}: proof stream has multiple PASS attempts")
            accepted = accepted_by_proof.get(proof_id)
            if accepted is not None:
                ordinal = accepted.get("ordinal")
                if accepted.get("proof_id") != proof_id:
                    errors.append(f"{task_id}/{proof_id}: accepted proof ID mismatch")
                if len(passing) != 1 or passing != [ordinal]:
                    errors.append(f"{task_id}/{proof_id}: accepted proof is not the sole PASS")
                elif attempts[ordinal - 1].get("selected_count", 0) <= 0:
                    errors.append(f"{task_id}/{proof_id}: accepted proof selected no tests")
                if ordinal not in range(1, len(attempts) + 1) or accepted.get("artifact_digest") != attempts[ordinal - 1].get("artifact_digest"):
                    errors.append(f"{task_id}/{proof_id}: accepted proof digest mismatch")
            if status in {"accepted", "integrated"} and accepted is None:
                errors.append(f"{task_id}/{proof_id}: missing accepted proof")

        review = task.get("review")
        if review is not None:
            if review.get("verdict") not in VERDICTS or not is_digest(review.get("result_digest")):
                errors.append(f"{task_id}: invalid review result")
            if review.get("candidate_sha") != candidate or review.get("generation") != generation:
                errors.append(f"{task_id}: review has stale candidate generation")
            review_event = next((event for event in events if event.get("hash") == review.get("controller_event_hash")), None)
            if review_event is None:
                errors.append(f"{task_id}: reviewer controller-state identity is invalid")
            else:
                reviewed_task = review_event.get("mutable", {}).get("tasks", {}).get(task_id, {})
                if reviewed_task.get("candidate_sha") != candidate or reviewed_task.get("generation") != generation:
                    errors.append(f"{task_id}: reviewer controller-state identity names another candidate")
        if status in {"accepted", "integrated"} and (not review or review.get("verdict") != "APPROVE"):
            errors.append(f"{task_id}: missing exact APPROVE review")

        if status == "integrated":
            if task_id not in integration_order or not is_git_sha(task.get("integrated_commit")) or not is_git_sha(task.get("integrated_parent")):
                errors.append(f"{task_id}: integrated task missing integration identity")
            if not is_git_sha(task.get("evidence_commit")):
                errors.append(f"{task_id}: integrated task missing external-plan evidence commit")

    if len(active) > 3:
        errors.append("more than three implementor allocations are active")
    for index, (left_id, left) in enumerate(active):
        for right_id, right in active[index + 1:]:
            if any(overlaps(a, b) for a in left.get("write_set", []) for b in right.get("write_set", [])):
                errors.append(f"write-set overlap: {left_id} and {right_id}")
            if set(left.get("locks", [])) & set(right.get("locks", [])):
                errors.append(f"semantic lock conflict: {left_id} and {right_id}")

    positions = {task_id: index for index, task_id in enumerate(integration_order)}
    expected_parent = mutable.get("initial_head")
    for task_id in integration_order:
        if task_id not in tasks or tasks[task_id].get("status") != "integrated":
            errors.append(f"integration_order contains non-integrated task {task_id}")
            continue
        for dependency in tasks[task_id].get("dependencies", []):
            if positions.get(dependency, sys.maxsize) >= positions[task_id]:
                errors.append(f"integration order violates {dependency}->{task_id}")
        if tasks[task_id].get("integrated_parent") != expected_parent:
            errors.append(f"{task_id}: declared integration parent is not the direct serial predecessor")
        expected_parent = tasks[task_id].get("integrated_commit")

    lanes = mutable.get("lanes", {})
    cargo = lanes.get("cargo", []) if isinstance(lanes, dict) else []
    if len(cargo) > 2:
        errors.append("at most two Cargo lanes are permitted")
    worktrees = [lane.get("worktree") for lane in cargo]
    targets = [lane.get("target_dir") for lane in cargo]
    if any(not value for value in worktrees) or len(set(worktrees)) != len(worktrees):
        errors.append("Cargo lane worktrees must be distinct and nonempty")
    if any(not value for value in targets) or len(set(targets)) != len(targets):
        errors.append("Cargo target directories must be distinct and nonempty")
    owners = [lane.get("owner") for lane in cargo if lane.get("owner")]
    if len(set(owners)) != len(owners):
        errors.append("a task cannot own multiple Cargo lanes")
    for owner in owners:
        if owner not in tasks or tasks[owner].get("status") not in ACTIVE:
            errors.append(f"Cargo lane owner {owner} is not active")
    lane_names = {lane.get("name") for lane in cargo}
    for task_id, task in tasks.items():
        attempts_by_proof = task.get("proof_attempts", {})
        if isinstance(attempts_by_proof, dict):
            for attempts in attempts_by_proof.values():
                if not isinstance(attempts, list):
                    continue
                for attempt in attempts:
                    cargo_lane = attempt.get("cargo_lane")
                    if cargo_lane is not None and cargo_lane not in lane_names:
                        errors.append(f"{task_id}: proof references unknown Cargo lane {cargo_lane}")
    stateful_owner = lanes.get("stateful_owner") if isinstance(lanes, dict) else None
    if stateful_owner and (stateful_owner not in tasks or tasks[stateful_owner].get("status") not in ACTIVE):
        errors.append(f"stateful lane owner {stateful_owner} is not active")
    review_owners = lanes.get("review_owners", []) if isinstance(lanes, dict) else []
    if len(review_owners) > 2 or len(set(review_owners)) != len(review_owners):
        errors.append("review ownership must contain at most two distinct tasks")

    integrated = [tasks[task_id] for task_id in integration_order if task_id in tasks]
    if integrated and mutable.get("integration_head") != integrated[-1].get("integrated_commit"):
        errors.append("integration_head does not match final integrated commit")
    evidence_commit = mutable.get("evidence_commit")
    if evidence_commit is not None and not is_git_sha(evidence_commit):
        errors.append("invalid external-plan evidence commit")
    if integrated and evidence_commit != integrated[-1].get("evidence_commit"):
        errors.append("global evidence commit does not match final integrated task evidence")

    repo = state.get("repo_root")
    if repo:
        try:
            for task_id, task in tasks.items():
                if task.get("status") not in {"candidate", "accepted", "integrated"}:
                    continue
                candidate, parent, base = task["candidate_sha"], task["candidate_parent"], task["base_sha"]
                actual_parent = git(repo, "rev-parse", f"{candidate}^")
                if actual_parent != parent or parent != base:
                    errors.append(f"{task_id}: candidate ancestry mismatch")
                diff_digest, patch_id = commit_diff(repo, parent, candidate)
                if diff_digest != task["diff_digest"] or patch_id != task["patch_id"]:
                    errors.append(f"{task_id}: candidate diff identity mismatch")
                requirements = task.get("proof_requirements", {})
                attempts_by_proof = task.get("proof_attempts", {})
                for proof_id, requirement in requirements.items():
                    if not attempts_by_proof.get(proof_id):
                        continue
                    for prerequisite in requirement.get("prerequisites", []):
                        prerequisite_commit = tasks.get(prerequisite, {}).get("integrated_commit")
                        if prerequisite_commit:
                            git(repo, "merge-base", "--is-ancestor", prerequisite_commit, candidate)
            prior_commit = mutable["initial_head"]
            for task_id in integration_order:
                task = tasks[task_id]
                patch_id = task["patch_id"]
                integrated_commit = task["integrated_commit"]
                integrated_parent = git(repo, "rev-parse", f"{integrated_commit}^")
                if integrated_parent != prior_commit:
                    errors.append(f"{task_id}: integrated commit is not the direct serial successor")
                _, integrated_patch = commit_diff(repo, integrated_parent, integrated_commit)
                if integrated_patch != patch_id:
                    errors.append(f"{task_id}: cherry-pick patch identity mismatch")
                git(repo, "merge-base", "--is-ancestor", prior_commit, integrated_commit)
                prior_commit = integrated_commit
        except (subprocess.CalledProcessError, KeyError, IndexError) as error:
            errors.append(f"Git identity validation failed: {error}")
    return sorted(set(errors))


def main() -> int:
    """Validate one state file and return a shell-compatible status."""
    if len(sys.argv) != 2:
        print("usage: validate_controller_state.py STATE.json", file=sys.stderr)
        return 2
    with open(sys.argv[1], encoding="utf-8") as handle:
        state = json.load(handle)
    errors = validate(state)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print("controller state valid")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
