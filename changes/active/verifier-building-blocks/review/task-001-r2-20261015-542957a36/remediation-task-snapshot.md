---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 6
requirements: [REQ-005, REQ-007, AC-001, AC-010]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-3, FIND-TASK-001-6]
---

# Finish the bounded Verdict contract validation

## Authority and immutable review subject

Route this task to `$wyrd-implement`; do not redesign the schema contract.

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Approved spec: `changes/active/verifier-building-blocks/spec.md`, r6; captured as `spec-snapshot.md` beside this task.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-001-verifier-contract.md`; captured as `task-snapshot.md`.
- Prior remediation: `changes/active/verifier-building-blocks/review/task-001-20261015-896e563be/TASK-001-R1-contract-gaps.md`; captured here as `remediation-task-snapshot.md`.
- Original acceptance base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Previous candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Reviewed cumulative candidate / remediation starting point: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`.
- Current verdict: `verdict.md`; independently validated diagnosis/correction: `findings-validation.md`, both beside this task.

## Outcome, ownership and preserved work

Correct the remaining concrete structural Verdict-schema misclassifications at the existing shared walker, and finish the two required formatter error sections. No new fields, public schema profile or runtime machinery is needed. Findings 1, 2, 4 and 5 are source-closed; preserve their one-catalog permissions, exact scope attenuation, tool provenance and complete traversal budget.

Own the existing `wyrd-spec/src/card/blocks/schema_path.rs` structural walk, its public validation fixture in `validate.rs`, and the two cited Rust doc locations in Reference and Skald MediaFormat. Make the smallest necessary source/test edits in those owners, adapting adjacent pure fixtures only when required. Other contributors may be working concurrently; do not revert their work.

## Diagnosis and selected corrections

The following retained ledger is independently validated. It supplies the invalid-state producer, shared correction owner, all affected consumer modes and focused proof; an implementer need not rediscover the correction boundary.


### FIND-TASK-001-3 — Important — Complete the bounded Verdict structural comparison

