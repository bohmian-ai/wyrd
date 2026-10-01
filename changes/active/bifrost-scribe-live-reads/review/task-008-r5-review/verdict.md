# TASK-008-R5 confirmation verdict

**FIX_REQUIRED. FIND-007-10, FIND-007-11 and FIND-007-12 are CLOSED. FIND-007-13 is the sole retained finding: stale active shutdown guarantees, requiring documentation-only correction. No executable durability, replay or restart-publication regression was established.**

## Immutable subject

Candidate `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`, detached HEAD; parent `ca99db0af5a0d898ef67834699405c1c73719f56`. Cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Authority: current confirmation instructions and standing maintainer decisions; approved [spec revision 20](../../spec.md); original [TASK-007](../../tasks/TASK-007-one-parquet-scan-for-live-reads.md) and [TASK-008](../../tasks/TASK-008-tenant-proven-per-file.md); prior remediation chain, [R4 verdict](../task-008-r4-review/verdict.md), [R5 task/evidence](../task-008-r4-review/TASK-008-R5-preserve-oracle-only-peer-drain-and-complete-evidence.md); repository rules and applicable architecture/references. [Subject/navigation](subject.md), [cumulative diff](cumulative.diff), [correction diff](correction.diff) and [verification](verification.md) preserve inputs.

The no-shutdown-residue-publication decision stands. FIND-007-3, excluded Postgres tenant columns, `WYRD_VALA_500_QUERY_TENANT_INVARIANT` and no live-read cap remain fixed. Capacity qualification is not reopened.

## Independent reports and reconciliation

Every discovery, follow-up and validation role used a separate fresh agent. Discovery received the same starting map without sibling conclusions or an intended verdict. All required reports are complete.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: leftover descriptions only |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL: same descriptions |
| Standards | [standards-review.md](standards-review.md) | FAIL: same descriptions |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| Tenancy/security | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Persistent data/durability | [domain-review-data.md](domain-review-data.md) | PASS |
| Concurrency/resources | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Structured Ponytail validation | [findings-validation.md](findings-validation.md) | One REVISED finding |

[Claim comparison](claim-comparison.md) required follow-up because reviewers differed on the blocking significance of stale descriptions within the correctness-only and explicitly named leftover-reference scope. Follow-up distinguished definite false shutdown sweep guarantees from valid explicit flush and normal admitted publication. Fresh validation independently confirmed that distinction, narrowed all proposals into FIND-007-13, and rejected the worker-field queue-drain wording and ambiguous publish_due phrase as standalone blockers. No runtime defect is inferred from stale prose. The adjacent tick retry hypothesis was investigated and excluded as unchanged behavior outside this correction's regression scope.

## Reconciled acceptance matrix

| Obligation | Source and attributed proof | Result |
|---|---|---|
| REQ-014 shared staged/hot/published scan, pruning, engine memory source and partitions | Existing HotParquetExec/follower/native frame owners; focused/module/journey evidence in packet | PASS |
| REQ-015 authenticated footer producers, proof before rows including cached/aggregate reads, seal-bound memory, no row column/filter/fallback | Shared footer reader and Scribe/assembly/Forge writers; focused negatives and tenant journeys | PASS |
| Remote refusal/first-refusal audit and prior closures | Aborted remains TenantInvariant; leader audit owner retained; exact remote-footer/classifier evidence | PASS |
| FIND-007-10 Oracle-only graceful runner and blocked Scribe cleanup | Listener owner branches on actual Scribe capability; original runner for Oracle-only, stopping IO for Scribe/mixed; supplied focused 3-pass and peer 11/11 | CLOSED |
| FIND-007-11 IO Errors and ConnectInfo rustdoc | All five fallible methods and associated type documented; supplied lints | CLOSED |
| FIND-007-12 exact focused command records | Exact source-confirmed package/target/selectors and results appended | CLOSED |
| Approved shutdown retains staging without forced residue sweep | Flush/drain/fsync ordering unchanged; no new deletion; deleted helper has no source caller | PASS |
| Durability, replay and restart publication | Restore/reconcile/resume before WAL replay/readiness; original ready timestamps and production tick retained; supplied redux 872/872, Scribe 20/20, server 26/26 | PASS |
| Legitimate residue operations preserved | Explicit flush retains publish_residue(Drain); normal target/dwell publication and fences unchanged | PASS |
| Current leftover descriptions match approved shutdown | Reliability reference, assembly docs and lifecycle journey prose still promise removed sweep | FAIL: FIND-007-13 |
| Fixed decisions, static scope and immutable source | No excluded decision reopened, runtime command, source edit or commit; HEAD unchanged | PASS |

## Validated finding and remediation

**FIND-007-13 — REVISED — REGRESSION (documentation).** Current `architecture/references/domain/analytical-operations-reliability.md:40–43`, `scribe/assembly.rs:437,982–985` and `wyrd-testing/tests/bifrost/scribe/lifecycle.rs:106–110,122–124,149,159–162` promise the deleted shutdown residue sweep or misstate the journey's proof. The shared cause is removal of the shutdown sweep while retaining its current descriptions. Replace or delete those false guarantees; describe explicit flush separately and preserved staging/restart/tick publication accurately. Preserve all runtime code, enum/metric values, normal admitted publication, explicit flush, executable assertions and historical evidence.

Full independently validated diagnosis, producer/caller proof and minimal closure checks: [findings-validation.md](findings-validation.md). Remediation: [TASK-008-R6-align-shutdown-residue-descriptions.md](TASK-008-R6-align-shutdown-residue-descriptions.md), routed to `$wyrd-implement`. No new product, architecture, security, concurrency or persistent-data decision is needed.

FIND-007-10/11/12 are closed. Earlier closed findings remain preserved; fixed FIND-007-3 is unchanged.

## Verification limits and immutability

Static review only. No cargo, nextest, mise, builds, tests, benchmark, source edits or commits. Runtime results are attributed supplied evidence, not freshly executed proof. Both fresh committed-range whitespace checks pass. Direct restoration of the original Oracle-only runner supplies owner-boundary proof; no fresh timing journey is claimed. Restart and idle-publication seams plus source ordering support retained recovery, without claiming a freshly run combined restart/tick scenario. The supplied 100M-row benchmark establishes shutdown cost, not restart durability.

Concluding HEAD remains `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; tracked working diff is empty. Only this review directory was written.

**Verdict: FIX_REQUIRED — FIND-007-13 only; FIND-007-10, FIND-007-11 and FIND-007-12 CLOSED.**
