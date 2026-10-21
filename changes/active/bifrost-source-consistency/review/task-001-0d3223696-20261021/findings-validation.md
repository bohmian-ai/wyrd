# Independent findings validation — TASK-001

Recommended verdict: **FIX_REQUIRED**. Five bounded implementation/documentation findings survive independent source validation. Separately, **REQ-006 / AC-006 fails actual performance qualification**; closing these five findings cannot establish PASS while that obligation remains failed. The performance evidence does not establish that any approved behavior or expensive-to-reverse decision must change, so it does not by itself justify `SPEC_REVISION_REQUIRED`.

## Subject, independence, and coverage

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`.
- Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
- Candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`.
- Approved authority: `changes/active/bifrost-source-consistency/spec.md`, revision 2; original task: `changes/active/bifrost-source-consistency/tasks/TASK-001-source-consistency.md`.
- No prior task findings or remediation IDs exist. This fresh validation agent did not implement the change or participate in discovery. It read all eight discovery reports and `claims-comparison.md`, then checked claims against actual source and the cumulative range. CodeGraph was used before ordinary source navigation. No source changes, builds, tests, benchmark reruns, or subagents were used.
- Applicable authority checked: AGENTS (including the actual §16 rustdoc rule), agent rules, reference router, spec-driven development, maintainer style, testing workflows, OLAP serving, analytical operations/reliability, and the governing Bifrost source-selection, admission/memory, deadline, recovery and failure invariants. Revision 2 governs obsolete private revision-1 task mechanics.
- Cumulative coverage includes all 58 paths: SQL revision state/cut projection; cut cache/materialization; Scribe capture, source registry, publication, recovery and shard handoff; Oracle retained routes, planning, profile and follower execution; server discovery/private adapters; wire/digest/conversion; capacity judges; tests/fixtures; current architecture/product/generated prose; workflow prose and carried draft specifications/span-permit correction. The existing planner, notification cache, publisher, staged leases, root memory governor and authenticated peer machinery remain appropriate owners. No blocking duplicate-workflow proposal is supported.
- HEAD was rechecked at the candidate. `git diff --name-only 05186c1e4..0d3223696` contains only the task evidence document, supporting runtime equivalence of retained capacity and later checks. All discovery reports are present and complete for their assigned scopes. No follow-up was required: the remaining disagreements concern severity/correction/acceptance interpretation, resolved below from authority and source.

## Disposition of every discovery proposal

| Discovery ID | Disposition | Final disposition |
| --- | --- | --- |
| BEH-001 | CONFIRMED | FIND-TASK-001-1 |
| INV-REV-02 | CONFIRMED | FIND-TASK-001-1 |
| CONC-001 | REVISED | FIND-TASK-001-1; retain allocation accounting correction, without inventing a separate metadata quota or charging an entire table per query |
| BEH-002 | CONFIRMED | FIND-TASK-001-2 |
| INV-REV-03 | CONFIRMED | FIND-TASK-001-2 |
| CONC-002 | CONFIRMED | FIND-TASK-001-2 |
| DUR-001 | REVISED | FIND-TASK-001-2; ordering outcome accepted, exact lock/control-flow prescription remains implementation-owned |
| SEC-001 | CONFIRMED | FIND-TASK-001-2; capture-local elapsed-authority rejection is required independently of eventual timer cleanup |
| SYSTEM-001 | CONFIRMED | FIND-TASK-001-2 |
| REPO-002 | CONFIRMED | FIND-TASK-001-2 |
| SYSTEM-002 | CONFIRMED | FIND-TASK-001-3; unique proposal verified through outer callers and transport |
| REPO-001 | CONFIRMED | FIND-TASK-001-4; all five cited methods, including formatter, fall under literal §16 |
| M-001 | CONFIRMED | FIND-TASK-001-4 |
| M-002 | REVISED | FIND-TASK-001-5; keep false public terminal promises and expressly required obsolete allowance/instruction deletion; omit merely stylistic residual wording |
| BEH-003 | CONFIRMED as failed acceptance; not an actionable bounded FIND | REQ-006 / AC-006 qualification blocker below; no source optimization has a supported decision-complete diagnosis |
| INV-REV-01 | CONFIRMED as failed acceptance; not an actionable bounded FIND | Same qualification blocker; not a second finding |

