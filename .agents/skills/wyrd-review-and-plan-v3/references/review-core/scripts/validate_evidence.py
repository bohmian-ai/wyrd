#!/usr/bin/env python3
"""Validate full-review evidence and cross-artifact traceability."""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


REPORT_SECTIONS = (
    "## Scope",
    "## Adversarial probe",
    "## Candidate findings",
    "## Clean rationale",
    "## Static-analysis limits",
)
CANDIDATE_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*/C\d{3}$")
CANDIDATE_HEADING = re.compile(
    r"^### ([a-z0-9]+(?:-[a-z0-9]+)*/C\d{3}): .+$",
    re.MULTILINE,
)
CANDIDATE_FIELDS = (
    "Severity:",
    "Confidence:",
    "Category:",
    "Requirements:",
    "Rules:",
)
CANDIDATE_SECTIONS = (
    "#### Maintainer summary",
    "#### Problem",
    "#### Failure scenario",
    "#### Recommended correction",
    "#### Verification",
)
PROHIBITED_CANDIDATE_PHRASES = (
    "the cited owner",
    "the invariant expressed by the title",
    "normal production workflow",
    "validated source location",
)
BASELINE_CAPABILITIES = {
    "correctness": "correctness-specialist",
    "security": "security-specialist",
    "code-quality": "quality-specialist",
    "maintainability": "maintainability-specialist",
    "tests": "test-specialist",
    "developer-experience": "dx-specialist",
    "architecture-contracts": "architecture-specialist",
}
PROMPT_BY_DOMAIN = {
    "correctness": "review-bugs/review-bugs.md",
    "security": "review-security/review-security.md",
    "code-quality": "review-code-quality/review-code-quality.md",
    "maintainability": "review-maintainability/review-maintainability.md",
    "tests": "review-tests/review-tests.md",
    "developer-experience": "review-developer-experience/review-developer-experience.md",
    "architecture-contracts": "review-architecture-contracts/review-architecture-contracts.md",
    "persistence-storage": "review-persistence-storage/review-persistence-storage.md",
    "async-reliability": "review-async-reliability/review-async-reliability.md",
    "pyo3-cross-language": "review-pyo3-cross-language/review-pyo3-cross-language.md",
    "vala-data-plane": "review-vala-data-plane/review-vala-data-plane.md",
    "wyrd-ui": "review-wyrd-ui/review-wyrd-ui.md",
}
VALIDATION_FIELDS = (
    "Source reviewers:",
    "Original severity:",
    "Original locations:",
    "Original claim:",
    "Validation status:",
    "Final mapping:",
    "Source re-read:",
    "Callers/consumers inspected:",
    "Authority and intent checked:",
    "Reachability analysis:",
    "Materiality analysis:",
    "Decision rationale:",
    "Dissent:",
)
STATUSES = {
    "CONFIRMED",
    "MERGED",
    "FOLLOW_UP",
    "KNOWN_DEFERRED",
    "STATIC_LIMIT",
    "REJECTED",
    "MATERIAL_DECISION_REQUIRED",
}
HIGH_RISK_CATEGORIES = {
    "auth",
    "tenancy",
    "destructive-persistence",
    "migration",
    "concurrency",
    "recovery",
    "public-contract",
    "other",
}


def load_json(path: Path, errors: list[str]) -> Any:
    """Load one JSON artifact and report malformed or missing content."""
    if not path.is_file():
        errors.append(f"missing evidence artifact: {path}")
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        errors.append(f"invalid JSON artifact {path}: {error}")
        return None


def section_body(text: str, section: str) -> str:
    """Return one second-level Markdown section body."""
    if section not in text:
        return ""
    after = text.split(section, 1)[1]
    next_section = re.search(r"^## ", after, re.MULTILINE)
    return (after[: next_section.start()] if next_section else after).strip()


def candidate_blocks(text: str) -> list[tuple[str, str]]:
    """Return candidate IDs and their report bodies."""
    findings = section_body(text, "## Candidate findings")
    matches = list(CANDIDATE_HEADING.finditer(findings))
    return [
        (
            match.group(1),
            findings[
                match.end() : matches[index + 1].start()
                if index + 1 < len(matches)
                else len(findings)
            ],
        )
        for index, match in enumerate(matches)
    ]


