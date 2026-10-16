# TASK-001 cumulative R2 verdict

**FIX_REQUIRED** — **FIND-TASK-001-7** is independently confirmed. **Prior findings 1–6 are CLOSED.** No specification revision or currently exposed remote exploit is established.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Previous candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`.
- Cumulative candidate: `542957a3622461e259aee5f8108d978e18adad1c`.
- Approved r6 spec and original Task001: [spec-snapshot.md](spec-snapshot.md), [task-snapshot.md](task-snapshot.md), captured from their canonical active-packet paths in [subject.md](subject.md).
- Full acceptance diff: [cumulative.diff](cumulative.diff), 26 source files. Latest fix locator: [remediation.diff](remediation.diff), one commit/four files. Review covered the complete original-base-to-candidate state.
- Latest remediation and evidence: [remediation-task-snapshot.md](remediation-task-snapshot.md), captured from [R2](../task-001-r1-20261015-eb1681461/TASK-001-R2-verdict-schema-closure.md).
- Previous [R1-review verdict](../task-001-r1-20261015-eb1681461/verdict.md) and [ledger](../task-001-r1-20261015-eb1681461/findings-validation.md); initial [verdict](../task-001-20261015-896e563be/verdict.md), [ledger](../task-001-20261015-896e563be/findings-validation.md) and [R1 remediation](../task-001-20261015-896e563be/TASK-001-R1-contract-gaps.md) were included.

Candidate and production source remained unchanged throughout review. Original spec/task snapshots still match their files; existing docsite/planning edits were excluded. This review changed only its own artifacts. Full legacy judge removal, including offline evaluation, remains owner-assigned Task005 work; temporary compilation-preserving retention is accepted, without indefinite preservation. New seams retain BlockError.

## Required independent review results

Each discovery role used a separate fresh agent, the same pinned subject/map, and independently expanded source coverage. Discovery roles did not consume another current review's conclusions or the conversational completion summary. The orchestrator did not act as a reviewer or validate its own findings.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | PASS; prior six closed; no proposal |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL; prior six closed; INV-R2-001 proposed |
| Repository standards | [standards-review.md](standards-review.md) | PASS; cumulative authority/rustdoc coverage |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS; no material proposal |
| System resilience | [system-review.md](system-review.md) | PASS; no additional deployed-path regression |
| Security/RBAC and trust boundaries | [domain-review-security.md](domain-review-security.md) | PASS within bounded staged contracts |
| Fresh independent Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; new FIND-7 confirmed, prior six closed |

The sensitive-domain review traced permission validation/intersection, effective/accepted authority, catalog and sibling gateway consumers. System review established actual live versus staged paths and failure/recovery boundaries. No database transaction, persistent writer, queue, runtime worker or deployment/concurrency owner changed; their separate domain audits are not required for this subject.

## Comparison and follow-up decision

[claims-comparison.md](claims-comparison.md) groups claims by required behavior. There is no source disagreement about the R2 closure or formatter documentation. The invariant reviewer supplied one complete concrete selector trace; other reports' generic selector PASS did not trace a successful result for that counterexample. The final validator independently confirmed both producers, complete owner and all sibling consumers, and narrowed the generic PASS accordingly.

No focused discovery follow-up was required: the new path was fully traced, no contrary concrete trace or missing root-cause boundary remained, and the repeated schema work shares an already identified owner. Independent validation confirmed this decision. A unique finding is retained because of its source evidence, not because of reviewer agreement. Optional style notes and schema/regex profile expansion were excluded.

## Reconciled cumulative acceptance

Complete matrices are preserved in both implementation reports. PASS here means the Task001 pure-contract/handoff obligation, not future hosted capability delivery.

| Obligation | Source and proof | Result |
|---|---|---|
| REQ-001: subject-less authoring and global block IDs | Typed blocks/defaults, requirement_owners and graph/round-trip tests | PASS |
| REQ-002: typed named slots, required/optional/bounded declarations | InputSlot/SlotName/InputDeclaration and tests; request-time value enforcement remains Task005 | PASS |
| REQ-003: reference grammar, dependencies, scope and known/unknown fields | Parser/planner/visibility/bounded recursion pass; exact literal-pattern field declaration is ignored by `admits` | FAIL — FIND-TASK-001-7 |
| REQ-004: three actions and nine locked extraction types | Closed action/output types, required nullable fields, schemas/locators/round-trips | PASS |
| REQ-005/007: five checks and derived Verdict schema | Shared bounded typed-schema guard now retains sibling facts, finite-value types and exact literal pattern field compatibility; specified R2 fixtures pass in existing modes | PASS — prior3 closed |
| REQ-006: decisions and honest missing/error/skip representation | Decide/RequirementOutcome/StepStatus and result/error types/tests; aggregation runtime remains Task002 | PASS |
| REQ-008/INV-003: complete tools and general global/scoped permissions | One ten-name catalog, strict typed scope/resource validation, existing coverage/intersection/authority codecs, no implicit domain/default grants | PASS — prior1/2 closed |
| REQ-008/015: tool output provenance | Shared typed record retains optional digest/count/error through action/result handoff | PASS — prior4 closed |
| REQ-009: original-slot citations/source propagation | Citation/Locator and CallInput.sources; runtime existence checks remain Task003 | PASS |
| REQ-010: canonical media and compatibility | Shared MediaFormat/Prompt kind metadata and format/binding tests; content sniffing later | PASS |
| REQ-011–013: Run/Window, wire/status/result/identity/delayed conflict | Typed declarations and stable status/error/ID reuse; queue implementation deferred | PASS |
| REQ-014: explicit scheduled inputs and drift reuse | ScheduledInput/validate_schedule/DriftSpec and tests; actual async fitting/scheduling later | PASS |
| Prior recursion safety | One complete Walk budget covers branches/ref hops across input/output consumers, preserving conservative unknowns | PASS — prior5 closed |
| Required cumulative rustdoc | Both formatter Errors sections repaired; standards/maintainer inspect private/trait/helper/test items including R2 helpers | PASS — prior6 closed |
| Purity, ownership, reuse and staged cutover | Existing DAG/operator/Prompt/visitor/auth owners extended; async only Vala seam; no duplicate loader/runtime/store or dual live protocol | PASS |
| AC-001/009/010 and bounded evidence | Owner/focused/lint/codegen proof green; exact selector case lacks coverage and fails required semantics | FAIL only for FIND-7 |
| Non-goals | No new Card kind, plugin framework, runtime executor, projector, routes/store/queue/sandbox/SDK or compatibility surface | PASS |

## Prior finding closure

All closures were independently rechecked against current source, not accepted solely from prior reports.

| Stable ID | Closure evidence |
|---|---|
| FIND-TASK-001-1 | CLOSED: Tool resource/exact scope plus All, strict wire validation, ordinary coverage/intersection, role/effective/accepted authority preservation and disjoint gateway/data reach. |
| FIND-TASK-001-2 | CLOSED: one complete BuiltinTool vocabulary shared by validation, scope and provenance, with filesystem/workspace/check refusals intact. |
| FIND-TASK-001-3 | CLOSED within approved bounded comparison: generated typed schema remains authoritative; selected allOf/const/enum/exact-name pattern cases and previous negatives pass. |
| FIND-TASK-001-4 | CLOSED: typed returned-output digest and byte count have honest no-output semantics in the shared handoff/result record. |
| FIND-TASK-001-5 | CLOSED: recursive schema and local-ref work retain a finite shared budget through both producers/consumers. |
| FIND-TASK-001-6 | CLOSED: accurate formatter Errors docs and other required cumulative changed-item rustdoc are present. |

FIND-7 is a distinct REQ-003 field-reference obligation. It does not reopen bounded Verdict inclusion or recursion findings and does not authorize arbitrary regex/schema handling.

## Final validated finding ledger

**FIND-TASK-001-7 — CONFIRMED / INCORRECT / Important.** Discovery INV-R2-001.

- Exact source: `crates/wyrd-spec/src/card/blocks/schema_path.rs:93–94`, `Walk::admits`; shared consumers `validate.rs:692,738`.
- A schema declaring `patternProperties: {"^score$": {"type":"number"}}` with `additionalProperties:false` permits `score`, yet the walker sees no `properties.score` and reports `missing_field` without consulting that declaration.
- Public `VerifierSpec::validate()` therefore rejects valid JSON-input and preparation-output references. Both inline and resolver-supplied action outputs reach the same owner after ordinary Prompt syntax validation. No future server/provider execution is needed to reach the defect.
- Minimal selected correction: reuse existing `literal_name` in the shared field-presence decision before concluding absence, then traverse remaining selectors under the same root/budget. Preserve named-property/local-ref/nested/missing/unknown behavior and distinct Verdict-fit rules. No generic regex engine, schema profile, new dependency or repeated caller guards.
- Closure proof: pure public validation for JSON input and inline/resolved Agent output, nested exact-name path, and an absent sibling that still fails; retain recursion/local-ref/Verdict regressions. No server/journey suite required.

The complete root-to-consumer trace, exact JSON reproduction, alternatives considered, dispositions and decision-complete recommendation are in [findings-validation.md](findings-validation.md). No other finding is retained; rejected optional proposals are not presented as advice.

## Verification and limits

Fresh orchestrator proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict)'
# 1 passed: focused-check.log
mise exec -- cargo nextest run --locked -p wyrd-spec --lib
# 869 passed, zero skipped: library-check.log
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-skald-spec --all-features --lib --tests -- -D warnings
# Passed: clippy-check.log
mise run codegen:check
# Passed, tracked generated outputs unchanged: codegen-check.log
git diff --check 08b38f74255d344e1c5ec9a6d278935a26277622 542957a3622461e259aee5f8108d978e18adad1c
# Passed
```

The invariant reviewer independently ran 1139/1139 four-library tests and three exact schema targets; security ran six exact owner tests, with commands/results in their reports. Existing passing tests omit the new selector case; its failure is source-proven, not claimed as a newly executed regression. Implementation fmt/dependency/workspace proof remains recorded evidence; no baseline blame exercise, gate weakening, source mutation or live crash reproduction occurred.

Server/provider/SQL/sandbox fault tests, SDK journeys, final judge cutover and full integrated runtime proof remain assigned to later tasks. This review does not certify hosted authorization, cancellation, isolation or durable recovery.

## Disposition

Route [TASK-001-R3-schema-reference-presence.md](TASK-001-R3-schema-reference-presence.md) to `$wyrd-implement`. It packages only the confirmed new shared-owner correction, preserves all six closed findings and calls for focused owner/contract proof. It does not require a specification revision, general schema redesign or broad journey rerun.

No implementation, commit, merge, push or deployment was performed. Task001 has not passed its completion gate.