- **Disposition / classification:** REVISED / INCORRECT; residual prior finding.
- **Discovery aliases:** BEH-R1-001, INV-R1-001, SEC-R1-001.
- **Violated obligation:** REQ-005/007 and original Task 001 Scenario 3; R1 explicitly requires incompatible field structure refusal while preserving compatible additional restrictions without a new authoring profile.
- **Exact locations:** `crates/wyrd-spec/src/card/blocks/schema_path.rs:128`, `:149`, `:187`; consumer `validate.rs:568`; typed boundary `verification/execution.rs:339`.
- **Reachable consequences:** valid check Agents using reason type+allOf refinement or finite string const/enum are refused as check_agent_schema; a declared pattern-added score field is accepted although conforming model output containing score cannot decode as Verdict. Both check positions and resolution modes share the cause. No mounted-route exploit is asserted. Runtime malformed-output handling cannot repair a declaration refused before execution and does not replace required early structural refusal.
- **Minimum selected correction:** extend the existing bounded Walk at the current check-position guard, retaining generated Verdict/Citation/Locator as the shape source. Preserve same-node structural evidence when allOf merely conjoins refinements; a sufficient sibling type/enum must not be discarded. Reuse the current finite-values analysis to establish primitive string shape from a string const/enum, without requiring redundant type. In closed-object comparison, account for ordinary exact anchored literal-name pattern properties such as `^score$` as named property declarations: an admissible score field must meet the same closed-property refusal as properties.score. An exact-name refinement of an existing field such as `^reason$` is not an extra field and must not be blanket-refused. This bounded literal-name case selects the necessary correction; it does **not** ask the implementer to decide a general regex acceptance policy or solve arbitrary regular-language/schema inclusion. Keep existing conservative behavior for analysis outside established structural evidence, and do not ban keywords or announce a new authoring profile.
- **Preserve:** action output freedom, Prompt-derived schemas/media/placeholders, tool/workspace refusals, prior incompatible outcome/reason/citation/locator/extra-field negatives, local refs/annotations, shared traversal bound, selector unknown/known-missing semantics and later malformed-output refusal. No extra durable fields, runtime executor changes, provider/schema fetch, dependency, witness generator or alternative validation platform.
- **Focused closure proof:** extend the existing public validation test with reason `{"type":"string","allOf":[{"maxLength":500}]}`, finite string const and enum positives; include numeric/mixed finite-value negatives. Add equivalent outcome enum plus redundant allOf refinement. Add root patternProperties `^score$` number negative and same-name `^reason$` maxLength refinement positive. Keep the existing canonical/local-ref and incompatible suite. Exercise the shared guard through both check positions and inline/resolver Agents using existing fixtures, retaining arbitrary structured action output. The cases need no server/provider/journey harness. Run the exact existing owner test:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
```

These new cases were proven by source trace during this read-only review, not added or executed. No broad theorem suite is required for closure.

### FIND-TASK-001-6 — Important — Document the two formatter failure contracts

- **Disposition / classification:** CONFIRMED / VIOLATION; residual prior finding.
- **Discovery aliases:** RS-R1-001, MNT-R1-001, INV-R1-002; fresh follow-up resolved the closure discrepancy.
- **Exact locations:** `crates/wyrd-spec/src/card/blocks/reference.rs:235`; `crates/skald/skald-spec/src/media.rs:304`.
- **Authority/evidence:** AGENTS.md:723-737 requires rustdoc for changed trait items and Errors on every fallible method, explicitly blocking completion. Reference fmt propagates write errors with `?`; MediaFormat fmt directly returns `write_str` failure. Both have only summaries. There is no trait-method exception and no inference that canonical formatting itself is wrong.
- **Consequence:** required changed-item documentation gate is incomplete. This is the skill's explicit rustdoc exception to non-blocking wording/style findings.
- **Minimum correction:** add an accurate Errors section to each current method documenting formatter/write failure. Keep bodies and canonical output unchanged. Batch with FIND-3; no new checker, abstraction, behavior test or separate style-only round.
- **Focused proof:** static inspection of both complete docs and normal formatting. Existing narrow lint/owner proof applies to the combined remediation; tests cannot establish missing prose.


## Constraints and non-goals

- Extend the current bounded structural guard and generated typed Verdict/Citation/Locator source. Keep Prompt-derived authoring, action-position freedom, current wrong-type/enum/citation/extra-field refusals and runtime malformed-output handling.
- Do not add generic JSON Schema or regex inclusion/proof machinery, a new dependency, a blanket keyword ban, an accepted schema profile, a registry, external schema fetch, witness generator, new public fields or API.
- The selected exact anchored literal-name pattern cases are bounded structural evidence, not authority to define an arbitrary regex policy. Preserve established behavior outside that correction and the existing shared traversal bound.
- Keep the prior permission/catalog/provenance/recursion fixes intact. Do not add default tool grants or change global/scoped permission meaning.
- Current owner direction assigns all legacy LLM-judge removal, including offline evaluation, to TASK-005. This task neither removes it prematurely nor preserves it indefinitely. The earlier R1 instruction to retain it is superseded to that extent; retain only the temporary compilation-preserving state pending the later cutover.
- No runtime tool implementation, engine projector, sandbox, routes, persistence, queue, SDK or final protocol cutover. No full integration/journey or repository aggregate run here.
- Batch the two rustdoc corrections with the behavioral fix; do not broaden into a style or legacy-doc rewrite.

## Execution and acceptance

1. Add the concrete schema cases to the existing public validation fixture, run its exact selection and record expected RED; the existing test currently misses those inputs. No provider or server is needed.
2. Correct the shared structural analysis once, preserving the same node's conjunctive facts and existing finite-value analysis. Keep local references and the traversal budget. Run the focused test GREEN and retain its prior negatives; refactor only to remove actual duplication.
3. Add the two formatter Errors sections, documenting only underlying write failures and preserving implementations.

| Finding | Required observable closure |
|---|---|
| FIND-TASK-001-3 | Compatible string reason const/enum and sibling/allOf forms validate; incompatible numeric/mixed finite sets and exact literal extra-field pattern fail; a literal pattern refining an existing field remains usable. Existing two-position/inline/resolver and action-freedom coverage remains intact. |
| FIND-TASK-001-6 | Both cited Display methods document formatter-write failure under Errors, without body/output changes. |

## Focused verification and evidence

The named existing regression target is:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
```

