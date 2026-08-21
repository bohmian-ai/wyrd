#!/usr/bin/env python3
"""Validate the consolidated Wyrd v3 terminal review artifact."""

from __future__ import annotations

import re
import sys
from pathlib import Path


METADATA = (
    "Review ID:",
    "Mode:",
    "Verdict:",
    "Risk:",
    "Base:",
    "Target:",
    "Merge base:",
    "Reference:",
    "Plan:",
    "Static analysis:",
    "Execution handoff:",
)

SECTIONS = (
    "## Executive assessment",
    "## Scope and intent",
    "## Lens coverage",
    "## Requirement traceability",
    "## Confirmed findings",
    "## Follow-ups and deferrals",
    "## Static analysis boundary",
    "## Validation ledger",
    "## V3 handoff",
)

FINDING = re.compile(r"^### (REV-\d{3}): (.+)$", re.MULTILINE)
FINDING_FIELDS = (
    "Severity:",
    "Confidence:",
    "Class:",
    "Disposition:",
    "Category:",
    "Source reviewers:",
    "Assignments:",
    "Reviewer IDs:",
    "Candidates:",
    "Affected tasks:",
    "Requirements:",
    "Rules:",
    "Hard gate:",
    "Structural change:",
)
FINDING_SECTIONS = (
    "#### Maintainer summary",
    "#### Problem",
    "#### Failure scenario",
    "#### Recommended correction",
    "#### Required structure",
    "#### Verification",
)
REQUIRED_STRUCTURE_FIELDS = (
    "Natural owner:",
    "Target owner:",
    "Composed state:",
    "Public methods:",
    "Private workflow methods:",
    "Pure helpers:",
    "Remaining free-function justification:",
    "Sync/async boundary:",
    "Rustdoc coverage:",
    "Local precedent:",
    "Forbidden structure:",
)

VERDICTS = {
    "CLEAN",
    "REMEDIATION_REQUIRED",
    "MATERIAL_DECISION_REQUIRED",
    "REVIEW_BLOCKED",
}
MODES = {"full"}
RISKS = {"low", "standard", "high"}
SEVERITIES = {"critical", "high", "medium", "low"}
CONFIDENCES = {"high", "medium", "low"}
REQUIRED_DISPOSITIONS = {"BLOCK_BEFORE_MERGE", "FIX_BEFORE_PRODUCTION"}
FINDING_CLASSES = {"REVERSIBLE", "TASK_CONTRACT_REPAIR", "MATERIAL"}
PLACEHOLDER = re.compile(
    r"^(?:tbd|todo|pending|unknown|placeholder)(?:\b|$)",
    re.IGNORECASE,
)
PROHIBITED_FINDING_PHRASES = (
    "the cited owner",
    "the invariant expressed by the title",
    "normal production workflow",
    "validated source location",
)


def value_for(text: str, field: str) -> str | None:
    match = re.search(
        rf"^{re.escape(field)}[ \t]*(.+)$",
        text,
        re.MULTILINE,
    )
    return match.group(1).strip() if match else None


def ordered_sections(text: str, errors: list[str]) -> None:
    positions: list[int] = []
    for section in SECTIONS:
        count = text.count(section)
        if count != 1:
            errors.append(f"review must contain exactly one `{section}` section")
            continue
        positions.append(text.index(section))
    if len(positions) == len(SECTIONS) and positions != sorted(positions):
        errors.append("required review sections are not in canonical order")


def finding_blocks(text: str) -> list[tuple[str, str, str]]:
    confirmed_start = text.find("## Confirmed findings")
    confirmed_end = text.find("## Follow-ups and deferrals")
    if confirmed_start < 0 or confirmed_end < 0:
        return []
    confirmed = text[confirmed_start:confirmed_end]
    matches = list(FINDING.finditer(confirmed))
    return [
        (
            match.group(1),
            match.group(2).strip(),
            confirmed[
                match.end() : matches[index + 1].start()
                if index + 1 < len(matches)
                else len(confirmed)
            ],
        )
        for index, match in enumerate(matches)
    ]


def section_body(body: str, section: str) -> str:
    """Return one finding subsection body, or an empty string when absent."""
    if section not in body:
        return ""
    after = body.split(section, 1)[1]
    next_section = re.search(r"^#### ", after, re.MULTILINE)
    return (after[: next_section.start()] if next_section else after).strip()


def top_section_body(text: str, section: str) -> str:
    """Return one top-level report section body, or empty text when absent."""
    if section not in text:
        return ""
    after = text.split(section, 1)[1]
    next_section = re.search(r"^## ", after, re.MULTILINE)
    return (after[: next_section.start()] if next_section else after).strip()


def is_placeholder(value: str) -> bool:
    """Return whether a required value is an unresolved template marker."""
    normalized = value.strip().strip("`")
    return (
        not normalized
        or normalized.startswith("<")
        or normalized.endswith(">")
        or PLACEHOLDER.match(normalized) is not None
    )


