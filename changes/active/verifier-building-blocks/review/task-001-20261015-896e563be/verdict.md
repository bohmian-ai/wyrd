# TASK-001 acceptance verdict

**FIX_REQUIRED**

Six independently confirmed or revised findings remain. They are bounded corrections under approved revision 6; no specification revision is required. Current library and generated-output checks pass, but they do not cover the missing contracts and validation cases.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Dependency base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Cumulative candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`, including the explicitly approved BlockError replacement on the new executor seams.
- Approved specification: `changes/active/verifier-building-blocks/spec.md`, revision 6, captured in [spec-snapshot.md](spec-snapshot.md).
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-001-verifier-contract.md`, captured in [task-snapshot.md](task-snapshot.md).
- Actual complete base-to-candidate diff: [cumulative.diff](cumulative.diff). Dependency changes merged before this base are not Task 001 implementation.
- Subject, map and proof: [subject.md](subject.md). Production source and candidate remained unchanged throughout review. Uncommitted docsite/planning edits were excluded. Spec/task snapshots match their original files.

## Independent review results

Each role ran in a separate fresh agent context. Discovery reviewers received the same subject/map and independently expanded source coverage without other reviewers' conclusions. The orchestrator did not act as a discovery reviewer or validate its own findings.

| Role | Report | Result |
| --- | --- | --- |
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL; BEH-001–004 proposed |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL; INV-001–005 proposed |
| Repository standards | [standards-review.md](standards-review.md) | FAIL; RS-001 proposed |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL; MNT-001 proposed |
| System resilience | [system-review.md](system-review.md) | PASS; no findings |
| Security/RBAC and contract trust boundaries | [domain-review-security.md](domain-review-security.md) | FAIL; SEC-001–002 proposed |
| Fresh independent Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; six deduplicated findings |

The review covered the cumulative 21-file implementation, governing repository/design/spec authority, existing alternatives and consumer paths. Security/RBAC was the sensitive domain requiring a separate reviewer. No SQL, durable writer, deployment topology or concurrency implementation changed; the system reviewer established actual staged versus live effects and checked shared planner/operator regression boundaries.

## Follow-up and reconciliation

[claims-comparison.md](claims-comparison.md) groups proposals by violated behavior. A focused follow-up was not required: no unresolved contradictory source trace or missing root-cause boundary remained. Specific cyclic-schema evidence narrows the behavior report's generic schema-path PASS; the invariant/security traces agree on that concrete path. System PASS establishes unchanged deployed behavior and does not waive required pure-contract correctness.

The fresh validator traced each proposal independently, consolidated duplicates, revised the Verdict-schema correction to bounded structural compatibility, and limited provenance proof to the staged value handoff. A generic JSON Schema inclusion/proof engine, exact-schema-text equality, new public schema profile and deferred runtime projector are not authorized corrections. Style-only preferences were rejected as blockers. Mandatory rustdoc remains blocking under the explicit repository rule.

Older statements that codegen was not run are superseded by fresh successful `codegen:check`; no generated drift is established and no missing-codegen finding remains.

## Reconciled acceptance matrix

The implementation reviewers' complete matrices remain in their reports. This matrix reconciles task-scoped obligations against the independently validated ledger. PASS means the bounded contract obligation passes, not that the future server capability is live.

| Obligation | Implementation / proof | Result |
| --- | --- | --- |
| REQ-001: subject-less inputs/steps/requirements/decide and global IDs | `card/blocks/mod.rs`, duplicate-ID validation, authoring tests | PASS |
| REQ-002: typed JSON/media/trace/samples slots and defaults | `InputSlot`, SlotName, positive-bound and declaration tests; request-time input-value validation remains Task 005 | PASS |
| REQ-003: reference grammar, dependency inference, scope, cycles and known/unknown fields | Reference parser and shared DAG/visitor pass; schema combinator/ref cycles do not terminate | FAIL — FIND-TASK-001-5 |
| REQ-004: three actions and locked extraction outputs | Typed nine-format union, required/null fields, selected schema and serialization tests | PASS |
| REQ-005/007: five checks, Prompt-derived position/schema rules | Check Agent tools/workspace and media/placeholder checks pass; required-name-only schema test admits incompatible Verdict fields | FAIL — FIND-TASK-001-3 |
| REQ-006: decisions, thresholds and honest outcomes representable | Decide validation, requirement/step/error distinctions; aggregation execution belongs to Task 002 | PASS |
| REQ-008: approved built-in declaration catalog | Six-entry private list rejects four of the ten approved names | FAIL — FIND-TASK-001-2 |
| REQ-008 / Scenario 5 / INV-003: global or exact scope permissions | Existing PermissionScope/Resource owners unchanged; neither approved grant form representable | FAIL — FIND-TASK-001-1 |
| REQ-008/015: complete tool provenance | Existing shared ToolCallRecord cannot carry output digest or returned byte count | FAIL — FIND-TASK-001-4 |
| REQ-009: original-source citation contract and propagation | Citation/Locator, source-slot sets and action/check handoff; runtime existence validation belongs to Task 003 | PASS |
| REQ-010: canonical media, aliases and distinct legacy Office formats | MediaFormat and media-kind accessor; media tests pass; content sniffing belongs to Task 003 | PASS |
| REQ-011/012: Run/Window and execution request/status/result distinctions | Typed declarations/status/results, UUIDv7 execution ID, mixed-mode refusal | PASS |
| REQ-013: delayed idempotency conflict without synchronous reservation | Existing status/error reuse and conflict conversion test; queue mechanics remain Task 005 | PASS |
| REQ-014: explicit scheduled mappings and existing drift reuse | ScheduledInput, validate_schedule and drift contract tests | PASS |
| INV-001/006/008: pure foundation, honest evidence and staged protocol | No foundation IO/async/PyO3; old envelope/routes stay live until Task 005; offline scenarios retained | PASS |
| Reuse and consumer closure | Existing planner/operator/visitor/Prompt owners extended; no duplicate runtime or Card loader; narrow injected Vala seams | PASS within implemented scope |
| Complete parallel handoff | Tool permissions and result metadata incomplete | FAIL — FIND-TASK-001-1/4 |
| AC-001/009/010 and repository verification | Library, exact selections, diff and codegen checks pass; required negative cases are absent and rustdoc incomplete | FAIL — validated ledger |
| Mandatory changed-item rustdoc | Private/trait/test items and fallible validation helpers lack required docs/error sections | FAIL — FIND-TASK-001-6 |
| Non-goals and cutover boundary | No new Card kind, nested Verifier, plugin platform, routes, sandbox, runtime loop, store or compatibility surface | PASS |

