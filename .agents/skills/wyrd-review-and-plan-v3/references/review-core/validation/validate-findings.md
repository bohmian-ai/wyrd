# Validate and Consolidate Review Candidates

Validate every specialist candidate against current source, eliminate false
positives, deduplicate shared root causes, apply the production materiality
gate, and ground final corrections directly in the consolidated `review.md`.

Read `../artifact-contract.md` and `maintainer-gate.md` completely first.

## Inputs

- stable evidence packet;
- candidate reports from every selected reviewer;
- quality-coverage and Rust structural matrices when applicable;
- approved intent and requirement map;
- applicable repository instructions and hard gates;
- current source, callers, contracts, tests, and local precedents.

Candidate reports are durable supporting evidence and follow
`../specialist-contract.md`.

## 1. Parse candidates

Build one candidate list containing:

- immutable namespaced candidate ID;
- assignment IDs, reviewer IDs, and required capabilities;
- source reviewer and proposed category;
- proposed severity and confidence;
- locations and symbols;
- issue and why it matters;
- evidence and production trigger;
- suggested correction and test oracle.

Capture clean reviewer coverage separately. A clean specialist response is
successful evidence, not an empty artifact.

## 2. Consolidate obvious duplicates

Group candidates that describe the same root cause even when reviewers cite
different symptoms, categories, or locations.

Keep the clearest issue statement and merge:

- all source reviewers;
- all material locations;
- distinct supporting evidence;
- every affected requirement;
- the strongest valid test oracle.
- every source assignment and reviewer identity;
- every distinct affected surface and impact.

Do not create one final finding per reviewer, file, comment, test, or symptom.

## 3. Validate against source

For each consolidated candidate:

1. Read the exact cited location and full owning symbol.
2. Inspect direct callers and affected downstream consumers.
3. Inspect related types, contracts, tests, fixtures, and generated surfaces.
4. Inspect one to three local precedents when the claim depends on convention.
5. Compare architecture and ownership claims with approved intent,
   active authority, reverse dependencies, and feature cones.
6. Challenge whether the proposed correction preserves the user workflow and
   existing owner.
7. Attempt to disprove the candidate with the strongest realistic
   counterexample supported by the changed impact cone.

Classify in the durable ledger:

- `CONFIRMED`;
- `MERGED`;
- `FOLLOW_UP`;
- `KNOWN_DEFERRED`;
- `STATIC_LIMIT`;
- `REJECTED`;
- `MATERIAL_DECISION_REQUIRED`.

Reject a candidate when the location does not exist, source disproves it, it is
preference-only, it follows stale authority, or its proposed correction creates
greater unapproved contract or dependency harm.

Before accepting a clean verdict, apply `../adversarial-contract.md`: challenge
the three most dangerous changed invariants and record how each survived,
produced a candidate, or remained a static-analysis limit.

When static evidence cannot settle behavior, do not use an executable check as
a substitute inside this terminal review. Reject speculative candidates and
record a material static-analysis limit when useful.

## 4. Apply materiality and hard gates

Apply `maintainer-gate.md` to each confirmed candidate.

Record:

```text
Production trigger:
Material impact:
Evidence and reachability:
Disposition:
Disposition rationale:
Hard gate:
```

Only `BLOCK_BEFORE_MERGE` and `FIX_BEFORE_PRODUCTION` become `REV-NNN`
findings and enter the v3 remediation or material-decision handoff.

Keep `FOLLOW_UP`, `KNOWN_DEFERRED`, and material static-analysis limits visible
in their dedicated `review.md` sections without `REV-NNN` IDs.

A confirmed applicable hard-gate violation in new or materially modified code
is always `BLOCK_BEFORE_MERGE`.

When more than eight required root causes remain, perform another consolidation
pass and explain why each is independently required.

## 5. Ground the final finding

Before assigning `REV-NNN`, inspect enough current implementation to make its
correction executable:

- complete owner and affected callers;
- input, output, error, state, persistence, and wire types;
- nearest successful local pattern;
- existing tests and sanctioned failure seams;
- dependency and feature-cone effects.

Write all of this into the final finding:

- the simple maintainer summary required by `orchestration-contract.md`;
- problem, failure scenario, recommended correction, and verification as
  separate, self-contained blocks;

- target invariant and post-fix behavior;
- existing or natural owner;
- ordered implementation direction;
- affected surfaces;
- constraints and non-goals;
- failure, retry, replay, cancellation, concurrency, tenancy, persistence, and
  idempotency semantics when applicable;
- complete Rust structural contract when applicable;
- exact setup/action/assertion regression oracle;
- future regression proof and focused terminal re-review criteria.

Rewrite any block that refers to the title, a cited owner, a generic production
workflow, or external evidence for missing facts. Reject copied prose across
unrelated findings. Location annotations must identify the cited symbol's role
in the defect.

Do not invent a new symbol, API, table, field, route, or signature without
current source or local precedent. If a safe design still requires a material
choice, use `MATERIAL_DECISION_REQUIRED` and route it through `$wyrd-plan-v3`.

## 6. Build the validation evidence

Write `evidence/validation.md` with one complete section per candidate as
defined by `../evidence-contract.md`. Write `evidence/ledger.json` with every
candidate, including merged and rejected candidates. Preserve material dissent
and the independent validator's challenges when applicable.

Summarize compactly in `review.md`:

- total candidates;
- confirmed required root causes;
- duplicates and their target `REV-NNN`;
- rejected candidates and one concrete reason each;
- material static-analysis limits;
- follow-ups and known deferrals;
- required counts by disposition.

The summary keeps the authoritative review concise; the durable evidence makes
every decision inspectable.

## 7. Audit the final report

Before handoff:

- every final finding is self-contained;
- every contributing reviewer is preserved;
- every hard gate is correctly classified;
- every required correction is grounded;
- follow-ups and static-analysis limits are excluded from required work;
- verdict matches the remaining dispositions;
- no other review artifact is required to understand an issue.

Run `scripts/validate_review.py` and `scripts/validate_evidence.py`; fix every
reported error.

Then emit bounded v3 remediation contracts for `REVERSIBLE` and
`TASK_CONTRACT_REPAIR` findings. Invoke `$wyrd-plan-v3` only when a `MATERIAL`
finding requires a new decision.