No source proposal is discarded because it was unique, and agreement is not used as proof. Unnumbered discovery notes are resolved below. There is no human-directed addition assigned a FIND ID.

## Final deduplicated actionable ledger

### FIND-TASK-001-1 — Retained Arrow outlives its governed charge

**Status:** CONFIRMED (CONC-001 narrowed as above). **Classification:** INCORRECT. **Sources:** BEH-001, INV-REV-02, CONC-001. **Obligation:** INV-006 / AC-002; Bifrost admission/memory and resource invariants require memory still held by children to stay charged to the one shared root.

**Exact locations:** `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs:1678` (`LiveCapture`), `:1701` (`take`), `:1809` (`read`), `:1886` (`ReadableSource::capture`); retirement consumer `scribe/shards.rs:2375` (`release_committed_generation_memory`), particularly `:2405`; accounting owner `scribe/memory.rs:581` (`ScribeOwnership::release_immutable`); follower consumer `oracle/follower.rs:414` (`live_leaf`).

**Independent producer-to-sink proof:** writable `MemtableBucket::readable_source` and frozen `FrozenMemtable::readable_source` both call `ReadableSource::capture`, which clones `RecordBatch` references and bounds/WAL values only. `LiveCapture::take` retains all matching table batches and later `read` projects them into `CapturedRead`; neither carries an accounting owner. `FetchLiveTailService::retain` stores this capture for the query lifetime. `Oracle::run_sql_attempt` calls `classify_one_build` before `admit_built_attempt` (`oracle/mod.rs:2158–2176`), so an ordinary captured table may wait for admission or another table without a follower grant. Normal rotation/staging makes the generation durable. `release_committed_generation_memory` unconditionally shrinks the immutable root-backed ledger before `Memtable::commit_retirement` removes the generation. Its full body checks identity/bytes and poison handling, but never retained Arrow holders. `LiveCapture` still owns those allocations. `open_capture` and `live_leaf` project the retained references into `MemtableSource`, without adopting the original charge. Projected reads can also outlive the registry capture. This is a newly extended whole-table, pre-admission, query-lifetime ownership path, not an unrelated old short snapshot concern.

**Sibling paths and existing alternative:** `retire_committed_generation` and both replay-completion paths use the same release method. Replay rollback (`rollback_replayed_generation`, shards line 1938) is another immutable release writer but occurs before serving readiness; preserve its exact cleanup. Active insertion rollback and category movement belong to the same `ScribeOwnership`, not another budget. Existing `ScribeMemoryLease::{split,merge,transfer_bytes_to}` (resources lines 2947, 2969, 3272) support net-zero ownership transfer; no new capacity admission is necessary to transfer already charged bytes. Staged leases protect paths, not Arrow allocation accounting. Existing `MemtableSource` and its pushdown/repartitioned clones must retain whichever Arrow ownership they keep; changing only the table registry is insufficient.

**Consequence:** ingestion can reuse supposedly freed capacity while captures/readers retain prior allocations. Multiple retained generations can exceed the shared governed memory bound and threaten sibling availability despite apparently reclaimed ledger totals.

**Decision-complete minimum correction:** keep the existing admitted Arrow charge attached to shared allocation ownership from active insertion, through freeze/retirement and capture projection, until the last physical holder releases it. Reuse `ScribeOwnership` and root-backed `ScribeMemoryLease` transfer; detach the retiring generation's existing charge into shared retained ownership instead of returning it while captured buffers remain. Share that ownership with overlapping captures and opened memory readers, and return it exactly once at the final holder. Preserve ordinary category/accounting errors, rollback, WAL retirement and publication progress. Do not add a second governor/quota, double-charge each capture, reserve an entire table per query, delay durable publication until query completion, or move admission ahead of physical classification. This is correction of the existing budget promise, not new concurrency or resource semantics.