def validate_specialists(
    directory: Path,
    roster: dict[str, dict[str, Any]],
    errors: list[str],
) -> dict[str, tuple[str, str]]:
    """Validate reports and map candidate IDs to assignment and reviewer IDs."""
    candidate_sources: dict[str, tuple[str, str]] = {}
    reports = {path.stem: path for path in directory.glob("*.md")}
    attestation_suffix = ".attestation.json"
    attestations = {
        path.name[: -len(attestation_suffix)]: path
        for path in directory.glob(f"*{attestation_suffix}")
    }
    expected = {
        assignment_id
        for assignment_id, assignment in roster.items()
        if assignment.get("status") == "completed"
    }
    missing = sorted(expected - reports.keys())
    unexpected = sorted(reports.keys() - expected)
    missing_attestations = sorted(expected - attestations.keys())
    unexpected_attestations = sorted(attestations.keys() - expected)
    for assignment_id in missing:
        errors.append(f"completed assignment has no specialist report: {assignment_id}")
    for assignment_id in unexpected:
        errors.append(f"specialist report has no completed assignment: {assignment_id}")
    for assignment_id in missing_attestations:
        errors.append(
            f"completed assignment has no dispatch attestation: {assignment_id}"
        )
    for assignment_id in unexpected_attestations:
        errors.append(
            f"dispatch attestation has no completed assignment: {assignment_id}"
        )

    for assignment_id, path in reports.items():
        text = path.read_text(encoding="utf-8")
        assignment = roster.get(assignment_id, {})
        expected_report = assignment.get("report_path")
        if expected_report and not str(expected_report).endswith(f"/{path.name}"):
            errors.append(f"{assignment_id} report path does not match roster")
        attestation_path = attestations.get(assignment_id)
        if attestation_path is not None:
            attestation = load_json(attestation_path, errors)
            expected_attestation = assignment.get("attestation_path")
            if expected_attestation and not str(expected_attestation).endswith(
                f"/{attestation_path.name}"
            ):
                errors.append(f"{assignment_id} attestation path does not match roster")
            if isinstance(attestation, dict):
                expected_values = {
                    "schema_version": 1,
                    "assignment_id": assignment_id,
                    "domain": assignment.get("domain"),
                    "reviewer_id": assignment.get("reviewer_id"),
                    "target_sha": assignment.get("target_sha"),
                    "prompt_sha256": str(
                        assignment.get("prompt_digest", "")
                    ).removeprefix("sha256:"),
                    "report_path": assignment.get("report_path"),
                    "report_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    "status": "completed",
                }
                if set(attestation) != set(expected_values):
                    errors.append(
                        f"{assignment_id} attestation has unexpected or missing fields"
                    )
                for field, expected_value in expected_values.items():
                    if attestation.get(field) != expected_value:
                        errors.append(
                            f"{assignment_id} attestation has invalid {field}"
                        )
        domain = assignment.get("domain")
        positions: list[int] = []
        for section in REPORT_SECTIONS:
            if text.count(section) != 1:
                errors.append(f"{path} must contain exactly one `{section}`")
                continue
            positions.append(text.index(section))
            if not section_body(text, section):
                errors.append(f"{path} has empty `{section}`")
        if len(positions) == len(REPORT_SECTIONS) and positions != sorted(positions):
            errors.append(f"{path} specialist sections are out of order")

        for candidate_id, body in candidate_blocks(text):
            if not candidate_id.startswith(f"{domain}/"):
                errors.append(
                    f"candidate `{candidate_id}` does not match assignment domain `{domain}`"
                )
            if candidate_id in candidate_sources:
                errors.append(f"duplicate candidate ID: {candidate_id}")
            candidate_sources[candidate_id] = (
                assignment_id,
                str(assignment.get("reviewer_id", "")),
            )
            for field in CANDIDATE_FIELDS:
                if not re.search(rf"^{re.escape(field)}\s*\S", body, re.MULTILINE):
                    errors.append(f"{candidate_id} is missing `{field}`")
            locations = body.split("Locations:", 1)
            if len(locations) != 2 or not re.search(
                r"^- `[^`]+`(?: — .+)?$", locations[1], re.MULTILINE
            ):
                errors.append(f"{candidate_id} lacks a concrete location")
            for section in CANDIDATE_SECTIONS:
                if body.count(section) != 1:
                    errors.append(
                        f"{candidate_id} must contain exactly one `{section}`"
                    )
            lowered_body = body.lower()
            for phrase in PROHIBITED_CANDIDATE_PHRASES:
                if phrase in lowered_body:
                    errors.append(
                        f"{candidate_id} uses non-actionable placeholder phrase "
                        f"`{phrase}`"
                    )
    return candidate_sources


