---
id: TASK-001-R3
kind: remediation
status: complete
spec: SPEC-verifier-building-blocks
spec_revision: 6
requirements: [REQ-003, AC-001, AC-010]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-7]
---

# Recognize declared literal-pattern fields in references

## Authority and subject

Route to `$wyrd-implement` for one bounded implementation correction.

- Root: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Approved r6 spec: `changes/active/verifier-building-blocks/spec.md`, captured here as `spec-snapshot.md`.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-001-verifier-contract.md`, captured as `task-snapshot.md`.
- Original acceptance base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Reviewed cumulative candidate / remediation starting point: `542957a3622461e259aee5f8108d978e18adad1c`.
- Latest prior candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`.
- Current verdict and independently validated diagnosis: `verdict.md` and `findings-validation.md` alongside this task.
- Prior R1: `changes/active/verifier-building-blocks/review/task-001-20261015-896e563be/TASK-001-R1-contract-gaps.md` and its verdict/ledger.
- Prior R2: `changes/active/verifier-building-blocks/review/task-001-r1-20261015-eb1681461/TASK-001-R2-verdict-schema-closure.md` and its verdict/ledger; latest R2 task/evidence captured as `remediation-task-snapshot.md` here.

Prior findings 1–6 are CLOSED from current source; preserve them and do not reopen their boundaries. FIND-7 is a distinct original REQ-003 selector obligation, not another Verdict-fit or recursion correction.

## Outcome and ownership

A valid exact-name pattern declaration must remain referenceable from both input slots and preparation output, while genuinely absent fields are still refused. Correct the shared schema-path field-presence owner once using its existing bounded recognition and traversal. Own `wyrd-spec/src/card/blocks/schema_path.rs` and its existing public validation fixtures in `validate.rs`; change no runtime or contract field. Other contributors may be active; do not revert their changes.

## Independently validated diagnosis and selected correction


**FIND-TASK-001-7 — A declared literal-pattern field is refused as missing.**

- **Discovery / disposition / classification:** INV-R2-001 / CONFIRMED / INCORRECT.
- **Violated obligation:** REQ-003 and original Task 001 Scenario 2 require known-missing detection while preserving usable known and unknown shapes. This is reference-authoring behavior, independent of FIND-3's check-Agent Verdict compatibility obligation.
- **Exact location:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:93–94`, in `Walk::admits`; consumers at `card/blocks/validate.rs:692` and `:738`.
- **Evidence:** for schema `{"type":"object","patternProperties":{"^score$":{"type":"number"}},"additionalProperties":false}`, `score` is a declared field and `{"score":1}` conforms. `admits` finds no `properties.score`, does not exclude object type, then returns false solely from `additionalProperties:false`. It never consults `patternProperties`. The existing `literal_name` at `schema_path.rs:280` already recognizes precisely this bounded declaration in the sibling `fits_object` consumer at `:202–218`.
- **Observable consequence:** public staged `VerifierSpec::validate()` refuses a valid JSON-input reference or preparation-output reference with rule `missing_field`. This blocks pure authoring now; it does not depend on a future provider, engine or mounted server route.
- **Decision-complete minimal correction:** change the field-presence decision once in the existing bounded `Walk::admits`, reusing `literal_name` to recognize an exact anchored declaration matching the selected field before concluding that a closed object's field is absent. Traverse that declaration's remaining selectors using the same root and retained work budget. Preserve named-property traversal, ordinary known-missing refusal, local refs, unknown/budget fallback, action output freedom, and the distinct Verdict-fit refusal behavior. Keep this as bounded structural evidence; do not introduce arbitrary regex matching/inclusion, a schema validator, a dependency, a public authoring profile, keyword bans, per-caller guards, or weaker missing-field checks.
- **Focused closure proof:** extend existing pure public-validation fixtures to accept the exact field from a JSON slot and from an action Agent output, through inline and supplied-resolver production. Include a nested exact-name selector, and a missing sibling under the literal-only closed schema that still yields `missing_field`. Retain cyclic/local-ref/finite-recursive selector and Verdict-fit cases. Reuse the existing fixture and owner tests; no provider/server/journey harness or generic schema theorem suite is warranted. The existing exact `card::blocks::validate::tests::verifier_spec_refuses_invalid_graphs_and_references` and `card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references` targets are available for extension. Run whichever targets are actually changed using exact nextest selection through mise, plus only the narrow affected owner verification. This review source-proved the new cases; it did not add or execute them.


## Minimal public case

```json
{
  "inputs": {"answer": {"type": "json", "schema": {
    "type": "object",
    "patternProperties": {"^score$": {"type": "number"}},
    "additionalProperties": false
  }}},
  "requirements": [{"id": "positive", "statement": "Score is positive",
    "check": {"kind": "assert", "path": "inputs.answer.score",
      "operator": "greater_than", "expected": 0}}]
}
```

This must validate; `{"score": 1}` is valid input under its declared schema. Current validation emits `missing_field` because it mistakes the absence of `properties.score` for absence of every declaration of score. JSON-slot and action-output reference branches share the same cause. Preserve existing reference parsing, graph dependency and visibility rules.

## Constraints and preserved behavior

- Reuse `literal_name` and the current Walk; traverse the matching declaration under its existing root/budget. No per-producer or per-check guards.
- Preserve ordinary named properties, local refs, nested selectors, known-missing refusal, unknown/budget fallback and the separate conservative Verdict-fit behavior.
- Do not weaken validation, remove additionalProperties checks, prohibit patternProperties, add a schema profile, add arbitrary regex handling/inclusion, introduce a new dependency or schema framework, or rewrite Prompt compilation.
- Keep the closed permission/catalog/provenance/Verdict/docs/recursion findings closed. No new public fields, default grants or compatibility paths.
- No engine, tool runtime, projector, sandbox, API/SDK, queue/store or final cutover work. Full legacy judge removal remains TASK-005.

## Focused execution and acceptance

1. Add the JSON-slot exact-field case to an existing public-validation test, run its exact selector and record the expected `missing_field` RED.
2. Apply the shared presence correction and rerun GREEN. Cover the same field through inline and resolver-supplied action outputs, a nested exact-name path, and a genuinely absent sibling under the closed literal-only schema. Reuse current fixture setup.
3. Retain cyclic/local-reference/finite-recursive and existing Verdict cases; refactor only when it removes real duplication. No new harness or generic schema theorem suite.

FIND-TASK-001-7 closes when all declared exact-name input/output references validate, absent siblings still fail with `missing_field`, and the existing traversal/visibility/Verdict behavior remains intact. Every actual named test must be recorded and run with its exact command; do not claim uncreated selectors were executed.

Existing focused targets, when extending those tests:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_refuses_invalid_graphs_and_references)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
```