def normalized_prose(value: str) -> str:
    """Normalize substantive prose for title and duplicate comparisons."""
    return " ".join(re.sub(r"[^a-z0-9`]+", " ", value.lower()).split())


def validate_findings(text: str, errors: list[str]) -> set[str]:
    ids: set[str] = set()
    substantive_blocks: dict[str, tuple[str, str]] = {}
    for finding_id, title, body in finding_blocks(text):
        if finding_id in ids:
            errors.append(f"duplicate finding ID: {finding_id}")
        ids.add(finding_id)

        for field in FINDING_FIELDS:
            if value_for(body, field) is None:
                errors.append(f"{finding_id} is missing `{field}`")

        if len(re.findall(r"^Locations:[ \t]*$", body, re.MULTILINE)) != 1:
            errors.append(f"{finding_id} must contain exactly one `Locations:`")
        else:
            locations = body.split("Locations:", 1)[1].split(
                "#### Maintainer summary",
                1,
            )[0]
            if not re.search(r"^- `[^`]+`(?: — .+)?$", locations, re.MULTILINE):
                errors.append(
                    f"{finding_id} must contain a concrete `Locations:` entry"
                )

        lowered_body = body.lower()
        for phrase in PROHIBITED_FINDING_PHRASES:
            if phrase in lowered_body:
                errors.append(
                    f"{finding_id} uses non-actionable placeholder phrase `{phrase}`"
                )

        severity = value_for(body, "Severity:")
        if severity is not None and severity not in SEVERITIES:
            errors.append(f"{finding_id} has invalid severity `{severity}`")

        confidence = value_for(body, "Confidence:")
        if confidence is not None and confidence not in CONFIDENCES:
            errors.append(f"{finding_id} has invalid confidence `{confidence}`")

        finding_class = value_for(body, "Class:")
        if finding_class is not None and finding_class not in FINDING_CLASSES:
            errors.append(f"{finding_id} has invalid class `{finding_class}`")

        disposition = value_for(body, "Disposition:")
        if disposition is not None and disposition not in REQUIRED_DISPOSITIONS:
            errors.append(
                f"{finding_id} has non-required disposition `{disposition}`; "
                "follow-ups and static-analysis limits belong in dedicated "
                "sections"
            )

        hard_gate = value_for(body, "Hard gate:")
        if hard_gate not in {"yes", "no", None}:
            errors.append(f"{finding_id} has invalid hard-gate value `{hard_gate}`")
        if hard_gate == "yes" and disposition != "BLOCK_BEFORE_MERGE":
            errors.append(
                f"{finding_id} is a hard-gate finding but is not "
                "`BLOCK_BEFORE_MERGE`"
            )

        structural = value_for(body, "Structural change:")
        if structural not in {"yes", "no", None}:
            errors.append(
                f"{finding_id} has invalid structural-change value `{structural}`"
            )

        for section in FINDING_SECTIONS:
            count = body.count(section)
            if count != 1:
                errors.append(
                    f"{finding_id} must contain exactly one `{section}` section"
                )
                continue
            if not section_body(body, section):
                errors.append(f"{finding_id} has an empty `{section}` section")

        summary = section_body(body, "#### Maintainer summary")
        if summary:
            if normalized_prose(summary) == normalized_prose(title):
                errors.append(
                    f"{finding_id} maintainer summary only restates its title"
                )
            sentences = re.findall(r"[^.!?]+[.!?](?:\s|$)", summary)
            if len(sentences) < 2 or len(sentences) > 4:
                errors.append(
                    f"{finding_id} maintainer summary must contain 2-4 sentences"
                )

        correction = section_body(body, "#### Recommended correction")
        if correction and not re.search(r"`[^`]+`", correction):
            errors.append(
                f"{finding_id} recommended correction must name an owner or symbol"
            )

        verification = section_body(body, "#### Verification")
        if verification:
            if not re.search(r"`[^`]+`", verification):
                errors.append(
                    f"{finding_id} verification must name a test location or symbol"
                )
            if "assert" not in verification.lower():
                errors.append(f"{finding_id} verification must state exact assertions")

        for section in (
            "#### Maintainer summary",
            "#### Problem",
            "#### Failure scenario",
            "#### Recommended correction",
            "#### Verification",
        ):
            content = normalized_prose(section_body(body, section))
            if len(content) < 100:
                continue
            previous = substantive_blocks.get(content)
            if previous is not None:
                prior_id, prior_section = previous
                errors.append(
                    f"{finding_id} `{section}` duplicates substantive prose from "
                    f"{prior_id} `{prior_section}`"
                )
            else:
                substantive_blocks[content] = (finding_id, section)

        if structural == "yes":
            structure = section_body(body, "#### Required structure")
            for field in REQUIRED_STRUCTURE_FIELDS:
                value = value_for(structure, field)
                if value is None:
                    errors.append(
                        f"{finding_id} structural contract is missing `{field}`"
                    )
                elif is_placeholder(value):
                    errors.append(
                        f"{finding_id} structural contract has unresolved "
                        f"`{field}` value `{value}`"
                    )

        if not re.search(
            r"`[^`]+(?:\.rs|\.py|\.ts|\.tsx|\.svelte|\.sql|\.md|"
            r"\.toml|\.yaml|\.yml)[^`]*`",
            body,
        ):
            errors.append(f"{finding_id} does not cite a concrete source path")

    return ids