def validate_coverage(
    data: Any, errors: list[str], verdict: str | None = None
) -> dict[str, dict[str, Any]]:
    """Validate coverage assignments and return the assignment roster."""
    if not isinstance(data, dict):
        errors.append("coverage.json must contain an object")
        return {}
    if data.get("schema_version") != 3:
        errors.append("coverage.json must use schema_version 3")
    mode = data.get("mode")
    if mode != "full":
        errors.append("coverage.json has invalid mode")
    adversarial = data.get("adversarial_review")
    if not isinstance(adversarial, dict):
        errors.append("coverage.json `adversarial_review` must be an object")
    else:
        if adversarial.get("mode") != mode:
            errors.append("adversarial review mode does not match coverage mode")
        if adversarial.get("status") not in {"completed", "gap"}:
            errors.append("adversarial review has invalid status")
        if (
            not isinstance(adversarial.get("summary"), str)
            or not adversarial.get("summary", "").strip()
        ):
            errors.append("adversarial review has no summary")
        if adversarial.get("status") == "gap" and verdict not in {
            "REVIEW_BLOCKED",
            "MATERIAL_DECISION_REQUIRED",
        }:
            errors.append("adversarial review gap requires a blocking verdict")

    roster_entries = data.get("roster")
    if not isinstance(roster_entries, list):
        errors.append("coverage.json `roster` must be a list")
        roster_entries = []
    roster: dict[str, dict[str, Any]] = {}
    baseline_domains: set[str] = set()
    completed_reviewers: set[str] = set()
    for assignment in roster_entries:
        if not isinstance(assignment, dict):
            errors.append("coverage roster entries must be objects")
            continue
        assignment_id = assignment.get("assignment_id")
        if not isinstance(assignment_id, str) or not assignment_id:
            errors.append("coverage roster entry has no assignment ID")
            continue
        if assignment_id in roster:
            errors.append(f"duplicate roster assignment: {assignment_id}")
        roster[assignment_id] = assignment
        domain = assignment.get("domain")
        requirement = assignment.get("requirement")
        status = assignment.get("status")
        if requirement not in {"baseline", "triggered"}:
            errors.append(f"{assignment_id} has invalid requirement")
        if status not in {"completed", "blocked"}:
            errors.append(f"{assignment_id} has invalid status")
        for field in (
            "required_capability",
            "reviewer_id",
            "prompt_path",
            "prompt_digest",
            "target_sha",
            "report_path",
            "attestation_path",
        ):
            if (
                not isinstance(assignment.get(field), str)
                or not assignment.get(field, "").strip()
            ):
                errors.append(f"{assignment_id} has no {field}")
        if not str(assignment.get("prompt_digest", "")).startswith("sha256:"):
            errors.append(f"{assignment_id} has invalid prompt digest")
        expected_prompt = PROMPT_BY_DOMAIN.get(str(domain))
        if expected_prompt is not None:
            if assignment.get("prompt_path") != expected_prompt:
                errors.append(f"{assignment_id} has incorrect prompt path")
            prompt_file = Path(__file__).parents[1] / expected_prompt
            expected_digest = (
                "sha256:" + hashlib.sha256(prompt_file.read_bytes()).hexdigest()
            )
            if assignment.get("prompt_digest") != expected_digest:
                errors.append(f"{assignment_id} has incorrect prompt digest")
        if requirement == "baseline":
            if domain in baseline_domains:
                errors.append(f"baseline domain has multiple assignments: {domain}")
            if isinstance(domain, str):
                baseline_domains.add(domain)
            expected_capability = BASELINE_CAPABILITIES.get(domain)
            if expected_capability is None:
                errors.append(f"unknown baseline domain: {domain}")
            elif assignment.get("required_capability") != expected_capability:
                errors.append(f"baseline capability mismatch: {domain}")
            if status != "completed" and verdict != "REVIEW_BLOCKED":
                errors.append(f"baseline assignment is not completed: {domain}")
        reviewer_id = assignment.get("reviewer_id")
        if mode == "full" and status == "completed":
            if reviewer_id in completed_reviewers:
                errors.append(f"completed assignments share reviewer: {reviewer_id}")
            if isinstance(reviewer_id, str):
                completed_reviewers.add(reviewer_id)
    for domain in sorted(set(BASELINE_CAPABILITIES) - baseline_domains):
        errors.append(f"missing baseline domain: {domain}")

    triggers = data.get("triggers")
    if not isinstance(triggers, list):
        errors.append("coverage.json `triggers` must be a list")
        triggers = []
    for trigger in triggers:
        if not isinstance(trigger, dict):
            errors.append("coverage trigger entries must be objects")
            continue
        trigger_id = trigger.get("trigger_id", "<missing>")
        if trigger.get("status") not in {"assigned", "gap"}:
            errors.append(f"trigger has invalid status: {trigger_id}")
        if not trigger.get("matched_locations"):
            errors.append(f"trigger has no matched locations: {trigger_id}")
        matches = [
            assignment
            for assignment in roster.values()
            if assignment.get("requirement") == "triggered"
            and assignment.get("status") == "completed"
            and assignment.get("domain") == trigger.get("domain")
            and trigger_id in assignment.get("trigger_ids", [])
        ]
        if trigger.get("status") == "assigned" and len(matches) != 1:
            errors.append(
                f"trigger needs exactly one completed matching assignment: {trigger_id}"
            )
        if trigger.get("status") == "gap" and verdict not in {
            "REVIEW_BLOCKED",
            "MATERIAL_DECISION_REQUIRED",
        }:
            errors.append(f"trigger gap requires a blocking verdict: {trigger_id}")
    independent = data.get("independent_validation")
    if not isinstance(independent, dict):
        errors.append("coverage.json `independent_validation` must be an object")
    else:
        status = independent.get("status")
        if status not in {"completed", "not_required"}:
            errors.append("independent validation has invalid status")
        if (
            not isinstance(independent.get("reason"), str)
            or not independent.get("reason", "").strip()
        ):
            errors.append("independent validation has no reason")
        if status == "completed" and (
            not isinstance(independent.get("reviewer"), str)
            or not independent.get("reviewer", "").strip()
        ):
            errors.append("completed independent validation has no reviewer")
    changed_files = data.get("changed_files")
    if not isinstance(changed_files, list):
        errors.append("coverage.json `changed_files` must be a list")
        changed_files = []
    seen_paths: set[str] = set()
    for entry in changed_files:
        if not isinstance(entry, dict):
            errors.append("coverage changed-file entries must be objects")
            continue
        path = entry.get("path")
        if not isinstance(path, str) or not path:
            errors.append("coverage changed-file entry has no path")
            continue
        if path in seen_paths:
            errors.append(f"duplicate coverage path: {path}")
        seen_paths.add(path)
        classification = entry.get("classification")
        primary = entry.get("primary_assignment")
        status = entry.get("status")
        if classification == "production" and not primary:
            errors.append(f"production file has no primary assignment: {path}")
        if status not in {"inspected", "sampled", "not_applicable"}:
            errors.append(f"coverage path has invalid status: {path}")
        if isinstance(primary, str) and primary:
            if primary not in roster:
                errors.append(f"coverage path references unknown assignment: {path}")
        secondary = entry.get("secondary_assignments", [])
        if not isinstance(secondary, list) or not all(
            isinstance(lens, str) and lens for lens in secondary
        ):
            errors.append(f"coverage path has invalid secondary assignments: {path}")
        elif any(value not in roster for value in secondary):
            errors.append(
                f"coverage path references unknown secondary assignment: {path}"
            )

    requirements = data.get("requirements")
    if not isinstance(requirements, list):
        errors.append("coverage.json `requirements` must be a list")
    else:
        seen_requirements: set[str] = set()
        for requirement in requirements:
            if not isinstance(requirement, dict):
                errors.append("coverage requirement entries must be objects")
                continue
            requirement_id = requirement.get("id")
            if not isinstance(requirement_id, str) or not requirement_id:
                errors.append("coverage requirement has no ID")
                continue
            if requirement_id in seen_requirements:
                errors.append(f"duplicate coverage requirement: {requirement_id}")
            seen_requirements.add(requirement_id)
            status = requirement.get("status")
            if status not in {"covered", "gap", "deferred", "not_applicable"}:
                errors.append(f"requirement has invalid status: {requirement_id}")
            if status == "covered" and (
                not requirement.get("source_locations")
                or not requirement.get("proof_locations")
            ):
                errors.append(
                    f"covered requirement lacks source or proof: {requirement_id}"
                )
            assignments = requirement.get("assignments", [])
            if not isinstance(assignments, list) or any(
                value not in roster for value in assignments
            ):
                errors.append(
                    f"requirement references unknown assignment: {requirement_id}"
                )
            if status == "gap" and verdict not in {
                "REVIEW_BLOCKED",
                "MATERIAL_DECISION_REQUIRED",
            }:
                errors.append(
                    f"requirement gap requires a blocking verdict: {requirement_id}"
                )

    boundaries = data.get("high_risk_boundaries")
    if not isinstance(boundaries, list):
        errors.append("coverage.json `high_risk_boundaries` must be a list")
    else:
        for boundary in boundaries:
            if not isinstance(boundary, dict):
                errors.append("high-risk boundary entries must be objects")
                continue
            boundary_id = boundary.get("id", "<missing>")
            category = boundary.get("category")
            assignments = boundary.get("assignments", [])
            if category not in HIGH_RISK_CATEGORIES:
                errors.append(f"high-risk boundary has invalid category: {boundary_id}")
            if boundary.get("status") not in {"covered", "gap"}:
                errors.append(f"high-risk boundary has invalid status: {boundary_id}")
            if boundary.get("status") == "covered" and (
                not isinstance(assignments, list)
                or len(
                    {roster.get(value, {}).get("reviewer_id") for value in assignments}
                )
                < 2
            ):
                errors.append(
                    f"covered high-risk boundary needs two independent reviewers: {boundary_id}"
                )
            if boundary.get("status") == "gap" and verdict not in {
                "REVIEW_BLOCKED",
                "MATERIAL_DECISION_REQUIRED",
            }:
                errors.append(
                    f"boundary gap requires a blocking verdict: {boundary_id}"
                )

    capabilities = data.get("user_facing_capabilities")
    if not isinstance(capabilities, list):
        errors.append("coverage.json `user_facing_capabilities` must be a list")
    else:
        for capability in capabilities:
            if not isinstance(capability, dict):
                errors.append("user-facing capability entries must be objects")
                continue
            capability_id = capability.get("id", "<missing>")
            assignments = capability.get("assignments", [])
            if capability.get("status") not in {"covered", "gap"}:
                errors.append(
                    f"user-facing capability has invalid status: {capability_id}"
                )
            if capability.get("status") == "covered" and (
                not isinstance(assignments, list)
                or not any(
                    roster.get(value, {}).get("domain") == "tests"
                    for value in assignments
                )
            ):
                errors.append(
                    f"covered user-facing capability needs tests lens: {capability_id}"
                )
            if capability.get("status") == "gap" and verdict not in {
                "REVIEW_BLOCKED",
                "MATERIAL_DECISION_REQUIRED",
            }:
                errors.append(
                    f"capability gap requires a blocking verdict: {capability_id}"
                )

    policy_gates = data.get("policy_gates")
    if not isinstance(policy_gates, list):
        errors.append("coverage.json `policy_gates` must be a list")
    else:
        for gate in policy_gates:
            if not isinstance(gate, dict):
                errors.append("policy-gate entries must be objects")
                continue
            gate_id = gate.get("id", "<missing>")
            required = gate.get("required_assignments", [])
            status = gate.get("status")
            if status not in {"covered", "gap", "not_applicable"}:
                errors.append(f"policy gate has invalid status: {gate_id}")
            if status == "covered" and (not isinstance(required, list) or not required):
                errors.append(
                    f"covered policy gate has no required assignment: {gate_id}"
                )
            if isinstance(required, list) and any(
                value not in roster for value in required
            ):
                errors.append(f"policy gate references unknown assignment: {gate_id}")
            if status == "gap" and verdict not in {
                "REVIEW_BLOCKED",
                "MATERIAL_DECISION_REQUIRED",
            }:
                errors.append(f"policy-gate gap requires a blocking verdict: {gate_id}")
    return roster