After focused RED/GREEN, the narrow complete write-set proof is:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib
mise exec -- cargo clippy --locked -p wyrd-spec --all-features --lib --tests -- -D warnings
mise run fmt
mise run codegen:check
git diff --check
```

No dependency boundary changes or broad integration/journey runs are needed. Record actual diagnosis, focused results and final checks. Re-review the complete original-base-to-remediated-candidate against original Task001, prior verdicts, R1/R2/R3 and the stable ledger. Keep IDs1–6 CLOSED; preserve new FIND-TASK-001-7.

## Implementation Evidence

Status: `IMPLEMENTED`. Commit `a02055b55` on top of `e258ce76c`, which merges `change/deployment-addressing` into `542957a36` with no conflicts. Skald's new output-schema meta-validation accepts every Verdict and selector fixture.

### Diagnosis and correction

- **Symptom:** `inputs.answer.score` under `{"patternProperties":{"^score$":…},"additionalProperties":false}` was refused with `missing_field` (RED in `verifier_spec_bounds_cyclic_schema_references`, `requirements[0].check`).
- **Cause:** `Walk::admits` field branch consulted only `properties`, then concluded absence from `additionalProperties:false`.
- **Fix site (shared owner, both consumers):** `schema_path.rs::Walk::admits` now walks `patternProperties`. A literal-name match (the existing `literal_name`) follows into that declaration under the same root and budget. Any non-literal pattern makes the field undecidable, so it is admitted through the existing unknown fallback (no regex matching). Otherwise the prior `additionalProperties` logic applies.
- **Beyond the literal fix:** the non-literal admit closes the same false `missing_field` for patterns like `^p.*$`, which the walker's "false only when provably absent" contract already required. It adds no regex policy.

### Acceptance

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Exact-name field referenceable from a JSON slot, including nested | `schema_path.rs` `Walk::admits` | `verifier_spec_bounds_cyclic_schema_references`: `inputs.answer.score`, `inputs.answer.score.value` validate | PASS |
| Same from inline and resolver-supplied action output | same | same test: inline `steps.findings.output.score.value`; `validate_with` resolver for a registered Agent Card | PASS |
| Absent siblings still `missing_field` | same | `inputs.answer.points`, `inputs.answer.score.nope`, inline and resolver `steps.findings.output.points` | PASS |
| Cyclic/local-ref/finite-recursive, Verdict fit, graph/reference rules retained | unchanged paths | `verifier_spec_bounds_cyclic_schema_references`, `verifier_spec_requires_check_agents_to_produce_a_verdict`, `verifier_spec_refuses_invalid_graphs_and_references`, `schema_path_refuses_only_known_missing_selectors` | PASS |

### Commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_bounds_cyclic_schema_references)'   # RED then GREEN
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_refuses_invalid_graphs_and_references)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::schema_path::tests::schema_path_refuses_only_known_missing_selectors)'
mise exec -- cargo nextest run --locked -p wyrd-spec --lib   # 869 passed
mise exec -- cargo clippy --locked -p wyrd-spec --all-features --lib --tests -- -D warnings   # clean
mise run fmt; mise run codegen:check; git diff --check   # clean
```

Non-goals held: no new fields, dependency, profile, regex engine, runtime, or judge work. Findings 1–6 untouched. Your docs edit stays unstaged.

## Owner acceptance — 2026-10-15

The owner explicitly approved the completed remediation at
`a02055b5561b889fa9d934038109f3028b0ecb1d` and authorized the task-status
update. FIND-TASK-001-7 is closed by that owner acceptance; Task 001 is
complete and Tasks 002–004 are ready for parallel execution. No fresh
independent review or test run is claimed by this record. Earlier immutable
review verdicts remain unchanged.