## Validated finding ledger

The final ledger and complete producer/consumer traces, exact locations, evidence, selected corrections and focused closure proof are authoritative in [findings-validation.md](findings-validation.md). Only independently confirmed/revised findings are retained here.

| ID | Disposition / classification | Consequence and correction boundary |
| --- | --- | --- |
| FIND-TASK-001-1 | CONFIRMED / MISSING | Approved global/scoped tool grants cannot decode or survive existing authority machinery. Extend existing typed permission/coverage/intersection owners; share one ten-name vocabulary. |
| FIND-TASK-001-2 | CONFIRMED / INCORRECT | `cards.list`, `cards.get_by_ref`, `bifrost.list_tables`, `bifrost.describe_table` are refused. Use the same complete catalog for declaration validation. |
| FIND-TASK-001-3 | REVISED / INCORRECT | Check schemas with wrong outcome/reason/citation structures pass. Enforce bounded Verdict-specific structural compatibility at the existing position guard, preserving shared Prompt derivation and arbitrary action outputs. |
| FIND-TASK-001-4 | REVISED / MISSING | The handoff/result cannot retain required tool-output digest and returned bytes. Complete the existing record with honest success/no-output semantics and pure serialization/handoff proof. |
| FIND-TASK-001-5 | CONFIRMED / INCORRECT | A finite schema using a combinator and local self-reference can exhaust the validation stack. Bound the complete shared schema walk; indeterminate/cyclic analysis remains unknown/admitted. |
| FIND-TASK-001-6 | CONFIRMED / VIOLATION | Required changed-item rustdoc is incomplete. Document affected private/trait/test items and actual error conditions in the same correction pass. |

No Critical finding or currently exposed remote exploit is asserted. The public pure validation API is an explicitly required surface even though its eventual server route is staged.

## Verification and limits

Fresh review proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-skald-spec -p vala-eval --lib
# 1069 passed, zero skipped; library-check.log

mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-skald-spec -p vala-eval --lib \
  -E 'test(=blocks::tests::action_executor_receives_resolved_inputs) | test(=card::blocks::validate::tests::verifier_spec_enforces_agent_positions_and_media) | test(=refs::blocks_tests::blocks_verifier_yields_every_reference_slot)'
# 3 passed, 1066 skipped; focused-check.log

mise run codegen:check
# Passed, tracked outputs unchanged; codegen-check.log

git diff --check 08b38f74255d344e1c5ec9a6d278935a26277622 896e563bee69ab6b5f47b31977d7de1a0ca88d57
# Passed
```

The invariant reviewer also ran three exact focused tests and records their command/result. Implementer-reported Clippy, workspace compile, fmt and dependency checks were assessed for scope but not rerun during this review. Passing existing tests does not exercise omitted permission/schema/provenance cases. No process-aborting stack reproduction, live provider, sandbox, server integration or SDK journey was executed. Full integration/journeys remain deferred to the merged five-task change as approved.

## Prior closure and disposition

This is the first Task 001 review: no prior FIND IDs, remediation task or closure claims exist. The earlier plan-readiness finding is not a task-review finding. The approved JudgeError replacement is accepted owner direction and is not retroactively assigned a finding ID.

Route the single self-contained [TASK-001-R1-contract-gaps.md](TASK-001-R1-contract-gaps.md) to `$wyrd-implement`. It packages all six findings, combines the shared catalog correction and batches documentation work. It requires focused owner/contract proof, not broad integration or journey reruns. This review implements, commits, merges, pushes and deploys nothing. Task 001 has not passed its completion gate.