def review_candidate_map(path: Path, errors: list[str]) -> dict[str, set[str]]:
    """Return final finding IDs mapped to candidate IDs."""
    if not path.is_file():
        errors.append(f"missing review artifact: {path}")
        return {}
    text = path.read_text(encoding="utf-8")
    findings = list(re.finditer(r"^### (REV-\d{3}): .+$", text, re.MULTILINE))
    result: dict[str, set[str]] = {}
    for index, match in enumerate(findings):
        body = text[
            match.end() : findings[index + 1].start()
            if index + 1 < len(findings)
            else len(text)
        ]
        candidates = re.search(r"^Candidates:\s*(.+)$", body, re.MULTILINE)
        if candidates is None:
            errors.append(f"{match.group(1)} has no candidate mapping")
            continue
        result[match.group(1)] = {
            value.strip() for value in candidates.group(1).split(",") if value.strip()
        }
    return result


def review_identity_map(
    path: Path, errors: list[str]
) -> dict[str, tuple[set[str], set[str], set[str]]]:
    """Return final findings mapped to domains, assignments, and reviewers."""
    if not path.is_file():
        return {}
    text = path.read_text(encoding="utf-8")
    findings = list(re.finditer(r"^### (REV-\d{3}): .+$", text, re.MULTILINE))
    result: dict[str, tuple[set[str], set[str], set[str]]] = {}
    for index, match in enumerate(findings):
        body = text[
            match.end() : findings[index + 1].start()
            if index + 1 < len(findings)
            else len(text)
        ]
        assignments = re.search(r"^Assignments:\s*(.+)$", body, re.MULTILINE)
        reviewers = re.search(r"^Reviewer IDs:\s*(.+)$", body, re.MULTILINE)
        domains = re.search(r"^Source reviewers:\s*(.+)$", body, re.MULTILINE)
        if assignments is None or reviewers is None or domains is None:
            errors.append(f"{match.group(1)} lacks source-reviewer linkage")
            continue
        result[match.group(1)] = (
            {value.strip() for value in domains.group(1).split(",") if value.strip()},
            {
                value.strip()
                for value in assignments.group(1).split(",")
                if value.strip()
            },
            {value.strip() for value in reviewers.group(1).split(",") if value.strip()},
        )
    return result


