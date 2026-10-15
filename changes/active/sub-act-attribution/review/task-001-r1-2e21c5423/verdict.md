# TASK-001 / R1 cumulative acceptance verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/sub-act-attribution`.
- Original base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Candidate: `2e21c5423036460d5da2ca22a51efde63635e352`.
- Prior implementation candidate: `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.
- Complete acceptance range: original base → candidate, including initial implementation and R1. The R1 locator diff from `69687f648` did not replace this range.
- Approved spec: [spec.md](../../spec.md), revision 6, approval commit `785d8293e44afcac5e94be15210026c4855650a6`.
- Original task: [TASK-001-sub-act-attribution.md](../../tasks/TASK-001-sub-act-attribution.md).
- R1: [TASK-001-R1-close-attribution-gaps.md](../task-001-518a5d8b8/TASK-001-R1-close-attribution-gaps.md).
- Prior review: [verdict.md](../task-001-518a5d8b8/verdict.md), [findings-validation.md](../task-001-518a5d8b8/findings-validation.md).
- Current subject/map and comparison: [subject.md](subject.md), [claim-comparison.md](claim-comparison.md).

The candidate and tracked source remained unchanged throughout review. Only this new review directory was written. Review did not implement, stage, commit, merge, push, deploy or complete the change.

## Result and prior closure

R1 closes the validated runtime and required-proof gaps. Independent validation closes FIND-TASK-001-1/2/4/5/6/7 with current source and inspected test assertions. FIND-TASK-001-3 is superseded by approved r5: ordinary absent/malformed/unverifiable bearer refusals remain unaudited; existing exchange-refusal auditing remains. Approved r6 governs the local Workflow Card and unchanged Card-scope refusal; the allowed fixture changes agree with that approval.

Two existing findings remain partially open. They require bounded documentation corrections under r6; no further specification decision, runtime implementation, permission change or new finding ID is needed. No runtime escalation or cross-tenant access defect was established. Mandatory rustdoc remains an explicit blocking repository rule, and the stale security/row descriptions remain an incomplete approved contract outcome.

## Independent review results

| Role | Report | Discovery result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: residual FIND-9 |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | PASS |
| Repository standards | [standards-review.md](standards-review.md) | FAIL: residual FIND-8 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL: residual FIND-8 |
| System resilience | [system-review.md](system-review.md) | PASS |
| Security/RBAC | [domain-review-security.md](domain-review-security.md) | FAIL: residual FIND-9 |
| Persistent data/concurrency/durability | [domain-review-data.md](domain-review-data.md) | PASS |
| Tenancy | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | Retains FIND-8 and FIND-9 only |

All required reports are complete. Discovery is not a vote. One focused follow-up was necessary because reports differed on FIND-9 closure/severity: correction of the Wyrd/Bifrost passages did not establish closure of the explicitly tasked security authority. The final validator independently traced the conflicting claims and all prior closures. It rejected optional import-style/ephemeral-reference notes as independent blockers and retained no new runtime finding or speculative scheduled-query guard.

## Validated ledger and remediation

| Stable finding | Validation | Remaining correction |
|---|---|---|
| FIND-TASK-001-8 | CONFIRMED; prior locations corrected, five additional genuinely changed items remain | Add required rustdoc to two audit schema/layout methods, the new authority-reload test and inner constant, and the extended provisioning test. Preserve bodies/assertions. |
| FIND-TASK-001-9 | REVISED; public result/Bifrost text corrected, required authority reconciliation incomplete | Correct security-posture capture/peer, obsolete result/read-token and Workflow-only snapshot descriptions, plus the old managed principal_id telemetry sentence in Wyrd design. Preserve runtime behavior. |

Exact locations, discovery source IDs, complete source traces, consequences, rejected proposals and minimum correction/proof are in [findings-validation.md](findings-validation.md). No new FIND IDs were assigned. Closed prior items remain source-backed rather than inherited from an implementation claim.

One self-contained remediation task batches both findings:
[TASK-001-R2-close-attribution-documentation.md](TASK-001-R2-close-attribution-documentation.md).
Route it directly to `$wyrd-implement`. It requests static comparison, Rust formatting and whitespace proof, with no new test, runtime change, broad suite or permanent check.

## Reconciled acceptance matrix

The matrix below is reproduced from independent validation. It distinguishes source-backed product/proof closure from remaining documentation correctness; reported green execution does not prove semantic prose or the per-item rustdoc gate.

Each row is a complete obligation, including its negative conditions in the approved spec; related evidence is shared rather than replaced by a green label. PASS means current source plus credible **reported** focused execution; FAIL identifies retained incomplete documentation. The mandatory Rust documentation gate is additional to these product obligations.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 non-null server pair throughout inventory | Defining SQL, Attribution, Gate/frame, managed declaration/arrays, audit content | V4/V5/V9; fresh SQL; P1/P2 | PASS |
| REQ-002 correct direct/delegated/background/judge/Workflow/creation identities | Caller/RunAuthority/direct and queued results, Workflow and creator propagation correct; security capture text remains false | V1/V2/V6/P1–P8; FIND-9 | FAIL |
| REQ-003 accepted authority/intersection, subject cost, no SYSTEM subject/grant; narrow reads | Effective Principal retained into GatewayInvocation/Ledger; separate SYSTEM exact table grants | V1/V2/P7/P10; shared unchanged-grant test | PASS |
| REQ-004 Card roles and local/hosted Workflow Card/null Run with unchanged scope | Result/binding/queue role fields; retained root UID and existing gateway Card check | P3/P15, hosted counterpart; timeline joins | PASS |
| REQ-005 named schemas, inventory/policy categories and immutable creation | Defining SQL, insert/read/update projections, audit None policy, sampling subject | V4/V5/V9, SQL/codegen; P1/P2 | PASS |
| REQ-006 wire/SDK/docs, reserved pair and no aliases | Typed wire/constants/projections/reservations agree; required architecture prose does not | V5/V9, codegen/P12; FIND-9 | FAIL |
| REQ-007 active tokenless SYSTEM and approved creator/lifecycle | Existing provisioning SQL/wrapper/fixtures; operator versus sentinel; no new bootstrap dependency | V2 seeded-before-fixture assertion; P1/P2 | PASS |
| REQ-008 frozen credential-free tenant/run authority, retry/restart/history, refusal and lifetime | Queue capture/claim; owner resolver; observation diagnostic; Caller restore; read context | P4/P6/P8/P9/P10 and serialization source | PASS |
| REQ-009 pair-scoped replay without authority replacement; existing background/Workflow keys | Same pair in registration unique/lookups and manual lock/lookup/index; lifecycle updates omit authority | V4; P9; existing manual route replay proof | PASS |
| REQ-010 verified audit/history/platform storage/sentinel, r5 omission and nonblocking path | Credential/read audit fixes; payload/content versus transport; ordinary pre-auth path unchanged | V6/P5/P6/P11; existing exchange/platform/outbox tests | PASS |
| REQ-011 realtime every-hop identity/authority and further narrowing | Direct/Eval trusted Caller handed into existing gateway; no historical-chain permission reconstruction | V1/P7/P10, trusted-context negative source | PASS |
| INV-001 trusted sources and approved no-caller exceptions | Verified Caller/Gate; same-tenant recovered owner; explicit creator sentinels | Reserved-name journeys/P1/P2/P4 | PASS |
| INV-002 SYSTEM actor only, Wyrd execution only | Creator pair no longer self; executor override doesn't replace subject | V1/V2/P1/P2/P6/P8 | PASS |
| INV-003 unchanged tenant isolation | TenantConn/RLS; owner/SYSTEM recovered within tenant; peer explicit tenant | SQL isolation/static checks; P4 foreign owner refusal | PASS |
| INV-004 origin links retain meanings | Separate principal pair/Card roles, queue source/binding keys, Workflow null application Run | V4/P3/P8; timelines | PASS |
| INV-005 trace observability only | Trusted operation bind/rebind; no trace-parent identity derivation | Telemetry subtree unit, V7/V10/V11 | PASS |
| INV-006 pair never permission/resource/Card grant | Gateway evaluates Principal; requested Card check unchanged; served history attribution only | P3/P7/P10 and unchanged input-grant test | PASS |
| INV-007 immutable creator/accepted authority | Conflict/update branches retain first pair; retry/claim/replay never assign new authority | V3/V4/P1/P2/P9/P10 | PASS |
| AC-001 scheduled owner/SYSTEM run and result (judge where supported) | Scheduler shared resolver → queue → real Drift runner/result | P8 real supported schedule/read/result | PASS |
| AC-002 observation owner over writer, denied model and owner spend | Owner acceptance, accepted Eval Caller, subject ledger | V2/P4 distinct writer/allowed-denied owners, one upstream request | PASS |
| AC-003 direct gateway/verify rows and audit across SDKs | Shared Caller/gateway/result/audit projected through language clients | V1/V5/V6/V7/V9/V10 and affected realtime stories | PASS |
| AC-004 delegated A/B gateway/audit | Last verified actor through capture/audit | V1/V5/V6/V9, local/hosted Workflow | PASS |
| AC-005 reserved sub/act refused in every SDK | Shared preflight and server decode reject supplied fields | V5/V9 real managed-row/refusal stories | PASS |
| AC-006 all old live attribution names removed/classified | Schema/wire/SDK old names absent; identity keys classified; generic telemetry still names old managed principal_id | Static inventory/codegen plus FIND-9 at wyrd-design:135 | FAIL |
| AC-007 no added migration, specified file deleted, empty DB | Defining migrations changed, trace context folded into defining queue | Reported test:sql/P2 fresh migration | PASS |
| AC-008 dev fixture active SYSTEM | seed_tenant uses canonical SYSTEM wrapper | V2 seeded row before fixture provisioning; P2 | PASS |
| AC-009 narrow actual-write-set proof incl SDK/schema/tenancy/deps | Original and R1 scoped selectors/check records; no aggregate used as substitute | V1–V11/P1–P15; docs/OpenAPI/examples additions recorded | PASS |
| AC-010 request/judge span pairs and result/Card joins | Existing operation-route registry and shared result payload | V7/V10 direct, V11 delegated/MCP; telemetry unit | PASS |
| AC-011 hosted/local direct/delegated Workflow pair/Card/null Run; scope refusal | Shared loaded UID and existing server Card check | P3 exact pair/Card/null Run and excluded scope/no upstream; hosted/P15 | PASS |
| AC-012 actor denied model prevents realtime judge | Captured intersection reaches gateway | V1/P7 asymmetry, no extra upstream | PASS |
| AC-013 durable retry/restart/expiry cannot widen accepted authority | Stored authority claim → Caller::accepted → actual gateway | P9 fresh runtime/retry plus P10 actual claim after observed expiry/grant edit | PASS |
| AC-014 pair-separated exact replay/conflict and preserved origin/Workflow semantics | Existing unique/lock/lookup owners; background keys and Workflow ownership retained | V4 and existing replay/concurrency tests; P9 immutable snapshot | PASS |
| AC-015 platform payload/sentinel exchange/r5 bearer omission/nonblocking failures | Existing audit producers and outbox; no new preidentity event | V6/P11; platform/JWT/MCP/outbox recorded proof | PASS |
| AC-016 all writers/read projections incl ingest/span; subject sampling/Variant unchanged | Gate/Scribe arrays/peer, SQL creators and audit producers; existing policy sampler | V5/V9/V7/V10/V11; P1/P2/P5/P6; owner tests | PASS |
| AC-017 observable absent/inactive/foreign owner or SYSTEM refusal, frozen authority, narrow reads/rechecks | Shared resolver/observation warning, scheduler refusal, read context; Operator rechecks retained | P4/P6/P8/P9/P10; unchanged input grants/serialization | PASS |
| AC-018 realtime subject every hop/both asymmetries/further narrowing/no ambient fallback | Direct Task/Eval Caller to protected gateway; existing invoker missing-context refusal | V1/P7 four permitted upstream, negatives; trusted-context proof/P10 | PASS |
| AC-019 platform operator creators, callerless sentinel, immutable first pair | Provisioning → canonical SYSTEM/builtin/catalog; autonomous entries explicit | P1 persisted operator/sentinel/restart/reserved owner and P2 backfill/seed/retry | PASS |

REQ-002/006 failures concern documentation agreement, not newly found runtime identity faults. AC-009 records credible required execution evidence, not semantic correctness of all prose. FIND-8 remains a separate hard repository gate regardless of product-row results.

## Verification limits

Review inspected the original V1–V11 and R1 P1–P15 recorded results, diagnoses, exact selectors and actual test assertions. No Cargo, database, SDK, provider, codegen, docsite or runtime lane was rerun during this review. Reported focused/static passes are supporting evidence, not independently reproduced results.

The queue retry proof uses fresh runtime owners and stored authority, not an OS-process kill. Its complementary claim-to-real-gateway proof observes initiating-token expiry, changed grants and refusal without provider traffic. Shared read-context and serialization proof covers the corrected producer without claiming every live peer/Eval combination. These limits do not justify a new runtime mechanism or broadened remediation suites; integrated broad proof remains change-review work.

The orchestrator ran the cumulative whitespace check and repeatedly checked immutable HEAD and tracked-source cleanliness. Required report presence, both 37-obligation matrices, artifact links, finding identities and artifact whitespace were checked before delivery.

**Final verdict: FIX_REQUIRED — FIND-TASK-001-8 and FIND-TASK-001-9.**

Skipped runtime reruns; this is an independent static acceptance audit supported by recorded execution.
Risk: mandatory Rust documentation and contradictory security/row authority prose still prevent complete task acceptance.