Record every actual newly named test and its exact selector if additional tests are needed. After focused RED/GREEN, use only the narrow owner/contract checks for the actual write set:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-skald-spec --all-features --lib --tests -- -D warnings
mise run fmt
mise run codegen:check
git diff --check
```

No dependency or crate boundary changes are planned; add the existing dependency boundary check only if the actual write set changes them. The Skald change is documentation only and needs no new behavior tests. Do not rerun server/journey/aggregate suites as this task's closure proof.

Record diagnoses, actual RED/GREEN results, selected shared owner correction and final evidence. Re-review the complete original-base-to-new-candidate together with the original task, prior verdicts, R1/R2 and stable closure ledger. Keep FIND-TASK-001-3 and -6; do not assign new IDs to their sibling callers.

## Implementation Evidence

Status: `IMPLEMENTED`. Candidate `eb1681461..542957a36` (one commit).

### Diagnosis and correction (shared owner `schema_path.rs::Walk::fits`)

- **allOf refinement refused:** authored `allOf` returned "any part fits", so the node's own `type: string` was dropped. Now the walk returns true when some part fits, and otherwise falls through to the same node's own keywords.
- **const/enum without `type` refused:** the type check read only `type`. With no `type`, it now takes the JSON types of the existing finite-value analysis (`values`), so `{"const":"fixed"}` is a string and `["short", 1]` is string + integer, which is refused.
- **Literal pattern field accepted:** `fits_object` ignored `patternProperties`. An exactly anchored literal name (`^name$`, ASCII alphanumeric/`_`) now counts as declaring that field. If it refines a declared property it adds nothing; otherwise it is compared like a property, so a closed typed object refuses an unknown name. Other patterns keep the prior conservative behavior. No regex policy was added.
- FIND-6: `# Errors` (formatter write failure) added to `Reference`'s `Display::fmt` and `MediaFormat`'s `Display::fmt`. Bodies are unchanged.

### Acceptance

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-3 | `schema_path.rs` (`fits` allOf fall-through, value-derived types, `json_type`, `literal_name`, `fits_object` pattern fields) | `card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict`: new positives are reason `allOf` maxLength, reason const, reason enum, outcome enum + redundant `allOf`, and `^reason$` maxLength; new negatives are reason const 7, reason `["short", 1]`, and `^score$` number. The prior canonical/local-ref/generated positives and 15 negatives are retained, as are both check positions and inline/resolver coverage (numeric-const negative now drives the step and resolver cases) and the arbitrary action output schema. | PASS |
| FIND-TASK-001-6 | `card/blocks/reference.rs` and `skald-spec/src/media.rs` `Display::fmt` docs | Static inspection; clippy clean | PASS |

RED: the focused test failed on the first new positive (reason `{"type":"string","allOf":[{"maxLength":500}]}` was refused as `check_agent_schema`). GREEN after the walker fix.

### Commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'   # RED then GREEN
mise exec -- cargo nextest run --locked -p wyrd-spec --lib   # 869 passed
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-skald-spec --all-features --lib --tests -- -D warnings   # clean
mise run fmt; mise run codegen:check; git diff --check   # clean
```

### Non-goals

No new fields, API, dependency, or schema profile. Permissions, the tool catalog, provenance, and the traversal budget are untouched. Judge removal stays with TASK-005. Your docs edits stay unstaged.