def validate_ledger(
    data: Any,
    specialist_sources: dict[str, tuple[str, str]],
    review_map: dict[str, set[str]],
    review_identities: dict[str, tuple[set[str], set[str], set[str]]],
    validation_text: str,
    coverage: Any,
    require_specialist_mapping: bool,
    allow_blocked: bool,
    errors: list[str],
) -> None:
    """Validate candidate lifecycle and final-finding mappings."""
    if not isinstance(data, dict) or not isinstance(data.get("candidates"), list):
        errors.append("ledger.json must contain a `candidates` list")
        return
    ledger_ids: set[str] = set()
    confirmed_map: dict[str, str] = {}
    merged_targets: dict[str, str] = {}
    candidate_links: dict[str, tuple[set[str], set[str]]] = {}
    independent_required = (
        not allow_blocked
        and isinstance(coverage, dict)
        and coverage.get("risk") == "high"
    )
    for entry in data["candidates"]:
        if not isinstance(entry, dict):
            errors.append("ledger candidate entries must be objects")
            continue
        candidate_id = entry.get("candidate_id")
        if not isinstance(candidate_id, str) or not CANDIDATE_ID.fullmatch(
            candidate_id
        ):
            errors.append(f"ledger has invalid candidate ID: {candidate_id}")
            continue
        if candidate_id in ledger_ids:
            errors.append(f"ledger contains duplicate candidate: {candidate_id}")
        ledger_ids.add(candidate_id)
        status = entry.get("status")
        if status not in STATUSES:
            errors.append(f"{candidate_id} has invalid ledger status: {status}")
        final_finding = entry.get("final_finding")
        merged_into = entry.get("merged_into")
        reason = entry.get("reason")
        severity = entry.get("original_severity")
        hard_gate = entry.get("hard_gate")
        dissent = entry.get("dissent")
        assignment_ids = entry.get("assignment_ids")
        reviewer_ids = entry.get("reviewer_ids")
        source_reviewers = entry.get("source_reviewers")
        if severity not in {"critical", "high", "medium", "low"}:
            errors.append(f"{candidate_id} has invalid original severity: {severity}")
        if not isinstance(hard_gate, bool):
            errors.append(f"{candidate_id} has invalid hard-gate flag")
        if not isinstance(dissent, list):
            errors.append(f"{candidate_id} has invalid dissent list")
            dissent = []
        if not isinstance(assignment_ids, list) or not assignment_ids:
            errors.append(f"{candidate_id} has no assignment IDs")
        if not isinstance(reviewer_ids, list) or not reviewer_ids:
            errors.append(f"{candidate_id} has no reviewer IDs")
        candidate_links[candidate_id] = (
            set(assignment_ids) if isinstance(assignment_ids, list) else set(),
            set(reviewer_ids) if isinstance(reviewer_ids, list) else set(),
        )
        roster = (
            {
                assignment.get("assignment_id"): assignment
                for assignment in coverage.get("roster", [])
                if isinstance(assignment, dict)
            }
            if isinstance(coverage, dict)
            else {}
        )
        if isinstance(assignment_ids, list):
            if set(assignment_ids) - set(roster):
                errors.append(f"{candidate_id} references unknown assignments")
            expected_reviewers = {
                roster[value].get("reviewer_id")
                for value in assignment_ids
                if value in roster
            }
            expected_domains = {
                roster[value].get("domain")
                for value in assignment_ids
                if value in roster
            }
            if (
                isinstance(reviewer_ids, list)
                and set(reviewer_ids) != expected_reviewers
            ):
                errors.append(f"{candidate_id} reviewer IDs do not match assignments")
            if (
                not isinstance(source_reviewers, list)
                or set(source_reviewers) != expected_domains
            ):
                errors.append(
                    f"{candidate_id} source reviewers do not match assignments"
                )
        source = specialist_sources.get(candidate_id)
        if require_specialist_mapping and source is not None:
            assignment_id, reviewer_id = source
            if assignment_id not in assignment_ids:
                errors.append(f"{candidate_id} ledger omits source assignment")
            if reviewer_id not in reviewer_ids:
                errors.append(f"{candidate_id} ledger omits source reviewer")
        if dissent:
            independent_required = True
        if status == "REJECTED" and (
            severity in {"critical", "high"} or hard_gate is True
        ):
            independent_required = True
        if not isinstance(reason, str) or not reason.strip():
            errors.append(f"{candidate_id} has no decision reason")
        if status == "CONFIRMED":
            if not isinstance(final_finding, str) or not re.fullmatch(
                r"REV-\d{3}", final_finding
            ):
                errors.append(
                    f"confirmed candidate lacks final finding: {candidate_id}"
                )
            else:
                confirmed_map[candidate_id] = final_finding
        elif final_finding is not None:
            errors.append(f"non-confirmed candidate has final finding: {candidate_id}")
        if status == "MERGED":
            if not isinstance(merged_into, str):
                errors.append(f"merged candidate lacks target: {candidate_id}")
            else:
                merged_targets[candidate_id] = merged_into
        elif merged_into is not None:
            errors.append(f"non-merged candidate has merge target: {candidate_id}")
        heading = f"### {candidate_id}:"
        if require_specialist_mapping and validation_text.count(heading) != 1:
            errors.append(
                f"candidate lacks exactly one validation section: {candidate_id}"
            )
        elif require_specialist_mapping:
            after = validation_text.split(heading, 1)[1]
            next_candidate = re.search(r"^### ", after, re.MULTILINE)
            validation_body = (
                after[: next_candidate.start()] if next_candidate else after
            )
            for field in VALIDATION_FIELDS:
                if not re.search(
                    rf"^{re.escape(field)}\s*\S", validation_body, re.MULTILINE
                ):
                    errors.append(f"{candidate_id} validation is missing `{field}`")

    if require_specialist_mapping:
        specialist_ids = set(specialist_sources)
        for candidate_id in sorted(specialist_ids - ledger_ids):
            errors.append(f"specialist candidate is absent from ledger: {candidate_id}")
        for candidate_id in sorted(ledger_ids - specialist_ids):
            errors.append(
                f"ledger candidate is absent from specialists: {candidate_id}"
            )
    for candidate_id, target in merged_targets.items():
        if target not in confirmed_map:
            errors.append(
                f"merged candidate target is not confirmed: {candidate_id} -> {target}"
            )

    for finding_id, candidate_ids in review_map.items():
        final_domains, final_assignments, final_reviewers = review_identities.get(
            finding_id, (set(), set(), set())
        )
        expected_domains: set[str] = set()
        expected_assignments: set[str] = set()
        expected_reviewers: set[str] = set()
        for candidate_id in candidate_ids:
            assignments, reviewers = candidate_links.get(candidate_id, (set(), set()))
            expected_assignments.update(assignments)
            expected_reviewers.update(reviewers)
            for assignment_id in assignments:
                for assignment in coverage.get("roster", []):
                    if assignment.get("assignment_id") == assignment_id:
                        expected_domains.add(assignment.get("domain"))
        if not expected_assignments.issubset(final_assignments):
            errors.append(f"{finding_id} omits source assignments")
        if not expected_reviewers.issubset(final_reviewers):
            errors.append(f"{finding_id} omits source reviewer IDs")
        if expected_domains != final_domains:
            errors.append(f"{finding_id} source reviewers do not match candidates")

    mapped_from_review = set().union(*review_map.values()) if review_map else set()
    expected_mapped = set(confirmed_map) | set(merged_targets)
    for candidate_id in sorted(expected_mapped - mapped_from_review):
        errors.append(
            f"confirmed or merged candidate is absent from review: {candidate_id}"
        )
    for candidate_id in sorted(mapped_from_review - expected_mapped):
        errors.append(f"review maps a non-final candidate: {candidate_id}")
    for candidate_id, finding_id in confirmed_map.items():
        if candidate_id not in review_map.get(finding_id, set()):
            errors.append(
                f"candidate mapping disagrees with ledger: {candidate_id} -> {finding_id}"
            )
    for candidate_id, target in merged_targets.items():
        finding_id = confirmed_map.get(target)
        if finding_id is not None and candidate_id not in review_map.get(
            finding_id, set()
        ):
            errors.append(
                f"merged candidate mapping disagrees with ledger: "
                f"{candidate_id} -> {finding_id}"
            )

    independent = (
        coverage.get("independent_validation", {}) if isinstance(coverage, dict) else {}
    )
    if isinstance(coverage, dict) and coverage.get("mode") == "quick":
        independent_required = False
    if independent_required and independent.get("status") != "completed":
        errors.append("independent validation was required but not completed")
    roster_reviewers = (
        {
            assignment.get("reviewer_id")
            for assignment in coverage.get("roster", [])
            if isinstance(assignment, dict)
        }
        if isinstance(coverage, dict)
        else set()
    )
    if (
        independent.get("status") == "completed"
        and independent.get("reviewer") in roster_reviewers
    ):
        errors.append("independent validator also produced a candidate report")