def validate_status(text: str, finding_ids: set[str], errors: list[str]) -> None:
    mode = value_for(text, "Mode:")
    verdict = value_for(text, "Verdict:")
    risk = value_for(text, "Risk:")
    review_id = value_for(text, "Review ID:")
    base = value_for(text, "Base:")
    target = value_for(text, "Target:")
    merge_base = value_for(text, "Merge base:")
    static_analysis = value_for(text, "Static analysis:")
    execution_handoff = value_for(text, "Execution handoff:")

    if mode not in MODES:
        errors.append(f"invalid or missing mode: `{mode}`")
    if verdict not in VERDICTS:
        errors.append(f"invalid or missing verdict: `{verdict}`")
    if risk not in RISKS:
        errors.append(f"invalid or missing risk: `{risk}`")
    if review_id is None or not review_id.strip():
        errors.append("missing review ID")
    branch_snapshot = re.compile(r"^[^@\s]+@[0-9a-fA-F]{7,64}$")
    if base is not None and branch_snapshot.fullmatch(base) is None:
        errors.append("base must be `<branch>@<resolved SHA>`")
    if target is not None and branch_snapshot.fullmatch(target) is None:
        errors.append("target must be `<branch>@<resolved SHA>`")
    if (
        merge_base is not None
        and re.fullmatch(
            r"[0-9a-fA-F]{7,64}",
            merge_base,
        )
        is None
    ):
        errors.append("merge base must be a resolved commit SHA")
    if static_analysis != "no runtime verification performed":
        errors.append("static analysis must state `no runtime verification performed`")

    classes = {value_for(body, "Class:") for _, _, body in finding_blocks(text)}
    if verdict == "CLEAN":
        if finding_ids:
            errors.append("CLEAN review cannot contain required findings")
        if execution_handoff != "Ready to push":
            errors.append("CLEAN review must use `Execution handoff: Ready to push`")
    if verdict == "REMEDIATION_REQUIRED":
        if not finding_ids or not classes.intersection(
            {"REVERSIBLE", "TASK_CONTRACT_REPAIR"}
        ):
            errors.append(
                "REMEDIATION_REQUIRED review needs an ordinary remediation finding"
            )
        if execution_handoff != "Controller remediation required":
            errors.append("REMEDIATION_REQUIRED review must use the controller handoff")
    if verdict == "MATERIAL_DECISION_REQUIRED" and "MATERIAL" not in classes:
        errors.append("MATERIAL_DECISION_REQUIRED review needs a MATERIAL finding")
    if verdict == "REVIEW_BLOCKED" and execution_handoff is not None:
        if not execution_handoff.startswith("Not ready — "):
            errors.append(
                "REVIEW_BLOCKED review must use `Execution handoff: Not ready — <reason>`"
            )


def validate(path: Path) -> list[str]:
    errors: list[str] = []
    if not path.is_file():
        return [f"missing review artifact: {path}"]

    text = path.read_text(encoding="utf-8")
    if not text.strip():
        return [f"review artifact is empty: {path}"]
    if not text.startswith("# Review: "):
        errors.append("review must begin with `# Review: <outcome>`")

    for field in METADATA:
        count = len(re.findall(rf"^{re.escape(field)}", text, re.MULTILINE))
        if count != 1:
            errors.append(f"review must contain exactly one `{field}` line")
            continue
        value = value_for(text, field)
        if value is None:
            errors.append(f"review metadata `{field}` must not be empty")
        elif is_placeholder(value):
            errors.append(f"review metadata `{field}` has unresolved value `{value}`")

    ordered_sections(text, errors)
    for section in SECTIONS:
        if text.count(section) == 1:
            body = top_section_body(text, section)
            if not body:
                errors.append(f"review section `{section}` must not be empty")
            elif is_placeholder(body):
                errors.append(f"review section `{section}` has unresolved content")
    finding_ids = validate_findings(text, errors)
    validate_status(text, finding_ids, errors)
    return errors


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: validate_review.py <review.md-or-directory>", file=sys.stderr)
        return 2

    path = Path(sys.argv[1]).expanduser().resolve()
    if path.is_dir():
        path = path / "review.md"

    errors = validate(path)
    if errors:
        print("review validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"review validation passed: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