**Focused closure proof:** at the existing charged shard/resource owner, capture real active/frozen buffers, stage and retire them while the capture remains queued/pre-admission, and assert the shared root still accounts those bytes and rejects growth past its cap. Exercise two captures sharing allocations and an opened projected read surviving registry release; dropping either non-final owner must not uncharge, and dropping the final owner must return the exact charge once. Preserve exact rows and ordinary WAL/retirement progress. Use existing governor/shard fixtures, not stress, a new harness or a full journey suite.

### FIND-TASK-001-2 — Expiry is neither safely installed nor enforced at capture open

**Status:** REVISED consolidated lifecycle correction. **Classification:** INCORRECT. **Sources:** BEH-002, INV-REV-03, CONC-002, DUR-001, SEC-001, SYSTEM-001, REPO-002. **Obligation:** INV-002 forbids expired capture authority; INV-006 / AC-002 requires bounded retention and expiry fallback when a listing/release is lost.

**Exact locations:** `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs:468` (`RetainedCapture`), `:690` (`capture_active_streams`), `:750` (`capture`), `:805` (`retain`, especially spawn at 814 and insert at 821), `:877` (`open_capture`, lookup at 892). The table's complete `insert/get/remove` bodies are at 521/534/551.

**Independent proof:** the listing converts a positive wall-time remainder to a monotonic deadline before synchronous capture/sizing. `capture` can finish its successful `LiveCapture::take` branch after that deadline; only its unsettled-publication wait has a timeout. `retain` spawns a task that sleeps to deadline, removes the ID once, and exits, then inserts that ID. Another runtime worker may complete removal while the ID is absent; the caller subsequently inserts a capture whose only expiry task has finished. Descheduling between spawn and insert suffices even with an originally future deadline. A lost/cancelled response gives Oracle no ID to release. No other sweep exists. Separately, even correctly installed entries remain openable after deadline until the timer is polled: `RetainedCapture` has no deadline, and `open_capture` checks stream, ordering, query and full binding but not elapsed authority before `capture.read`.

**Reachability and sibling tracing:** local `RegistryTailStreamDiscovery::list_local` and private `ScribeTailGrpc::list_active_streams` both reach this one producer. All production table insertions are from `retain`; explicit release and the timer are its only removers. `ScribeImpl` shares this table across tail services. All normal opens go through `ScribeTailResolver::resolve` → `LiveTailSource::open` → `open_capture`. Authenticated peer expiry is checked before an awaited catalog schema resolution; it cannot establish capture-local validity at the later synchronous open. A leader may ultimately reject the result at its own deadline, but INV-002 separately forbids using expired source authority. Thus this guard belongs at the capture authority owner, not in each follower or public caller. Existing open readers retain independent batches/staged leases and must remain protected after registry removal.

**Consequence:** an abandoned capture can pin memory/staged files for the Scribe lifetime; normal and recovered publication cleanup both wait on its staged lease (`claim_publication.rs:627,861` → `hot_source.rs:994`). Correct cleanup must not delete those files to escape the leak. Also, an elapsed capture can authorize a new read while cleanup scheduling is delayed.

**Decision-complete minimum correction:** make registration and expiry activation/removal one ordered lifecycle in the existing capture table owner so removal cannot finish before future registration. Refuse elapsed acquisition before exposing an identity, store the capture's original monotonic deadline with its authority, and refuse new opens once it elapsed before projecting its sources. A precheck alone is insufficient to order registration. Reuse the same table/removal/timer and existing refusal vocabulary; no second reaper, sweep, retry subsystem, public contract or per-consumer workaround. Keep query/table/stream checks, authenticated idempotent release, removal values dropped outside locks, and already-opened reader protection. Exact private locking/handshake representation remains implementation-owned.

**Focused closure proof:** deterministically force deadline readiness during installation before a remover could observe the entry; without explicit release require no retained table entry and drained staged lease after expiry. Separately advance the owning clock past deadline without polling expiry cleanup and require `open_capture` to refuse. Keep ordinary future expiry, foreign query/ID refusal, foreign release no-op, explicit release and opened-reader retention proofs. Existing `an_unreleased_capture_expires_at_its_deadline` advances after insertion and yields to cleanup, so it does not cover either gap.

