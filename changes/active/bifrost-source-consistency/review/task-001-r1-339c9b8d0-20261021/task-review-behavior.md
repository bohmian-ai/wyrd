# TASK-001 cumulative behavior review

**Overall: FAIL.** FIND-TASK-001-1 is only partially closed: the query-facing holders now retain the charge, but the real persistence payload still drops that ownership while retaining the same Arrow buffers. FIND-TASK-001-2 through -5 are closed on inspected source. Original REQ-006 / AC-006 also remain failed; bounded remediation closure cannot qualify the original task.

## Subject, scope, and authority

- Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
- Candidate: `339c9b8d0484421b5fd67b71194eed8fcc60cadb`; HEAD was unchanged when checked at report preparation.
- Prior candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`.
- Authority: approved source-consistency spec revision 2; original TASK-001; prior verdict and findings-validation; TASK-001-R1 including recorded implementation evidence; actual AGENTS.md and agent-rules; reference router, spec-driven-development and maintainer-style; Wyrd doctrine and applicable Wyrd/Bifrost design and OLAP/reliability guidance.
- This is an independent caller-to-result static audit of the cumulative base-to-candidate change, using the latest fix range to locate closure. Historical reports are evidence claims, not proof. No current discovery report was consulted.

I read the cumulative implementation diff across catalog/cache, Scribe capture/publication/recovery/retirement, Oracle acquisition/follower/execution, SQL revision state, signed internal protocol, server adapters, benchmark judgments and supporting tests/docs. CodeGraph was used before source navigation. I expanded the supplied map into the production persistence conversion and completion path, including `ImmutableGeneration::from_frozen`, `frozen_snapshot`, `process_job`, `deliver_completion`, shard normal and replay completion, and sibling retirement/rollback callers. The carried planning-skill edits and disclosed draft Verifier packets do not implement additional runtime capabilities; they grant no waiver of revision 2. The separate span-permit correction retains its deadline-bounded all-permits assertion and is test-only.

## Caller-to-result assessment

An ordinary query pins the cached published cut through the existing catalog, freezes the Scribe roster, obtains each table's captures, and compares reported revisions before installing live routes and building the executable root. A report above the cut evicts the table with a cache-generation bump; classification repins under the same deadline and profile before admission/output (`oracle/mod.rs::classify_one_build`, `build_physical_root`, `list_live_routes`; `catalog/cut_cache.rs::evict`). Empty captures retain no registry entry but still report their revision. Execution assignments name the selected capture; neither the follower nor live fragment opens a new snapshot after loss.

The publishing transaction's `file_list` insert trigger advances per-tenant/table/node revisions while retaining the counter lock to commit. Replay inserts nothing, rollback rolls back its revision, and cut state is read in the same statement snapshot. `ClaimPublisher` marks the generations before attempting the commit, retains marks for an unresolved outcome, and advances local authority plus revision atomically. Startup loads node revisions before readiness. The recovery path raises a missed local revision before releasing a generation. This extends existing catalog/publication ownership instead of introducing a second publisher or group read service.

Scribe capture takes memory guards in freeze's writable-then-immutable order and keeps them until staged authority/leases are selected. Its memory and staged choices are exclusive, including a memory generation already superseded by publication. Registry removal does not invalidate an opened read's lease. Signed capture identity, authenticated query identity, stream epoch and tenant/table checks compose at the follower/open boundary. Listing-only availability loss still degrades; accepted-capture open/stream loss fails the dependent execution before or after rows.

R1's shared charge slot is appropriate reuse of `ScribeOwnership` and `ScribeMemoryLease::split`/Drop, not a second quota. However, its producer-to-consumer coverage is incomplete as described below.

## Complete acceptance matrix

PASS means the inspected source and available recorded proof support that obligation at task scope; it does not claim execution by this reviewer or waive integrated gates.

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: rows survive memory/staging/publication transitions | `LiveCapture::take`; registry `capture_staged`; Oracle pre-execution revision comparison | Exact-row source-transition journey; rollover and staged-run proofs recorded passing | PASS |
| REQ-002: acquire/discover/validate before execution, no mixed attempts | `classify_one_build` loops only before physical-root admission; rejected live routes drop | Published invalid-capture journey and distributed failure matrix | PASS |
| REQ-003: unresolved publication and recovery cannot serve contradictory ownership | `begin_publication`, `advance_all_atomic`, `ClaimPublisher::publish`/`finish_committed`, boot revision restore | Publishing-generation refusal and recovery owner tests; SQL replay/rollback proof | PASS |
| REQ-004: preserve merged verification workflows | No new group scheduler/read path; shared Oracle used by evidence reads | Late-evidence group regression recorded passing; integrated checklist retained | PASS at task scope |
| REQ-005: remove superseded fresh execution acquisition and stale race allowance | Deleted `open_live_batches`, old shard snapshot/listing selection, pre-row execution-degrade branches; corrected current docs | Cumulative diff, docs checks, distributed terminal matrix | PASS |
| REQ-006: actual final capacity qualifies | Report thresholds correct; retained sequence fails and predates runtime R1 | `../bsc-rev2/status` and reports; no new final sequence in R1 | FAIL |
| INV-001: one server-owned reader and existing ownership boundaries | Oracle/Scribe/Vala SQL extended; SDK/group behavior has no alternate read loop | Source diff; existing public journey consumers | PASS |
| INV-002: query/tenant/table/deadline/epoch authority and signed capture identity | `ScribeProviderCut.capture_id`, v10 digest, authenticated query passed to resolver, `open_capture` checks stored deadline | Authority/digest/peer tests; two new expiry proofs recorded passing | PASS |
| INV-003: appends continue, no remote/global barrier or per-append SQL | Only local capture guards and publication's existing catalog transaction | Source diff and preserved write-capacity workload | PASS |
| INV-004: committed exact ordered per-node revisions | SQL trigger/counter lock; transaction revision returned to registry advance; replay no insert | `file_list_publications_advance_their_node_revision` | PASS |
| INV-005: unresolved acquisition/lost accepted capture cannot yield complete success | Stale revisions discarded before root; `LiveFragmentRead::into_stream` fatal loss; deadline cleanup bounded | Invalid-capture journey, terminal failure matrix, stalled-release proof | PASS |
| INV-006: all physical retained sources remain charged within existing budgets/lifetime | Query-facing `RetainedArrowCharge` propagation correct, but persistence producer omits it | New retirement test omits the real persistence payload, so misses residual FIND-1 | FAIL |
| INV-007: no omitted acquisition cost, wrong answers/refusals/samples still fail | Query measurement remains client request latency; report logic retains correctness/error/sample/headroom rules | Boundary tests and retained real reports | PASS |
| INV-008: steady-state current cached cuts issue no catalog statement | Cache serve, in-memory revision comparison; explicit eviction then one repin read | Warm zero-cut-read and stale-cut exactly-one-read proofs recorded passing | PASS |
| AC-001: real-client/server exact identities, final live partition disappearance, retained sources, rollover | `published.rs` three publication timings plus `LiveCapture` | Source-transition journey and local rollover tests | PASS |
| AC-002: retry/refuse invalid selections, cleanup with budgets | Expired open/installation and release bound corrected; persistence can still retain uncharged Arrow | Authority/expiry/release proofs pass their covered cases; FIND-1 residual not exercised | FAIL |
| AC-003: late evidence reaches verdict on corrected reader | Group consumers unchanged, one read service | Recorded `grouped_executions_include_late_evidence`; integrated SDK/agent checks remain change-review work | PASS at task scope |
| AC-004: one current implementation, current docs no obsolete publication allowance | Current Bifrost/Wyrd/reference/product/diagram/generated prose and dispatcher/query-stream guidance corrected | Recorded docs generation/check and source comparison | PASS |
| AC-005: inclusive exact limits, next measurable value fails; other rules unchanged | `p99_at_most_ms` uses `>`; verification histogram bound uses `> 0.0005`; old strict checks remain `>=` | Existing report tests explicitly cover 1000/1001 µs, 500/900 µs buckets and unchanged strict p95/background rules | PASS |
| AC-006: identified final candidate passes comparable write → verification → queries, queries last | No qualified final R1 candidate; actual retained predecessor fails absolute limits | Inspected retained commit/status/hot/Parquet/direct-verification reports | FAIL |
| AC-007: warm no-read plus one retry/cut read after pre-capture publication | Cache generation eviction and one profile/retry counter | Source-transition case 2; cached one-table/join regression | PASS |
| No unavailable-writer recovery, new WAL/ledger/history scan | No such production addition | Diff | PASS |
| No global point-in-time cut or late-append promise | Per-Scribe retained selection only | Capture and discovery source | PASS |
| No public query/SDK selector/API or compatibility route | Internal capture wire extension only; unified rollout | Spec/proto/conversion diff | PASS |
| No group reread/scheduling workaround or altered accepted-loss contract | No production group scheduling change | Shared consumer path and late-evidence regression | PASS |
| Preserve Forge promotion/rewrite reconciliation and pinned-file protection | Existing published cut/leases retained; new revision evidence is additive | Existing published/cut regression evidence and source | PASS |
| Preserve useful lazy decode/projection/filtering/metadata caches | Raw capture fixed; follower still builds lazy memory/Parquet leaves; cache retained | Follower/source diff and supporting tests | PASS |
| Preserve tenancy, signed peer authority and non-blocking authorization audit | Tenant binding and digest checks retained; no synchronous audit added | SQL RLS test, peer network tests, boundary checks recorded passing | PASS |
| Preserve merged workflow, pressure, source eligibility, scheduled samples/attribution checklist | No alternate producer/read semantics; full checklist remains explicit | Focused original proof recorded; full Rust/Python/TS/MCP/HTTP integrated gates intentionally deferred to change review | PASS at task scope |
| No speculative dependency/config/runtime subsystem | Existing uuid enables v4 for capture identity; existing timer/registry/lease owners extended | Manifest/source diff; no new dependency or governor/reaper | PASS under approved behavior |
| Mandatory changed-method rustdoc from prior FIND-4 | All five cited sites now explain error and relevant cancellation behavior | Bodies inspected against their new docs; recorded lints/docs | PASS |

## Prior finding closure

| Stable finding | Assessment | Source and proof |
|---|---|---|
| FIND-TASK-001-1 | **PARTIALLY CLOSED / remains open** | Bucket/frozen/capture/read/source/stream paths now share the lease slot, and retirement detaches net-zero. `ImmutableGeneration::from_frozen` clones the physical rows but does not carry `frozen.charge`; `frozen_snapshot` manufactures a new empty slot. Real persistence jobs can still hold Arrow after the memtable's charge returns. See BEH-R1-001. |
| FIND-TASK-001-2 | CLOSED | `tail_rpc.rs:842–875` installs before checking/arming expiry; any timer now removes an already-installed entry. `RetainedCapture.deadline` and `open_capture` at 953 reject elapsed access without depending on cleanup polling. New elapsed-installation and elapsed-open tests match these boundaries; existing expiry and opened-reader lease tests remain. |
| FIND-TASK-001-3 | CLOSED | Server discovery `admit` passes original deadline to `release`; shared remote release at 421 is wrapped in `timeout_at`. `LiveCaptures::drop` passes its stored original deadline to the same owner. The real mTLS stalled-release test reaches release after a successful capture plus failed listing, then exercises detached cleanup with the same mechanism. No renewed timeout or availability-wide failure introduced. |
| FIND-TASK-001-4 | CLOSED | Server `grpc/scribe_tail.rs` listing/release docs, Arc follower resolver docs, fixture discover docs and `LiveCaptures::fmt` docs now state body-accurate errors; listing cancellation relies on release/expiry. |
| FIND-TASK-001-5 | CLOSED | Cited current OLAP/design/product/diagram/generated text and dispatcher/query-stream guidance now distinguish listing degradation from fatal accepted-capture loss, and remove publication duplicate permission. Runtime open/stream loss remains fatal. |

## Proposed finding: BEH-R1-001 — persistence still loses the shared Arrow charge

**Stable identity:** residual FIND-TASK-001-1, not a new defect ID. **Classification:** INCORRECT. **Obligation:** INV-006 / AC-002 and R1's explicit shared allocation lifetime, including persistence and sibling retirement/replay holders.

**Exact locations:** `crates/vala/vala-bifrost-redux/src/scribe/persistence.rs:528–565` (`ImmutableGeneration::from_frozen`), `:568–582` (`frozen_snapshot`, changed line 581 initializes `charge: Arc::default()`); charge return at `memtable.rs:1167–1174` (`commit_retirement`). Affected real callers: `shards.rs:1641` (`prepare_replay_state`) and `:2150` (`prepare_and_queue_generation`); retaining consumers `persistence.rs:1998–2073` (`process_job`) and `:2096–2145` (`deliver_completion`). Replay release consumers include `shards.rs::complete_replay_generation` and `complete_single_replay_generation`; normal retirement uses `release_committed_generation_memory` at 2381.

**Producer-to-result evidence:**

1. `freeze` returns a `FrozenMemtable` that shares its new charge slot with the memtable. A normal sealed generation and a replayed generation both pass that value to `ImmutableGeneration::from_frozen`.
2. That conversion copies `frozen.batches` into `ImmutableGeneration.rows` (line 557), preserving the same physical Arrow allocations, but `ImmutableGeneration` has no charge field. The local `FrozenMemtable` then drops. `frozen_snapshot` clones those rows again and creates a separate empty charge slot rather than preserving the original one.
3. `process_job` retains `Arc::clone(&job.generation)` across staging and the awaited completion delivery. `deliver_completion` also owns `job.generation` while waiting for the shard's visibility acknowledgment. Neither holder keeps the charge installed into the original generation at retirement.
4. The shard marks the generation durable and can retire it on the next mailbox/lifecycle operation, before the persistence worker is scheduled to consume its acknowledgment and drop the job. The replay path is stronger evidence: `complete_single_replay_generation` calls `release_committed_generation_memory` while it still borrows `front.generation`, and owned replay also releases at completion. Thus a real Arrow-owning persistence object exists at the point the memtable entry can be the last holder of its charge slot.
5. With no live capture left, `commit_retirement` deposits the detached lease into that original slot and drops the retired entry. The root charge returns even though `ImmutableGeneration.rows` is still resident. A new reservation can reuse those bytes before the real physical holder drops. This is the same accounting/lifetime cause FIND-1 was intended to close, not a reason to add query guards or hold WAL retirement.

**Why the new proof misses it:** `a_retired_generation_stays_charged_until_its_last_holder_drops` at `shards.rs:8040` explicitly drops the returned frozen value and creates only `RetainedGeneration` bookkeeping. Its asserted final holder is the projected capture read. It does not construct or retain the production `ImmutableGeneration` that both real seal/replay producers create. Dropping freeze clones in the earlier replay test is appropriate for that test's own frozen holder, but cannot prove the production conversion carries ownership.

**Smallest testable correction:** propagate the same existing `Arc<RetainedArrowCharge>` through the existing `ImmutableGeneration` owner and its frozen snapshots, just as the query holders do. Extend the real conversion owner rather than adding a new reservation, downstream guard, delay, quota, or second ledger. Keep normal/replay completion, WAL retirement, rollback and staging unchanged. Fresh independently decoded/test fixtures may create fresh slots; a snapshot of existing physical buffers must share its existing slot.

**Focused closure proof:** retain an actual `ImmutableGeneration::from_frozen` value and its frozen snapshot across the existing owner retirement fixture; remove the memtable generation and release query holders. The shared root must still count the exact original bytes and refuse growth into them until the last real persistence/snapshot holder drops; then it returns those bytes once. Cover the same conversion used by normal seal and replay. This reviewer did not run that planned test and does not claim a selector exists. Existing query-holder regression stays relevant but is insufficient alone.

## Original qualification failure (separate from bounded R1 findings)

I inspected `../bsc-rev2/{commit,status}`, verification report and both standard query-tier reports. They identify predecessor `05186c1e443096e6c33d6b5f08831b613fa2caa2`, with write exit 0, verification exit 1, hot-query exit 1 and Parquet-query exit 1. Hot one-client selective p99 is **2.352 ms**, Parquet **2.745 ms**, against **≤1.000 ms**. The 64-client direct verification point has 435,378 observations, zero errors, and an overhead p99 bucket bound of **0.9 ms**, which cannot prove **≤0.5 ms**. Lower-client direct verification points pass. Selective throughput exceeds 1,200 QPS; that separate pass does not repair latency failure.

These reports correctly fail. Baseline-relative improvement is not approved absolute acceptance, and R1 changes runtime retention/cleanup after this measured predecessor. No new final capacity qualification is recorded. REQ-006 / AC-006 therefore fail independently of the remaining FIND-1 issue. I prescribe no speculative optimization from headline results. The approved final build still needs diagnosis, affected correctness proof and a comparable full write → verification → hot/Parquet query sequence, with queries last. This is required existing work, not evidence that a specification revision is necessary.

## Verification and limits

Recorded R1 RED/GREEN evidence covers the retired query-holder charge, elapsed installation/open and mTLS stalled release, followed by owner coverage (including 79 shard/memtable/source tests), format, lint and authored/generated docs checks. Their assertions support the source paths they actually exercise. The temporary RED-source overlap diagnosis is consistent with the recorded failure location and unchanged GREEN test; no weakened assertion or timer was added for it. Original exact-row, invalid-capture, SQL, late-group-evidence, report and cut-cache proof is recorded in TASK-001. Full first-class SDK/agent/contract gates remain change-review obligations under the spec.

No source was edited, no Cargo/test/benchmark commands were run, no agents were spawned and no commits were made. Only this report was written. Static evidence identifies the persistence ownership omission; it does not measure its duration, memory overshoot, or the final candidate's performance.

Skipped: executing tests, capacity reruns and integrated client/agent gates, as required for parallel review.
Risk: physical persistence buffers can still outlive their root charge; original absolute capacity qualification remains failed.
