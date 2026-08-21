# Wyrd v3 Consolidated Review Artifact Contract

Persist one authoritative terminal review at `{REVIEW_DIR}/review.md` and the
supporting full-mode evidence required by `evidence-contract.md`. Evidence
proves how the conclusion was reached; no evidence file may be required to
understand or execute a final remediation finding.

Do not create a review-private implementation plan. Ordinary findings become
v3 remediation contracts. Only a `MATERIAL` finding may invoke
`$wyrd-plan-v3`.

## Review metadata

`review.md` begins with:

```markdown
# Review: <outcome or change>

Review ID: <id>
Mode: full
Verdict: CLEAN | REMEDIATION_REQUIRED | MATERIAL_DECISION_REQUIRED | REVIEW_BLOCKED
Risk: low | standard | high
Base: <ref>@<resolved SHA>
Target: <ref>@<resolved SHA>
Merge base: <resolved SHA>
Reference: <intent path and digest>
Plan: <approved v3 plan path and digest>
Static analysis: no runtime verification performed
Execution handoff: <Ready to push | Controller remediation required | v3 plan path | Not ready — reason>
```

## Required report sections

Use these sections exactly once and in order:

1. `## Executive assessment`
2. `## Scope and intent`
3. `## Lens coverage`
4. `## Requirement traceability`
5. `## Confirmed findings`
6. `## Follow-ups and deferrals`
7. `## Static analysis boundary`
8. `## Validation ledger`
9. `## V3 handoff`

Use `None.` only when a section genuinely has no entries. Do not omit sections.

## Final findings

Assign `REV-NNN` only after source validation and root-cause deduplication.
Each final finding stands alone:

```markdown
### REV-001: <plain-English defect>

Severity: critical | high | medium | low
Confidence: high | medium | low
Class: REVERSIBLE | TASK_CONTRACT_REPAIR | MATERIAL
Disposition: BLOCK_BEFORE_MERGE | FIX_BEFORE_PRODUCTION
Category: <category>
Source reviewers: <review domains>
Assignments: <assignment IDs>
Reviewer IDs: <reviewer identities>
Candidates: <namespaced candidate IDs>
Affected tasks: <task IDs>
Requirements: <IDs or none>
Rules: <exact rules or none>
Hard gate: yes | no
Structural change: yes | no
Locations:
- `path:line` — `symbol and relevance`

#### Maintainer summary
<Two to four standalone sentences naming current behavior, consequence, and
correction.>

#### Problem
<Current flow, expected behavior, exact divergence, and evidence.>

#### Failure scenario
<Reachable entry point and state, observable failure, affected parties, blast
radius, detectability, and recovery.>

#### Recommended correction
<Natural owner, ordered behavioral changes, constraints, precedent, non-goals,
and unsafe shapes.>

#### Required structure
<Complete Rust structural contract below, or `Not applicable` and why.>

#### Verification
<Test tier/location, setup, action, exact assertions and assertion IDs,
required command IDs, and focused static closure inspection.>
```

For `Structural change: yes`, `Required structure` contains:

```text
Natural owner:
Target owner:
Composed state:
Public methods:
Private workflow methods:
Pure helpers:
Remaining free-function justification:
Sync/async boundary:
Rustdoc coverage:
Local precedent:
Forbidden structure:
```

Required findings use only `BLOCK_BEFORE_MERGE` or
`FIX_BEFORE_PRODUCTION`. Follow-ups, known deferrals, and static limits remain
in their dedicated sections and receive no `REV-NNN` ID.

Every final finding must preserve all contributing candidates, assignments,
reviewers, requirements, rules, exact target source, production trigger,
material impact, natural owner, constraints, and regression oracle. Essential
facts belong in the finding, not only in specialist evidence.

## Verdict and handoff consistency

- `CLEAN` contains no required findings and uses
  `Execution handoff: Ready to push`.
- `REMEDIATION_REQUIRED` contains at least one `REVERSIBLE` or
  `TASK_CONTRACT_REPAIR` finding and uses
  `Execution handoff: Controller remediation required`.
- `MATERIAL_DECISION_REQUIRED` contains at least one `MATERIAL` finding and
  uses a concrete `$wyrd-plan-v3` path or material-authority blocker.
- `REVIEW_BLOCKED` records the missing identity, authority, evidence, roster,
  independence, or coverage and uses `Execution handoff: Not ready — <reason>`.

The `## V3 handoff` section repeats each ordinary finding's bounded remediation
contract: class, affected tasks and owners, evidence, consequence, expected
outcome, acceptance assertion IDs, and verification command/assertion IDs.
Material findings link to the canonical v3 planning result. Never create a
second or review-private task format.