### FIND-TASK-001-3 — Failed acquisition can wait forever on release after its deadline

**Status:** CONFIRMED. **Classification:** INCORRECT. **Source:** SYSTEM-002. **Obligation:** INV-006 / AC-002 and existing total query deadline; best-effort release is backed by Scribe expiry, not an unbounded leader wait.

**Exact locations:** `crates/wyrd/wyrd-server/src/oracle/tail_discovery.rs:244` (`admit`, release await at 280), `:304` (`discover`, timeout at 354 and post-timeout admit at 362), `:369` (`release`, RPC join at 399–413); sibling owner `crates/vala/vala-bifrost-redux/src/oracle/live.rs:102,170` (`LiveCaptures`, drop); transport `scribe/tail_rpc.rs:139` (`release_capture`).

**Independent proof:** `discover` times only the concurrent listings. If one peer returned a capture and another errored/staled, `admit` awaits release of the successful captures after that timeout future has returned. `release` awaits `join_all` of RPCs with no remaining-budget timeout; `TonicTailReadTransport::release_capture` sets none, and the full TLS endpoint constructor at `wyrd-tonic/src/transport.rs:68` sets TLS only. A peer accepting the release but never replying therefore leaves acquisition pending indefinitely. Scribe expiry removes data but cannot resolve the pending release RPC.

**Outer and sibling consumers checked:** `Oracle::{query_sql_with_deadline,run_sql_query,run_sql_attempt,classify_one_build,build_physical_root,discover_live_routes,list_live_routes}` await this discovery without a surrounding timeout. `before_deadline` wraps particular planning operations, not this entire discovery. `ReadyOracleForwarder::forward` selects the local branch directly, `execute_local` directly awaits Oracle; `query/service.rs:205–226` hands off the edge timer before dispatch. Public local and scheduled/internal evidence reads can therefore observe the hang. Remote delivery timeouts do not rescue local request work. The only other release caller is detached `LiveCaptures::drop`; it also has no deadline and can accumulate hanging cleanup tasks/channels.

**Decision-complete minimum correction:** carry the already existing absolute query deadline through retained route cleanup and failed-acquisition cleanup, and bound the existing release workflow by that deadline at its shared discovery owner. At exhaustion stop awaiting release, log the best-effort failure, and rely on the corrected Scribe expiry from FIND-TASK-001-2. Preserve successful-peer cleanup and the existing listing/stale/elapsed error mapping; no renewed budget, retry loop, independent timeout configuration, fresh capture or process shutdown. This requires only internal plumbing, not a new public API or cross-service decision.

**Focused closure proof:** using existing private peer/discovery test facilities, return one successful capture and another listing failure, then leave the successful peer's release pending. Assert the dependent discovery/query settles by its original deadline and retained sources expire. Exercise the same bound on last-route drop cleanup, and prove a responsive release frees the capture promptly. Existing global injected listing outage returns before a successful capture; existing stalled listing is covered by the earlier timeout and cannot prove this path.

### FIND-TASK-001-4 — Mandatory changed-method rustdoc is incomplete

**Status:** CONFIRMED. **Classification:** VIOLATION. **Sources:** M-001, REPO-001. **Obligation:** actual repository AGENTS §16 and agent-rules require substantive docs for private/test items too, explicit `# Errors` on every fallible method, and relevant cancellation/partial-progress semantics. The review skill explicitly preserves this blocking gate.

**Exact changed methods, verified against their full bodies:**

- `crates/wyrd/wyrd-server/src/grpc/scribe_tail.rs:49`, `list_active_streams`: missing error and interrupted-response retention contract; malformed binding/query UUID or elapsed/state capture failure can reject, and cancellation after retention relies on release/expiry.
- Same file `:83`, new `release_capture`: missing malformed binding/UUID and capture-table-state error conditions.
- `crates/vala/vala-bifrost-redux/src/oracle/follower.rs:158`, changed `Arc<T>::resolve`: missing explicit delegated-error contract; forwards the new query identity and returns the injected resolver's errors unchanged.
- `crates/vala/vala-bifrost-redux/src/oracle/live.rs:138`, new fixture `NoCaptureDiscovery::discover`: missing `# Errors` stating this fixture always succeeds.
- Same file `:193`, new `LiveCaptures::fmt`: missing `# Errors` for propagation of formatter failure.

