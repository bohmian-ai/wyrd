# TASK-001 acceptance verdict

**SPEC_REVISION_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`.
- Base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- The cumulative range includes first implementation commit `d917a2751`, using its parent as base, and all subsequent fixes.
- Approved specification: [spec.md](../../spec.md), revision 4.
- Original task: [TASK-001-sub-act-attribution.md](../../tasks/TASK-001-sub-act-attribution.md), including its original base version and candidate execution evidence.
- Subject/navigation map: [subject.md](subject.md).

All required independent reviewers completed their assigned reports. The reviewed tracked source and HEAD stayed unchanged; only this new review directory was written. Review did not implement, stage, commit, merge, push or deploy.

## Decision

The existing execution design largely satisfies the intended reuse boundary: Caller, the existing permission evaluator, Verifier queue, gateway, catalog, telemetry routing and Scribe outbox remain the owners. The task cannot pass under revision 4 because the approved provenance rules do not define an admissible creator for actual callerless bootstrap operations.

The implementation gives SYSTEM its own SYSTEM/SYSTEM creator pair and gives built-in table creation the unresolved-audit sentinel, including when authenticated provisioning has a real creator available. Neither convention is approved for these creation rows. Startup reconciliation, the reserved audit-owner table, defining SQL and fixtures also include creation without an authenticated caller. Forwarding available caller context fixes only the authenticated part. Selecting a subject/actor convention for the autonomous part is a persistent-data decision, so review cannot choose it as private remediation.

Revise the specification through `$wyrd-spec` to settle that narrow creation rule and any necessary trusted-source or tenant-lookup exception, preserving eager reconciliation, immutable creation attribution and existing availability. After approval, derive one coherent remediation task through `$wyrd-plan` for the retained ledger. No remediation task is written under r4 because it would not be decision complete without inventing that convention. Review makes no choice among possible bootstrap identities.

## Independent review results

| Role | Report | Discovery result |
|---|---|---|
| Behavior implementation reviewer | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariant implementation reviewer | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards reviewer | [standards-review.md](standards-review.md) | FAIL |
| Maintainer reviewer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System-resilience reviewer | [system-review.md](system-review.md) | FAIL |
| Security/RBAC domain reviewer | [domain-review-security.md](domain-review-security.md) | FAIL |
| Persistent-data/concurrency/durability domain reviewer | [domain-review-data.md](domain-review-data.md) | FAIL |
| Tenancy domain reviewer | [domain-review-tenancy.md](domain-review-tenancy.md) | FAIL |
| Focused follow-up reviewer | [followup-review.md](followup-review.md) | RESOLVED source distinctions |
| Fresh structured Ponytail validator | [findings-validation.md](findings-validation.md) | SPEC_REVISION_REQUIRED |

Discovery outcomes are proposals, not votes. The final result uses only the independently confirmed or revised ledger. [claim-comparison.md](claim-comparison.md) records grouping and why one focused follow-up was needed: autonomous creator authority, distinct unresolved-authentication audit paths, and the served-subject denial conversion. The latter was rejected after no required reachable failing producer was established. No speculative guard or extra query API is requested.

## Validated finding ledger

Full source traces, discovery IDs, obligations, exact locations, consequences, correction boundaries and focused closure proofs are preserved in [findings-validation.md](findings-validation.md).

| ID | Validation | Required outcome |
|---|---|---|
| FIND-TASK-001-1 | REVISED — INCORRECT / DRIFT | Approve autonomous creator attribution; preserve available verified creators through existing SYSTEM and built-in provisioning owners. This is the specification-revision boundary. |
| FIND-TASK-001-2 | CONFIRMED — MISSING | Carry the known registered local Workflow Card through existing shared-client gateway correlation; keep null application run and current Card authorization. |
| FIND-TASK-001-3 | CONFIRMED — MISSING | Retain required ordinary HTTP authentication refusals using the existing unresolved pair and nonblocking audit path, with trusted storage/correlation and unchanged rejection responses. |
| FIND-TASK-001-4 | CONFIRMED — MISSING | Make a missing observation-binding owner refusal observable at acceptance, retaining healthy sibling processing and no fallback execution. |
| FIND-TASK-001-5 | CONFIRMED — INCORRECT | Preserve verified actor and history in API-key issuance completion audit instead of reconstructing A/A. |
| FIND-TASK-001-6 | CONFIRMED — MISSING | Preserve accepted delegation history through the shared Verifier SYSTEM input-read context without changing its narrow grants. |
| FIND-TASK-001-7 | REVISED — MISSING proof | Prove further delegation, actual reloaded-authority enforcement/expiry, supported scheduled execution and identity refusals; obtain missing mandated docs/examples/served-contract checks through existing lanes. |
| FIND-TASK-001-8 | CONFIRMED — VIOLATION | Complete mandatory rustdoc on enumerated changed items; keep code/assertions unchanged and add no permanent scanner. |
| FIND-TASK-001-9 | CONFIRMED — INCORRECT | Correct direct-versus-queued result actor and peer attribution documentation to match the implemented contract. |

The proof correction does not authorize manual queued Eval, scheduled Eval, a new public API, a crash harness or broad task-remediation suites. Use supported origins and the existing persisted-authority/protected-gateway seam. No permission escalation or cross-tenant data bypass was established by this review; accountability defects and required acceptance gaps independently prevent a pass.

## Reconciled acceptance matrix

The following matrix is reproduced from independent validation, including its distinction between source defects and required-proof gaps.

PASS means inspected implementation plus credible reported evidence supports the obligation; it is not an independent runtime attestation. FAIL includes explicitly required proof gaps and partial failures. F1–F9 refer to the stable findings above.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 non-null pair | Defining SQL, Attribution, managed_arrays and audit content retain non-null fields; provenance assessed separately | V4/V5/V9, fresh SQL reported | PASS |
| REQ-002 correct work identities | Caller/accepted execution correct generally; bootstrap creators and issuance completion/refusals incomplete | V1/V2/V6/V8; F1/F3/F5 counterexamples | FAIL |
| REQ-003 subject authority/cost; never SYSTEM sub | Gateway retains effective Principal and ledger subject; SYSTEM creation violates final invariant | V1 asymmetric denials/V2; F1 | FAIL |
| REQ-004 Card roles and Workflow Card | Results/run/bindings use correct roles; local registered Workflow drops Card | Hosted V8 versus local V8; F2 | FAIL |
| REQ-005 full schema/writer inventory | Columns/ordinary updates correct; creator provenance wrong in inventory owners | SQL/codegen plus F1 | FAIL |
| REQ-006 wire/SDK/docs/reserved names | Wire and SDK fields/reservation aligned; public actor documentation wrong | V5/V9/codegen; F9 | FAIL |
| REQ-007 active SYSTEM | Production and fixture provisioning create active tokenless SYSTEM | V2 checks seeded row before fixture provision | PASS |
| REQ-008 deferred authority/history/refusal | Stored credential-free snapshot and claim preserved; read history and missing-owner visibility fail | V3 storage proof; F4/F6/F7 | FAIL |
| REQ-009 replay scope | Existing registration/manual locks/lookups/uniqueness include pair; Workflow/background keys retained | V4/owner tests reported | PASS |
| REQ-010 audit payload/history/storage | Transport/content separation correct; refusals and two history projections incomplete | Existing platform/JWT/MCP evidence; F3/F5/F6 | FAIL |
| REQ-011 complete propagation/further narrowing | Direct/queued gateway retains Principal; required further-hop negative not exercised | V1 single-hop only; F7 | FAIL |
| INV-001 trusted allowed sources | Verified Caller/Gate correct; autonomous catalog uses unauthorised sentinel convention | F1 source trace | FAIL |
| INV-002 SYSTEM actor only | Execution pair correct; SYSTEM self-creation counterexample | F1 | FAIL |
| INV-003 tenant isolation | TenantConn/RLS, same-tenant owner recover/SYSTEM lookup remain | SQL queue isolation and tenant check reported; AC-017 proof gap separate | PASS |
| INV-004 origin/Card meanings | Field roles correct generally; registered Workflow call loses required Card | F2 | FAIL |
| INV-005 trace never authority | Trusted operation binding/rebind; no trace-derived attribution | Telemetry subtree test/V7/V10/V11 | PASS |
| INV-006 attribution never grants | Gateway Principal authorization; separate scoped internal reads | V1/V2 negatives and source | PASS |
| INV-007 immutable creators/snapshot | Updates/conflicts/claim/retry do not replace accepted values | V3 JSON equality/V4; wrong original provenance separate | PASS |
| AC-001 scheduled pair/results | Scheduler capture/runner source present | V2 observation only, no required scheduled journey; F7 | FAIL |
| AC-002 observation owner over writer | Observation sink/recovered owner/Eval gateway | V2 permitted owner and denied owner, different writer | PASS |
| AC-003 direct rows/audit | Caller/result/gateway and language row projections | V1/V5/V6/V7/V9/V10 | PASS |
| AC-004 delegated gateway/audit | Last verified actor, direct shared gateway capture | V1/V5/V6/V9 | PASS |
| AC-005 reject reserved names | Queue preflight/Gate/Scribe reservations | V5/V9 each SDK | PASS |
| AC-006 old attribution cleanup | Source-backed inventory; remaining identity/owner keys distinct | Disposition and codegen reported | PASS |
| AC-007 no new migration/fresh DB | Defining declarations changed; specified incremental file removed | test:sql reported 203 passes | PASS |
| AC-008 fixture active SYSTEM | PgFixture seed uses existing provisioning | V2 pre-provision assertion | PASS |
| AC-009 required scoped verification | Named commands credible but explicit branches and mandatory lanes absent | F7 | FAIL |
| AC-010 request/judge spans and joins | Trusted route rebind; Card role joins | V7/V10 direct, V11 delegated, telemetry unit | PASS |
| AC-011 Workflow pair/Card/null Run | Hosted path correct; local known Card omitted | F2 | FAIL |
| AC-012 actor-denied realtime judge | Retained narrowed Principal reaches gateway | V1 refusal/no extra provider call | PASS |
| AC-013 reloaded authority denied model/expiry | Queue persists snapshot; Eval consumer uses it | V3 scripts bypass protected call and no expiry; F7 | FAIL |
| AC-014 identity-specific replay | Pair-scoped existing SQL/locks/conflict semantics | V4/unchanged owner tests reported | PASS |
| AC-015 platform/unresolved/nonblocking audit | Existing payload separation/platform correct; ordinary invalid/absent auth disappears | F3; V6 expects omission | FAIL |
| AC-016 all writers/read projections | Managed ingress correct; creator and audit projections incomplete | V5/V9 and F1/F5/F6 | FAIL |
| AC-017 observable identity refusal/frozen authority/narrow reads | Tenant resolution and scoped reads correct; missing owner silent; negative proof incomplete | F4/F7; V2 suspended-owner subset | FAIL |
| AC-018 every hop and further delegation | Direct propagation source and single-hop tests correct | Required further-hop refusal absent; F7 | FAIL |

Constraints/non-goals assessed separately: existing Caller, queue, issuer recovery, gateway evaluator, catalog, outbox, telemetry route and language client owners are reused. AcceptedAuthority earns existence as durable credential-free operation authority; BackgroundAuthority retains the existing issuer rather than duplicating resolution. No new permission algebra, token/delegation issuance contract, role, dependency, configuration knob, queue/store, audit transport, compatibility alias, schema-evolution path or durable Workflow lifecycle was found. Variant policy, subject sampling algorithm, operational lineage, Operator rechecks, background deduplication and Workflow subject-based ownership remain within scope. F1 identifies the material exception to approved-authority compliance.

## Verification limits and prior closure

The candidate task records V1–V11, owner tests, SQL (203 tests), static checks, and first-class language checks as passing. Review inspected test source and recorded evidence; it did not rerun runtime, compilation, codegen, package or deployment lanes. Those reported passes cannot prove omitted branches: V3's scripted engines bypass the protected consumer, V6 expects ordinary authentication-refusal omission, and local V8 omits Workflow Card assertions. Missing docs/example/served OpenAPI checks are unrecorded proof, not known failures.

The orchestrator independently ran `git diff --check b22718f2c 518a5d8b8` successfully and repeatedly checked immutable HEAD and tracked-source cleanliness. Review artifact whitespace and required-report presence were also checked before delivery.

This is the first review of sub-act TASK-001. No prior findings of this task require closure or preserved numbering. The separate verify-attribution prerequisite's reports/remediation and integrated code were available as dependency context; its findings are not retroactively assigned to this task. Final change review still owns cumulative integration and broad journey proof.

**Final verdict: SPEC_REVISION_REQUIRED.** No remediation task under revision 4, implementation edit or completion approval is issued.

Skipped runtime reruns; this is a static acceptance audit supported by reported verification.
Risk: the autonomous creator convention needs approval, and findings 2–9 remain acceptance blockers.
