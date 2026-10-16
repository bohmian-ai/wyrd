# TASK-001 cumulative R1 re-review verdict

**FIX_REQUIRED** — retain **FIND-TASK-001-3** and **FIND-TASK-001-6**. Findings **1, 2, 4 and 5 are closed**. No new finding ID or specification revision is required.

## Immutable subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Original dependency base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Previous candidate: `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.
- Reviewed cumulative candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`.
- Approved r6 spec and original task: [spec-snapshot.md](spec-snapshot.md), [task-snapshot.md](task-snapshot.md), captured from the canonical active packet. Source files still match these snapshots.
- Prior [verdict](../task-001-20261015-896e563be/verdict.md), [validated findings](../task-001-20261015-896e563be/findings-validation.md), and [R1 task](../task-001-20261015-896e563be/TASK-001-R1-contract-gaps.md); R1 including implementation evidence is captured as [remediation-task-snapshot.md](remediation-task-snapshot.md).
- Complete acceptance diff: [cumulative.diff](cumulative.diff), 26 source files; latest fixes: [remediation.diff](remediation.diff), five commits and 16 files. The latest fixes locate changed owners; acceptance covered the full original-base-to-candidate state.
- [subject.md](subject.md) records inputs/navigation/proof. Candidate/production source stayed immutable; uncommitted docsite/planning edits were excluded. No source, task, spec or earlier review report was edited.

The owner assigns legacy LLM-judge removal everywhere, including offline evaluation, to Task 005. Its temporary presence is accepted for compilation until that cutover. The earlier R1 wording cannot preserve it indefinitely. No immediate removal, new retention promise, or retroactive finding is imposed here. The new executor seam uses the approved BlockError. Scenario simulation itself is not blanket-removed.

## Independent reviews

Every discovery role used a separate fresh agent with the same immutable subject/map and independently expanded source coverage. They did not consume another current discovery report or the conversational implementation summary. The orchestrator compared claims and packaged independently validated results; it did not serve as a reviewer or validate its own findings.

| Role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL; residual prior 3 |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL; residual prior 3/6 |
| Repository standards | [standards-review.md](standards-review.md) | FAIL; residual prior 6 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL; residual prior 6 |
| System resilience | [system-review.md](system-review.md) | PASS; no additional findings |
| Security/RBAC and contract trust boundaries | [domain-review-security.md](domain-review-security.md) | FAIL; residual prior 3 |
| Fresh focused follow-up | [followup-review.md](followup-review.md) | RESOLVED; prior 6 remains partially open |
| Fresh Ponytail independent validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; prior 3/6 retained, four prior findings closed |

Security/RBAC is the sensitive changed domain; its reviewer traced grants through role decoding, coverage/intersection, accepted authority and sibling gateway callers. System review established actual live versus staged changes and failure/recovery boundaries. No new persistent writer, database transaction, queue, sandbox or worker exists in this task, so a separate persistent-data or concurrency specialist was not required.

## Claim comparison and focused follow-up

[claims-comparison.md](claims-comparison.md) groups the concrete schema cases at the shared structural-fit owner. The allOf/finite-value false refusals and pattern-added extra-field false acceptance are residuals of prior finding 3, not new per-caller findings. The system PASS is consistent with these required pure-contract defects because runtime activation remains deferred.

A follow-up was required because behavior claimed full rustdoc closure while standards/maintainer/invariant cited two missing formatter error sections. The fresh follow-up read the full methods and exact AGENTS requirement, resolving this against complete closure; the final validator independently rechecked it. No unresolved discovery conflict remains.

Independent validation narrowed the pattern correction to exact anchored literal-name structural evidence, retained known-compatible same-name refinements, and rejected a generic regex/schema inclusion engine, blanket keyword bans, exact schema-text matching, and a new public authoring profile. Looser numeric bounds/string format or pattern-strength analysis was not promoted to a new finding. No unapproved schema redesign is prescribed.

## Reconciled cumulative acceptance

The complete implementation matrices remain in both implementation reports. This reconciled matrix refers to Task 001's pure contracts and staging handoff, not future live capability delivery.

| Obligation | Evidence | Result |
|---|---|---|
| REQ-001: reusable inputs/steps/requirements/decide and global ID namespace | Typed blocks, requirement_owners, authoring/graph tests | PASS |
| REQ-002: typed slots, defaults, optional/bounded declarations and input wire | InputSlot/SlotName/InputDeclaration and declaration tests; request-time value validation remains Task 005 | PASS |
| REQ-003: reference grammar, dependency inference, visibility, known/unknown fields | Reference/Validator/shared planner/visitor; same bounded Walk covers branch/ref cycles; public input/output recursion tests | PASS |
| REQ-004: three actions and nine locked extraction shapes | ExtractOutput and field/null/ordering contracts; schema and serialization tests | PASS |
| REQ-005/007: five checks and Prompt-derived schema/position/media behavior | Existing compiler and shared check guard; compatible allOf/finite string shapes rejected, literal pattern-added field admitted | FAIL — FIND-TASK-001-3 |
| REQ-006: decisions and honest skipped/error/missing outcomes representable | Decide/RequirementOutcome/StepStatus and result tests; aggregation runtime remains Task 002 | PASS |
| REQ-008: complete catalog and tool/workspace position rules | One ten-name BuiltinTool, from_name membership, retained filesystem/workspace/check constraints | PASS — prior 2 closed |
| REQ-008/INV-003: general global or exact scope permissions | Tool resource/scope, strict validation, generic scope coverage/intersection and role/AcceptedAuthority encoding | PASS — prior 1 closed |
| REQ-008/015: tool-output provenance | One ToolCallRecord with typed digest/count/error semantics used by ActionOutput and StepResult; pure handoff/result round-trip | PASS — prior 4 closed |
| REQ-009: original-source citations/derived provenance | Citation/Locator and CallInput.sources; runtime existence/authorization checks remain Task 003 | PASS |
| REQ-010: canonical formats/media compatibility | Shared MediaFormat/Prompt media-kind and tests; content sniffing remains Task 003 | PASS |
| REQ-011–013: Run/Window, request/status/result, identity and delayed conflict | Typed messages/ExecutionId/status/error reuse and tests; no synchronous reservation added | PASS |
| REQ-014: scheduled input mapping and drift reuse | ScheduledInput, validate_schedule and existing DriftSpec; runtime fitting/scheduling remains later work | PASS |
| INV-001/006/008: purity, honest evidence, staged replacement | Pure foundation, Vala async seams, original live Spec/routes remain until Task 005; offline compilation preserved | PASS |
| Existing-owner reuse/consumer handoff | Existing DAG/operator/visitor/Prompt/permission owners extended; no duplicate loader/store/runtime or default tool grant | PASS except residual schema compatibility |
| Cumulative mandatory rustdoc | Original omissions largely repaired; two Display methods still lack required Errors sections | FAIL — FIND-TASK-001-6 |
| AC-001/009/010 and required proof | 1139 libraries/9 focused/codegen/cumulative whitespace pass; concrete new schema cases remain uncovered | FAIL pending retained ledger |
| Non-goals and integration boundary | No new Card kind, plugin platform, routes, sandbox, projector, queue, storage or SDK transport; gateway match is minimum scope adaptation | PASS |

## Prior-finding closure and final ledger

| Stable ID | Source-confirmed closure |
|---|---|
| FIND-TASK-001-1 | CLOSED: typed tool scope/resource, strict decoding, existing coverage/intersection and accepted-authority preservation. |
| FIND-TASK-001-2 | CLOSED: all ten names share one catalog across permission/declaration/provenance; unknown/check/workspace refusals retained. |
| FIND-TASK-001-3 | OPEN / REVISED / INCORRECT: sibling allOf facts are discarded; finite string const/enum lacks inferred shape; literal patternProperties can add an incompatible field. |
| FIND-TASK-001-4 | CLOSED: typed output digest and returned bytes survive the existing handoff/result record, including honest no-output semantics. |
| FIND-TASK-001-5 | CLOSED: complete traversal budget covers recursion and ref hops across both schema consumers, preserving known-missing/unknown behavior. |
| FIND-TASK-001-6 | OPEN / CONFIRMED / VIOLATION: Reference::fmt at `crates/wyrd-spec/src/card/blocks/reference.rs:235` and MediaFormat::fmt at `crates/skald/skald-spec/src/media.rs:304` lack mandatory formatter-write Errors sections. |

For finding 3, exact shared owner locations are `card/blocks/schema_path.rs:128,149,187`, reached through `validate.rs:568` after ordinary Prompt syntax validation. The consequence is wrong public declaration acceptance/refusal in both check positions and resolution modes. The selected correction stays in the bounded owner, preserving generated typed schemas, action freedom, existing negative checks, original selector behavior and later typed decoding.

For finding 6, source confirms actual write failures are returned, and AGENTS explicitly requires error documentation on every fallible changed method, including trait methods. This is the review skill's rustdoc exception; no runtime defect or formatting-output error is asserted.

Complete producer/sibling-consumer traces, exact counterexamples, rejected proposals, selected minimum corrections and focused proof are in [findings-validation.md](findings-validation.md). Only independently confirmed/revised findings appear here. No new stable IDs or Critical/current remote exploit claims are introduced.

## Verification and limits

Fresh orchestrator checks at the pinned candidate:

```bash
mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime -p wyrd-skald-spec -p vala-eval --lib
# 1139 passed, zero skipped: library-check.log