**Consequence/correction:** the receiving lifecycle/error contract is undocumented and the explicit completion gate fails. Complete those local docs against corrected bodies, including cancellation where relevant; preserve behavior and error mapping. No helper, wrapper, new error or runtime test is warranted. This documentation work must be batched with the material runtime corrections, not made a separate style-only round.

### FIND-TASK-001-5 — Current documentation promises degraded results for fatal accepted-source loss

**Status:** REVISED. **Classification:** INCORRECT. **Source:** M-002, corroborated but not proved by unnumbered behavior/invariant/standards notes. **Obligation:** REQ-005 / AC-004 explicitly remove obsolete publication allowances; INV-005 and the approved technical boundary require loss of an accepted capture to fail execution.

**Exact locations:** `architecture/references/domain/olap-serving.md:93–94` retains the accepted duplicate-row allowance; `architecture/bifrost-design.md:357–359` retains generic pre-row live degradation; `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs:1782–1787` instructs callers to degrade an unavailable execution before rows; `oracle/query_stream.rs:891–893` repeats general pre-row loss guidance. Newly changed public descriptions at `architecture/wyrd-design.md:1699–1700`, `docs/src/content/docs/understand/bifrost.md:61`, `docs/src/content/docs/understand/runs-observations-and-evidence.md:107`, and `docs/src/lib/diagrams/bifrost.ts:300` promise degradation whenever the holding Scribe is unreachable. `docs/public/llms-full.txt:5004,5114` repeats the authored descriptions.

**Independent producer-to-consumer proof:** `Oracle::list_live_routes` records unavailable/still-stale listing as `listing_lost`; `record_live_tail_unavailable` contributes known degradation. In contrast, retained routes create assignments with `capture_id`; `LiveFragmentRead::into_stream` maps every dispatch/open error at `live.rs:809–813` and stream error at `:839–842` to a fatal execution error, without a pre-row availability exception. The updated distributed terminal failure matrix expects Failed for `UnavailableBeforeRows`. `live_execution_status_error` still classifies the transport error, but its caller no longer converts that classification to degraded execution. Thus a legitimate reachable unavailable Scribe after capture fails, contradicting current public guidance. Published/live duplication permission also conflicts directly with the approved revision and the same reference's corrected earlier paragraph.

**Decision-complete minimum correction:** reconcile those current contract statements to the implemented boundary: failure to list streams may produce known listing degradation; loss of an accepted capture fails its dependent execution even before a first row; publication itself never permits a selected row to disappear or repeat. Delete the obsolete duplication allowance and execution-degrade instructions. Preserve runtime branches, existing error vocabulary, listing degradation and historical packets. Update authored product/diagram prose and regenerate LLM text with the existing docs generator. Do not change the runtime to match the stale prose. Residual route descriptions saying they retain no rows are not a separate blocker: raw rows remain on Scribe, so merely preferring more explicit wording has no demonstrated public-contract consequence.

**Focused closure proof:** compare corrected current statements against the listing and live execution branches and existing terminal matrix; run the existing docs lane for authored/generated agreement. No new runtime test is needed for the wording correction. This finding has a public-contract consequence and expressly required deletion outcome; it is stronger than a prose-style preference and is batched with the runtime remediation.

## Confirmed acceptance failure without an invented remediation

**Discovery sources BEH-003 and INV-REV-01 are CONFIRMED as a failed obligation, but intentionally receive no actionable FIND ID.** Authority is approved spec REQ-006, AC-006 and its performance-contract table; task evidence cannot waive it.