def validate(review_dir: Path) -> list[str]:
    """Return every full-review evidence contract error."""
    errors: list[str] = []
    evidence = review_dir / "evidence"
    coverage = load_json(evidence / "coverage.json", errors)
    mode = coverage.get("mode") if isinstance(coverage, dict) else None
    packet = evidence / "packet.md"
    validation = evidence / "validation.md"
    specialists = evidence / "specialists"
    if not packet.is_file() or not packet.read_text(encoding="utf-8").strip():
        errors.append(f"missing or empty evidence packet: {packet}")
    if mode == "full":
        if (
            not validation.is_file()
            or not validation.read_text(encoding="utf-8").strip()
        ):
            errors.append(f"missing or empty validation artifact: {validation}")
            validation_text = ""
        else:
            validation_text = validation.read_text(encoding="utf-8")
    else:
        validation_text = ""
    if mode == "full" and not specialists.is_dir():
        errors.append(f"missing specialist evidence directory: {specialists}")

    ledger = load_json(evidence / "ledger.json", errors)
    review_path = review_dir / "review.md"
    review_text = (
        review_path.read_text(encoding="utf-8") if review_path.is_file() else ""
    )
    verdict_match = re.search(r"^Verdict:\s*(\S+)\s*$", review_text, re.MULTILINE)
    verdict = verdict_match.group(1) if verdict_match else None
    roster = validate_coverage(coverage, errors, verdict)
    specialist_sources = (
        validate_specialists(specialists, roster, errors)
        if mode == "full" and specialists.is_dir()
        else {}
    )
    review_map = review_candidate_map(review_dir / "review.md", errors)
    review_identities = review_identity_map(review_dir / "review.md", errors)
    validate_ledger(
        ledger,
        specialist_sources,
        review_map,
        review_identities,
        validation_text,
        coverage,
        mode == "full",
        verdict == "REVIEW_BLOCKED",
        errors,
    )
    return errors


def main() -> int:
    """Validate one full review directory from the command line."""
    if len(sys.argv) != 2:
        print("usage: validate_evidence.py <review-directory>", file=sys.stderr)
        return 2
    review_dir = Path(sys.argv[1]).expanduser().resolve()
    errors = validate(review_dir)
    if errors:
        print("evidence validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"evidence validation passed: {review_dir / 'evidence'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