mise run codegen:check
# Passed, tracked outputs unchanged: codegen-check.log

git diff --check 08b38f74255d344e1c5ec9a6d278935a26277622 eb1681461897ee1eb9fe1fa4e00cb0b435658559
# Passed
```

The nine exact remediation selections also passed; [focused-check.log](focused-check.log) records selected test identities/results. Reviewer reports include their additional exact focused commands and successful results. Existing passing cases do not cover the newly confirmed schema examples. All-features Clippy/workspace check, fmt and dependency-check successes remain recorded implementation evidence rather than fresh orchestrator reruns. No red lane is hidden, weakened or excused.

No live server/provider, sandbox fault, SQL integration or SDK journey was run. They remain outside this bounded task and require later integrated proof. This review does not certify runtime authorization, execution isolation, queue/recovery or full judge cutover.

## Remediation disposition

Route the single self-contained [TASK-001-R2-verdict-schema-closure.md](TASK-001-R2-verdict-schema-closure.md) to `$wyrd-implement`. It selects the concrete structural corrections and batches the two documentation sections, preserving the four closed findings. It requires focused owner/contract checks, not server/journey/aggregate reruns.

Original R1 wording about indefinite judge preservation is superseded by current owner direction; removal remains Task 005 and is not an implementation defect in this candidate. Source, spec and tasks remain unchanged by this review. No implementation, commit, merge, push or deployment occurred. Task 001 has not passed its completion gate.
