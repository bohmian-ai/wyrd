# TASK-008-R4 confirmation verdict

**FIX_REQUIRED. FIND-007-7, FIND-007-8 and FIND-007-9 are CLOSED. FIND-007-10 is REVISED and remains open; new findings are FIND-007-11 and FIND-007-12.**

## Immutable subject and authority

Candidate: `ca99db0af5a0d898ef67834699405c1c73719f56`, detached HEAD. Correction parent: `2f188cb6185061a43db36122aad68b5e253308d1`. Cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`; TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Reviewed approved [spec revision 20](../../spec.md), original [TASK-007](../../tasks/TASK-007-one-parquet-scan-for-live-reads.md) and [TASK-008](../../tasks/TASK-008-tenant-proven-per-file.md), prior review/remediation chain, [R3 verdict](../task-008-r3-review/verdict.md) and [TASK-008-R4](../task-008-r3-review/TASK-008-R4-preserve-remote-refusals-and-close-peer-shutdown.md), cumulative candidate, repository rules and applicable architecture/references. [Subject/navigation](subject.md), [cumulative diff](cumulative.diff), [correction diff](correction.diff) and [verification](verification.md) preserve review inputs.

Standing maintainer decisions remain unchanged: FIND-007-3; excluded Postgres data_tenant_id columns; `WYRD_VALA_500_QUERY_TENANT_INVARIANT`; no live-read cap. TASK-006 intersections are included at the source, transport, harness and evidence seams; complete capacity qualification remains caller-owned.

## Independent review and reconciliation

All discovery, focused follow-up and validation roles used separate fresh agents. Each discovery reviewer received the immutable subject and same starting navigation, without sibling conclusions or an intended verdict. All required reports are complete.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: Oracle-only preservation proposal |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL: same preservation proposal |
| Repository standards | [standards-review.md](standards-review.md) | FAIL: rustdoc and focused-command evidence |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS for cleanup/settlement; scope interpretation reconciled below |
| Tenancy/security | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Persistent data/durability | [domain-review-data.md](domain-review-data.md) | PASS |
| Concurrency/resources | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS for cancellation/release; scope interpretation reconciled below |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED: Oracle-only graceful transport differs from durable cleanup |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | Three independently retained findings |

[Claim comparison](claim-comparison.md) required follow-up because the task reviewers identified a violation of expressly preserved Oracle-only connection semantics, while system/concurrency reviewers established intact stopping-process cleanup. Follow-up and validation independently traced role composition, transport-before-role cancellation, pinned tonic graceful serving, and Analytical connection-lease settlement. Durable cleanup is intact, but does not preserve the prior graceful connection interval. The validator retained that distinction and reconciled the apparent conflict. Unique standards proposals were also independently validated; reviewer agreement was not proof.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation / proof | Result |
|---|---|---|
| REQ-014 shared staged/hot/published Parquet scan, pruning/cache and memory source | Follower reuses HotParquetExec and session-partitioned MemorySourceConfig with signed residual predicate; bespoke staged decoder remains deleted; existing focused/module/journey evidence | PASS |
| Native Arrow, terminal counts, scan bytes and Scribe-local query resources | Native tally/decoder and wire-only RPC encoding retain owners; query pool, local IO and staged stream leases retain ownership | PASS |
| REQ-015 per-file identity, authenticated producer/footer and memory seal binding | Row field/filter/tripwire/codec removed; authenticated binding reaches Scribe, assembly and Forge footer writers; seal-qualified memory selection | PASS |
| Fresh/cached file proof before rows, COUNT(*) included, no fallback | Shared metadata proof before decoder construction; retained footer negatives/cache and journey coverage | PASS |
| FIND-007-9 streamed refusal retains public tenant class and first-refusal audit | Aborted stays TenantInvariant at shared stream conversion, excluded from availability omission; staged-only remote journey checks zero rows, failed typed terminal and one leader security event | CLOSED behavior; command record addressed separately |
| FIND-007-10 blocked Scribe transport cancellation and release | Accepted IO observes shutdown independently of body polling; supplied undrained-client RED/GREEN requires clean stop, producer release, failed read and Oracle release | Original cause corrected |
| FIND-007-10 Oracle-only peer/Analytical preservation | Unconditional private runner also resets query-only listener connections before role cancellation; prior graceful interval lost | FAIL: revised FIND-007-10 |
| mTLS connect info, public listener and steady-state backpressure | Exact TcpConnectInfo forwarded through tonic TLS; public task retains original graceful helper; normal IO delegates | PASS |
| Durable ACK/WAL/publication/recovery and staged holds | Existing role/lease/settlement owners retained; no socket-owned durable deletion or acknowledgement | PASS |
| FIND-007-8 authoritative cap-free description | Existing Bifrost live-tail paragraph matches shallow snapshots and query execution pool; supplied docs:check | CLOSED |
| FIND-007-7 evidence whitespace | Content preserved; both fresh committed-range whitespace checks exit zero | CLOSED |
| Required changed-item rustdoc | Five fallible IO methods lack Errors sections; ConnectInfo undocumented | FAIL: FIND-007-11 |
| Exact focused command records for named closure tests | Classifier and remote-footer RED/GREEN tests are named without their exact focused recipes/results | FAIL: FIND-007-12 |
| Prior declaration imports, v6 digest/proto reservation and absolute root | Earlier corrections preserved; no independently validated new defect | PASS |
| Fixed decisions, static-only scope and immutability | No excluded decision re-raised, runtime command, reviewed-source edit or commit | PASS |

PASS rows combine static source and attributed supplied evidence. They do not claim freshly executed runtime verification or completion of the caller-owned capacity benchmark.

## Validated finding ledger

Only independently confirmed or revised findings are retained. Full diagnosis, caller evidence, selected correction and closure proof are in [findings-validation.md](findings-validation.md).

| ID / status / class | Exact location | Consequence and smallest correction |
|---|---|---|
| FIND-007-10 / REVISED / VIOLATION and REGRESSION | app/server.rs:678 | Stopping IO reaches Oracle-only private peers despite required preservation. Select it only for existing Scribe-containing fragment listeners; keep Oracle-only on existing graceful runner. Preserve durable cleanup and process deadlines. |
| FIND-007-11 / CONFIRMED / VIOLATION | wyrd-tonic/server/mod.rs:321,334,345,361,368,374 | Required per-item error/type documentation missing. Document actual cancellation/socket errors and ConnectInfo without runtime changes. |
| FIND-007-12 / CONFIRMED / VIOLATION | R4 task Implementation evidence:92–104 | Two named focused tests lack required exact command record. Append actual commands/results; execute absent proof only after separate authorization, without inventing historical results. |

FIND-007-10 retains its stable ID because Oracle-only preservation was already part of the original validated correction and R4 acceptance. The blocked-Scribe mechanism is corrected; this does not close the whole obligation. FIND-007-7/8/9 are closed. Earlier closed findings and fixed FIND-007-3 remain unchanged.

Remediation: [TASK-008-R5-preserve-oracle-only-peer-drain-and-complete-evidence.md](TASK-008-R5-preserve-oracle-only-peer-drain-and-complete-evidence.md), routed to `$wyrd-implement`. These bounded corrections require no new product, public API, protocol, persistent-data, security or concurrency-policy decision.

## Verification and immutability

No cargo, nextest, mise, builds, tests, benchmark, reviewed-source changes or commits were performed. Supplied implementer results: fmt, lints, docs:check, Oracle journeys 42/42, server peer 11/11 and focused blocked-Scribe/pending-source RED/GREEN evidence. Fresh static whitespace checks pass for both cumulative and correction bases. Missing command records do not establish that named tests were unrun or contradict broader green behavior evidence.

Concluding HEAD is `ca99db0af5a0d898ef67834699405c1c73719f56`; tracked working diff is empty. Only this new review directory was written.

**Verdict: FIX_REQUIRED — retained FIND-007-10, FIND-007-11, FIND-007-12; closed FIND-007-7, FIND-007-8, FIND-007-9.**
