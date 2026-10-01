# TASK-008-R3 confirmation verdict

**FIX_REQUIRED. FIND-007-4 is CLOSED.** Retained findings: **FIND-007-7, FIND-007-8, FIND-007-9, FIND-007-10**.

## Immutable subject and authority

Candidate: `2f188cb6185061a43db36122aad68b5e253308d1`, detached HEAD. Correction parent: `9c3d7ecb982435919924dfa8e6930352b27a9b7e`. Cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`. TASK-008 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Approved authority: [spec revision 20](../../spec.md), [TASK-007](../../tasks/TASK-007-one-parquet-scan-for-live-reads.md), [TASK-008](../../tasks/TASK-008-tenant-proven-per-file.md), prior review/remediation packets including [previous final verdict](../task-008-final-review/verdict.md) and [R3](../task-008-final-review/TASK-008-R3-close-residual-declaration-imports.md), repository rules and applicable architecture/references. The current user explicitly approves cap deletion and the follow-on corrections. [Subject/navigation](subject.md), [cumulative diff](cumulative.diff), [correction diff](final.diff) and [verification evidence](verification.md) preserve inputs.

FIND-007-3 remains unchanged; Postgres tenant columns are excluded; the selected error is `WYRD_VALA_500_QUERY_TENANT_INVARIANT`; there is no live-read batch or byte cap. Nothing in this verdict challenges those decisions. TASK-006 is included at the scan, transport, path, evidence and client seams; complete capacity qualification remains caller-owned.

## Independent review and follow-up

All discovery roles, focused follow-up and final validation used separate fresh agents. Discovery agents received the same navigation map and immutable inputs without sibling discovery reports or an intended verdict. All required reports are complete.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL; cap prose and remote streamed tenant refusal |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL; cap prose |
| Repository standards | [standards-review.md](standards-review.md) | FAIL; whitespace and cap prose |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL; cap prose |
| System resilience | [system-review.md](system-review.md) | FAIL; backpressured shutdown |
| Tenancy/security | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS in discovered paths; remote classification claim narrowed by follow-up |
| Persistent data/durability | [domain-review-data.md](domain-review-data.md) | PASS |
| Concurrency/resource lifecycle | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL; backpressured shutdown |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED both remote-stream uncertainties |
| Fresh Ponytail validation | [findings-validation.md](findings-validation.md) | Four retained findings; FIND-007-4 closed |

[Claim comparison](claim-comparison.md) triggered follow-up because a unique remote-stream classification finding conflicted with blanket preservation claims, and shutdown required tracing an unpolled transport body and its connection owner. Follow-up traced actual Wyrd consumers plus locally available pinned tonic/Hyper source. The fresh validator independently confirmed the paths and revised the shutdown recommendation to preserve sibling listener and authentication behavior. Agreement alone was not used as proof.

## Reconciled acceptance matrix

Detailed source locations and proof assessments are in discovery reports and the validated ledger.

| Obligation | Reconciled implementation / verification evidence | Result |
|---|---|---|
| REQ-014: one shared staged/hot/published Parquet scan and pruning | Follower composes HotParquetExec; custom decoder/per-batch compile deleted; retained selective-pruning proof | PASS |
| Engine memory source, signed pushdown and session partitions | MemorySourceConfig and common signed residual filter; session-sized leaf/follower; retained shape/projection proofs | PASS |
| Native Arrow without IPC/hash; terminal counts and scan bytes retained | Native tally/decoder reconcile; gRPC-only wire encoding; retained completion/scanned-byte tests | PASS |
| Scribe-local IO, query pool and staged leases | Authenticated resolver local IO; existing follower pool; scan/stream-held lease; retained lease/drop/overlap proofs | PASS |
| REQ-015: remove tenant row/filter/tripwire/codec | Schema/envelope/write/projection deletion and seal-qualified memory selection; retained absence/closure tests | PASS |
| Authenticated footer production through assembly and Forge | Existing binding reaches writer identity and both rewrite callers; retained footer/promotion/recovery proofs | PASS |
| Tenant proof before file rows, cache/COUNT(*) included | Shared loaders prove fresh/cached metadata before decoder; retained negative tests | PASS for proof owner |
| Query fails with tenant-invariant class and audit across remote live stream | Lazy footer refusal becomes Aborted, then streamed classifier converts it to Unavailable, permitting pre-row degradation | FAIL — FIND-007-9 |
| R3 declaration imports | IcebergError and DataFusionError imported at module top, used bare; exact types/bodies/callers unchanged | PASS — FIND-007-4 CLOSED |
| Approved cap deletion and digest/proto parity | No executable cap; deleted tags/names reserved; v6 digest and conversion agree; supplied contract/module/journey results | PASS |
| Deleted-cap documentation closure | Active architecture still claims byte/batch enforcement | FAIL — FIND-007-8 |
| Absolute root and recovery | Shared prepare resolves once before managed children/lock; recovered runs join validated file names to prepared root; supplied relative-root RED/GREEN | PASS |
| Shutdown while producer is pending | Token select can emit Unavailable when body is polled; supplied lost-Scribe journey 12.5 s | PASS for that path |
| Shutdown while response is transport-backpressured | Hyper capacity wait precedes body poll; graceful detached connections retain body/fragment and can exhaust server drain budget | FAIL — FIND-007-10 |
| ACK/WAL/publication/tenant and sibling owners | Data/root traces preserve durable authority, fences, local identity and recovery; no additional retained defect | PASS |
| Harness/client URL intersections | Retained production peer/server paths and shared defaults/contracts; supplied earlier journeys | PASS within scoped review |
| Required whitespace check | New TASK-006 line 82 fails cumulative and correction-range checks | FAIL — FIND-007-7 recurrence |
| Non-goals, immutable subject, static-only execution | Fixed maintainer decisions honored; no runtime execution, source edits or commit | PASS |

PASS rows use static source plus attributed supplied evidence. They do not assert completion of AC-016's caller-owned benchmark or claim fresh runtime verification.

## Validated finding ledger

Only independently CONFIRMED or REVISED findings are retained; the full decision-complete ledger is [findings-validation.md](findings-validation.md).

| ID / status / class | Exact location | Consequence and selected correction |
|---|---|---|
| FIND-007-7 / CONFIRMED / VIOLATION | TASK-006 evidence Markdown:82 | Two spaces fail required Git whitespace check. Content-preserving removal only; earlier R2 sites remain closed. |
| FIND-007-8 / CONFIRMED / INCORRECT | architecture/bifrost-design.md:311–312 | Current authority promises deleted live snapshot byte/batch caps. Update the existing paragraph to approved shallow snapshots and query execution governance; no cap restoration. |
| FIND-007-9 / CONFIRMED / INCORRECT | oracle/dispatcher.rs:1809–1815, stream_status_error | Remote lazy tenant footer refusal becomes availability loss; before rows the query may succeed degraded and miss tenant-refusal audit. Preserve Aborted as existing TenantInvariant at the shared conversion, leaving other stream classes unchanged. |
| FIND-007-10 / REVISED / INCORRECT | server oracle/peer_service.rs:572–601; correction owner app/server.rs:665–695 | HTTP/2 backpressure can prevent body cancellation and retain fragment/leases during shutdown. Use existing token at private fragment-serving accepted-IO lifecycle so connection termination is independent of body polling; preserve TCP/TLS connect info, public listeners, Oracle-only peers, durable settlement and normal backpressure. |

FIND-007-4 is independently closed. Earlier R2 normalization remains closed for its original files, but FIND-007-7's whitespace obligation recurs at a new site. FIND-007-8/9/10 are new findings; no prior finding is silently renumbered.

Remediation: [TASK-008-R4-preserve-remote-refusals-and-close-peer-shutdown.md](TASK-008-R4-preserve-remote-refusals-and-close-peer-shutdown.md), routed to `$wyrd-implement`. No new product, public API, protocol, concurrency policy or persistent-data decision is required by the selected bounded corrections.

## Verification limits and immutability

No cargo, nextest, mise, builds, tests, benchmarks, source edits or commits were performed. Existing supplied evidence includes spec 897/897, conversions 14/14, scoped redux 105/105, distributed journeys 7/7, relative-root RED/GREEN, lost-Scribe journey 12.5 s, and claimed codegen/fmt/lints success. The current static whitespace result conflicts with the clean-check claim. Existing footer/open-status and pending-producer proofs do not exercise the two retained remote-stream failure paths.

Concluding HEAD is `2f188cb6185061a43db36122aad68b5e253308d1`; tracked working diff remains empty. Only review artifacts were written. The prior untracked final-review packet was preserved.

**Verdict: FIX_REQUIRED — FIND-007-4 CLOSED; FIND-007-7, FIND-007-8, FIND-007-9, FIND-007-10 remain.**