I inspected actual `../bsc-rev2/status`, `commit`, `verification-artifacts/report.md`, and hot/Parquet `query-artifacts/standard/*/report.md`. The runtime-equivalent commit is `05186c1e443096e6c33d6b5f08831b613fa2caa2`. Status is write 0, verification 1, hot query 1, Parquet query 1. One-client selective p99 is **2.352 ms hot** and **2.745 ms Parquet**, against **≤1.000 ms**. Direct verification at 64 clients reports **p99 bucket 0.9 ms** against **≤0.5 ms**, with 435,378 observations and no errors. Its histogram evidence cannot establish the approved bound. Lower-client direct points pass. Peak selective throughput exceeds the separate ≥1,200 QPS floor; that cannot substitute for latency. The source judges correctly use inclusive comparison and preserve other thresholds.

The Parquet table aggregate at 56.9 QPS passes its existing configured criterion. The 66.4 baseline comparison alone establishes neither a causal regression nor that the decrease is noise; no separate FIND is justified. A matching baseline that also misses absolute latency does not change the promised limits. The final sequence predates the later verification pass, so the prescribed final ordering also cannot be certified from that sequence; failures already decide AC-006 regardless.

**Correction disposition:** actual performance diagnosis is incomplete. Neither reviewer consensus nor these headline reports proves a particular optimization sufficient. An instruction merely to “diagnose and optimize until fast” would not be a decision-complete bounded remediation, so it is not smuggled into FIND-TASK-001-1 through -5. The final verdict must preserve AC-006 as failed after any bounded remediation closure until a final implementation-equivalent candidate passes the approved comparable write → verification → hot-and-Parquet query sequence (query last), retaining its evidence. Performance investigation/work remains required under the existing specification. No existing source evidence proves meeting those limits requires changing approved behavior, architecture or ownership semantics. Consequently **inability to grant PASS is not evidence that specification revision is required**. If the maintainer instead chooses baseline-relative acceptance or relaxed thresholds, that choice requires an explicit approved specification revision; this review grants no waiver and prescribes no speculative optimization.

## Reconciled acceptance matrix

| Obligation | Source/proof assessment | Reconciled result |
| --- | --- | --- |
| REQ-001 / AC-001 | Per-node committed revisions, coherent memory/staged capture, stale-cut eviction/repin; exact-row and rollover proof recorded passing | PASS for row-selection behavior; retention budget remains separately failed |
| REQ-002 | Discovery is before execution/output; stale selection drops routes before rebuilding | PASS for selection ordering; failed-acquisition deadline cleanup fails FIND-3 |
| REQ-003 / INV-004 | Publication marked before SQL; unresolved outcomes refuse; atomic authority/revision advancement; restore before readiness; transaction rollback/replay proof | PASS |
| REQ-004 / AC-003 | Same server read service and late-evidence regression; integrated SDK/agent checklist explicitly belongs to change review | PASS at task scope; integrated proof remains due |
| REQ-005 / AC-004 | Fresh execution-time source open removed; independent integrity/file protection retained; contradictory current contract remains | FAIL — FIND-5 |
| INV-001 | Existing Oracle/Scribe/Vala SQL boundaries extended; no client/group workaround | PASS |
| INV-002 | Query/table/stream/epoch/capture digest wiring correct; expired authority not locally enforced | FAIL — FIND-2 |
| INV-003 | No remote-held barrier or new per-append catalog read; source capture only briefly excludes local movement | PASS |
| INV-005 | Lost accepted capture fails; listing-only known degradation preserved; unresolved selection emits no execution rows | PASS for result semantics; deadline settlement fails FIND-3 |
| INV-006 / AC-002 | Bare retained Arrow after release; expiry registration/read-authority gaps; unbounded remote cleanup | FAIL — FIND-1, FIND-2, FIND-3 |
| INV-007 / AC-005 | Accurate inclusive report boundaries, real client acquisition cost, retained samples/errors/refusals and old strict limits | PASS for measurement/judgment integrity |
| INV-008 / AC-007 | Cached current cuts compared in memory; generation-safe explicit eviction; recorded warm-zero-read and one-repin proof | PASS |
| REQ-006 / AC-006 | Actual final comparable capacity fails absolute targets; no baseline waiver | FAIL — qualification blocker, no invented bounded FIND |
| Repository rustdoc completion | Five changed methods lack explicit required contracts | FAIL — FIND-4 |
| Non-goals and unaffected integrity | No unavailable-writer recovery, new WAL/history scan, public selector, second scanner or scheduler workaround; RLS/peer/tenant/footer checks remain | PASS |

