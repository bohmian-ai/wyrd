# Concurrency domain review — TASK-001

Result: **FAIL**

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-source-consistency`.
- Base: `1c5edf72958af8f47c63eb8ce39eb316672e170a`.
- Candidate: `0d3223696f091fb16bb2e0ec36040a249d39b61f`; HEAD remained this candidate during this audit.
- Approved authority: `changes/active/bifrost-source-consistency/spec.md`, revision 2, and original `tasks/TASK-001-source-consistency.md` (revision 2 controls conflicts).
- Reviewed domain: coherent local capture, writable rollover, publication exclusion, retained-source resource ownership, cancellation and expiry across the Oracle/Scribe boundary.

I read the cumulative base-to-candidate diff directly, used CodeGraph before source navigation, and expanded the shared map through actual resource owners, shard retirement, follower memory sources, and local/private-RPC discovery callers. This is an independent discovery report; no other reviewer's report was consulted. Source and tests were not edited.

## Authority and source coverage

Read the task-review skill, repository AGENTS and agent rules, spec-driven-development and maintainer references, reference router, approved specification/task and recorded verification evidence. Applicable design authority includes Wyrd design/doctrine, Bifrost design (source ownership, pinned reads, admission/memory and shutdown), OLAP serving, analytical operations/reliability, and testing-workflow guidance.

| Boundary | Source and callers inspected | Assessment |
| --- | --- | --- |
| Writable → immutable → staged capture | `memtable.rs::{read_guards,select_sources,freeze,freeze_all_nonempty,complete_staged,LiveCapture::take,ReadableSource::capture}`, `shards.rs::{memtables,flush_staged,release_committed_generation_memory,retire_committed_generation}` | The common writable/immutable read guards prevent rollover between the memory selection and registry selection. Lock acquisition follows the existing writable-before-immutable order. No opposite registry-to-memtable critical section was found in the publication caller. Resource lifetime is incomplete; CONC-001. |
| Staged → published source exclusion | `hot_source.rs::{capture_staged,begin_publication,abandon_publication,advance_all_atomic,release,wait_for_no_leases,restore_publication_revisions}`, `claim_publication.rs::{mark_publication,publish,advance_published,retire_members,finish_committed,recover_terminal_members}`, staging restoration and persistence reconciler | Publication is marked before catalog IO. Known non-commit clears the mark; unresolved outcome retains it. Capture refuses the table while marked. Atomic local publication movement/revision and retained staged leases avoid choosing both authorities. No additional blocking finding. |
| Revision/cut concurrency | Publication migration/trigger, `insert_artifact_set_fenced`, SQL table-cut grouping, `CutCache::evict`, `BifrostCatalog::evict_cut`, Oracle `classify_one_build/build_physical_root/list_live_routes` | Per-node counter serialization is within the publishing transaction. Eviction invalidates raced fills. A greater local revision discards the selection before execution. SQL durability/security details are outside this domain's independent conclusion. |
| Retention and cancellation | `tail_rpc.rs::{CaptureTable,RetainedCapture,capture_active_streams,capture,retain,release_capture,open_capture}`, `ScribeImpl::tail_service`, `live.rs::{LiveCaptures,LiveTableRoutes,LiveFragmentRead}`, `RegistryTailStreamDiscovery::{discover,list_local,admit,release}`, private gRPC adapter/transport | Captures are shared across service instances; the last route owner schedules release, and opened staged reads retain their lease independently. Failed/unknown RPCs rely on expiry. Installation can miss that expiry; CONC-002. |
| Follower resource consumers | `ScribeTailResolver::{resolve,live_leaf}`, `LiveCapture::read`, `ReadableSource::project`, `MemtableSource::{try_new,open}`, `ScribeOwnership`, `ScribeMemoryLease` usage and shard memory release | No retained capture or projected memory leaf carries the original governed charge past shard retirement. A later follower runtime cannot account the full capture while it is waiting before admission; CONC-001. |
| Proof | New publication/cancellation journeys, rollover/staging/retained-read tests, expiry unit test, capacity/static evidence in task | Recorded green tests support the ordinary transitions but do not exercise retirement accounting under a retained capture or expiry racing installation. |

The cumulative range's other surfaces (workflow/draft-spec prose, capacity judgment changes, public documentation and test-only span-permit correction) were considered for concurrency impact; they do not independently alter this domain's runtime ownership. No benchmark result is claimed to prove the two races below.

## Findings

### CONC-001 — Captured Arrow survives the release of its governed memory charge

- Classification: **INCORRECT**.
- Severity: high resource/availability consequence.
- Violated obligation: specification **INV-006** requires captures to use existing query admission and memory budgets; **INV-003** preserves ingestion progress. Bifrost design's admission/memory section requires live ownership to remain charged against the shared Bifrost cap. Analytical reliability requires bounded active/immutable memory ownership.
- Changed location: `crates/vala/vala-bifrost-redux/src/scribe/memtable.rs:1678` (`LiveCapture`) and `:1712`–`:1752` (whole-table references retained without a memory owner); `tail_rpc.rs:821`–`:829` retains this state.
- Shared producer and consumer evidence:
  1. `LiveCapture::take` selects all matching table memory sources with no partition restriction. `ReadableSource::capture` (`memtable.rs:1886`–`:1901`) clones `RecordBatch` references, including all columns. The retained table therefore keeps their Arrow allocations alive even when a selective query will not read them.
  2. `Oracle::classify_one_build` constructs this retained selection before `run_sql_attempt` calls `admit_built_attempt` (`oracle/mod.rs:2150` onward). A capture can remain while its query waits for admission; it is not protected by a follower execution reservation.
  3. A writable source can then freeze and stage normally. `ShardOwner::release_committed_generation_memory` (`shards.rs:2376`–`:2420`) unconditionally calls `ScribeOwnership::release_immutable(token.arrow_bytes)` at `:2405` before removing the memtable entry. `ScribeOwnership::release_immutable` shrinks the shared root-backed ledger (`scribe/memory.rs:581`–`:593`). It has no connection to `LiveCapture` references.
  4. `LiveCapture`, `ReadableSource`, `RetainedCapture`, and `CapturedRead` hold batches/leases but no governed memory lease. Follower `live_leaf` builds `MemtableSource` over projected references, and that source only wraps `MemorySourceConfig`; it does not cover the full retained capture's bytes. Keeping projected opened reads after capture release also needs to preserve the relevant charge.
- Reachable failure: query A captures a large writable table, then waits for admission or remains open. Normal rollover/staging retires the original generation and frees its governed reservation while A still pins its allocations. Fresh writes reuse the reported free budget. Queries spanning successive generations can retain multiple generations beyond the shared cap without a memory refusal, even though the existing memory gauges show reclaimed capacity. This is ordinary live ingestion plus queries, not a malicious private-peer path.
- Why the candidate matters: the new owner retains whole-table, all-column snapshots for the query lifetime and does so before admission; old discovery references were temporary and execution opened a range/projection. The candidate explicitly adds this long-lived ownership and must preserve its required budget.
- Smallest correction boundary: keep the existing Scribe/root-backed Arrow ownership attached to the lifetime of the captured allocations, and propagate it to opened reads that can outlive the retained-table entry. Reuse `ScribeOwnership`/`ScribeMemoryLease` and normal typed resource refusals; do not create another memory budget, public selector, global append pause, or move query admission before its physical classification. Retirement must not return bytes that live captures/reads still retain, and capture creation must not leave retained metadata outside applicable resource ownership. The implementer should fix the common ownership boundary rather than add checks to individual follower consumers.
- Focused closure proof: use an existing production shard/memory-governor fixture to hold a real capture across writable freeze, successful staging and shard retirement. Show that the retained buffers remain charged and competing growth at the cap refuses. Cover two captures sharing the same allocations and an opened read outliving capture release; release the last owner and show the charge returns exactly once. Preserve normal retirement/WAL progress and retained-source readability. The existing row-count and capture-count assertions alone cannot prove this.

### CONC-002 — Expiry can finish before the retained capture is installed

- Classification: **INCORRECT**.
- Severity: resource leak; can indefinitely retain staged files and delay claim cleanup.
- Violated obligation: specification **INV-006**, **AC-002**, and the explicit expiry fallback for abandoned/failed acquisition. A lost release must not retain a capture after its query deadline.
- Exact changed location: `crates/vala/vala-bifrost-redux/src/scribe/tail_rpc.rs:812`–`:829`, especially spawning the expiry at `:814` before `CaptureTable::insert` at `:821`.
- Producer-to-consumer evidence:
  1. `capture_active_streams` validates the wall-clock expiry once, then `capture` takes synchronous memtable/registry locks and scans sources. On a successful capture its deadline is never checked again (`tail_rpc.rs:777`–`:785`); `partition_bytes` and installation also execute afterward. A short valid deadline can therefore have elapsed by the time `retain` begins. This can occur with ordinary near-deadline network delivery or lock contention.
  2. `retain` starts a Tokio task whose first operation is `sleep_until(deadline)`. On the multithread runtime this task can run on another worker before the following insertion. If the deadline has passed, it immediately removes the ID from the table while it is absent, then exits.
  3. The requesting worker then inserts the capture. No later sweep exists: this task was its one expiry owner. `RetainedCapture` holds only the abort handle, not a deadline that another operation enforces or a recurring cleanup owner.
  4. Failed/cancelled listings explicitly rely on expiry when Oracle never learns the ID (`RegistryTailStreamDiscovery::discover`), and a normal private response may also be lost. Thus the leaked entry has no guaranteed later explicit release. Its Arrow references and staged lease survive; publication cleanup waits for that lease via `ClaimPublisher::await_lease_drain`/registry lease drainage.
- Observable consequence: an acquisition which ended at its deadline can permanently retain a capture, memory and staged files until process teardown; subsequent captures do not clean it up. The ordinary `start_paused` expiry test advances time only after insertion and cannot hit this schedule.
- Smallest correction boundary: make capture installation and arming/removal by its deadline one ordered lifecycle under the existing `CaptureTable`/retained-capture owner, so expiry cannot observe absence and finish before a future insertion. Reject elapsed capture creation at the same owner boundary; a pre-check alone is insufficient because the timer can still win after that check. Preserve release idempotence and the rule that already-opened reads keep their own leases. No new persistent reaper, retry protocol, or resource configuration is needed.
- Focused closure proof: deterministically exercise expiry before installation would complete, including the deadline elapsing during selection and an abandoned response with no explicit release. Require no retained entry and drained staged lease after the deadline. Also preserve release-before-expiry and ordinary expiry tests. Use a controlled scheduling seam or direct lifecycle unit proof, not host stress or sleep-based probability.

## Verification limits and result

No Cargo checks, benchmarks, or failure-reproduction tests were run as instructed. The task's recorded passing selectors and lanes were inspected as evidence, not independently rerun. These findings are source-proven ownership/interleaving gaps; they do not assert that the recorded runs observed an OOM or leak.

The local ownership selection/publication locking is coherent on the inspected normal and uncertain-publication paths. The retained-resource lifecycle is incomplete in the two reachable paths above, so the domain result is **FAIL** pending independent validation of CONC-001 and CONC-002.

Skipped: test and capacity reruns; no source changes. Risk: retained captures can outlive both their governed memory charge and their deadline cleanup.