## Notes adjudicated and minimum remediation verification

- Pre-existing `PgConnection` cut-owner / `PgPool` watcher signatures conflict with literal SQL-capability prose, but this range does not introduce them or prove a new tenant escape. Do not waive the mismatch, widen SQL authority or bundle a broader redesign into this task.
- Local imports/fully-qualified signatures without a demonstrated behavior/contract effect are non-blocking placement preferences under this review threshold. They receive no FIND and must not generate a style round.
- Stale task revision/status and long-horizon supersession wording should be reconciled when recording disposition, subordinate to approved revision 2. They do not make authority unobtainable. UUID v4 activates an already installed dependency capability; the task's blanket “no feature” wording is inaccurate, but no new public/deployment feature or unresolved boundary follows. No new code is needed to correct the wording.
- The two carried Verifier draft specifications remain draft, add no runtime behavior and are expressly disclosed by the user as carried branch contents. Their presence supplies no approval for their proposed features and no source-consistency spec waiver. No deletion finding is supported solely by their presence.
- Test/fixture corrections have source-consistent recorded diagnoses (permit owner returns asynchronously; wrong fixture WAL ranges; required second live batch; real capture identity; Forge notifications causing the genuine extra read). No source-backed proposal establishes a hidden test weakening; do not manufacture one.

For a remediation restricted to these findings, use new focused owner tests and exact commands after confirming their actual names (new names are not claimed to exist or to have run in this review). Minimum existing regression selectors are:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tail_rpc::tests::active_rollover_during_capture_is_read_once) | test(=scribe::tail_rpc::tests::a_generation_staged_before_the_shard_settles_is_read_once) | test(=scribe::tail_rpc::tests::an_open_live_read_keeps_staged_runs_across_publication) | test(=scribe::tail_rpc::tests::a_capture_is_opened_only_by_its_query) | test(=scribe::tail_rpc::tests::an_unreleased_capture_expires_at_its_deadline) | test(=scribe::memory::tests::generation_ownership_moves_exact_category_bytes)'
```

Exercise the new shared-memory lifetime proof at the existing shard/resource fixture and the failed-listing/stalled-release proof at the existing server/private discovery fixture, with exact focused selectors and the repository Postgres wrapper only where that fixture actually needs it. Existing module-scoped supporting runs can use `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::tail_rpc::tests::/) | test(/^scribe::memory::tests::/) | test(/^oracle::follower::tests::/)'` and a focused `wyrd-server --lib` selector for the newly added discovery tests. Avoid claiming `test:bifrost:unit:rust:inner` covers Redux: its actual task includes server/testing/client, not Redux. For server family coverage when needed, run `mise exec -- scripts/postgres/with-test-postgres.sh -- mise run test:bifrost:unit:rust:inner`. If the release fixture specifically changes/exercises server integration seams, use `mise run test:bifrost:integration:server`; do not automatically add unrelated SQL/redux integration or whole journey aggregates.

Run `mise run fmt`, `mise run lints`, `mise run docs:generate`, `mise run docs:check`, and `git diff --check` for the expected Rust/docs write set. The docs generator is the existing source-driven update mechanism. Add `check:deps` only if the actual correction changes dependency/boundary wiring; no dependency addition is prescribed. No public schema/SDK/SQL change is needed for this correction. Broad `verify:bifrost`, full SDK/agent journeys and the required final performance sequence remain integrated qualification obligations, not a request to rerun them inside this bounded remediation.

Skipped: execution and reproduction of new tests, builds, capacity reruns, and integrated client/agent gates; this was static independent validation.
Risk: the candidate has demonstrated source-level lifetime/accounting/deadline defects, and actual capacity qualification remains failed independently of their closure.
