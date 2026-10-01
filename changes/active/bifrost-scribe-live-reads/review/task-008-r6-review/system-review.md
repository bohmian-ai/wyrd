# Independent system-resilience review

**Result: PASS. Proposed findings: none.**

## Subject and scope

Candidate `8955e75b71ded9d39daf7649a0c985be0803e266`; correction parent `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`. Reviewed cumulative TASK-007/TASK-008 runtime seams from their respective bases `a7582db587c6170a290760f1741673125612b797` and `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`, original tasks, approved revision-20 spec, remediation requirements and R6 correction diff. The latest TASK-006 benchmark synchronization delta is reviewed for correctness only, as expressly authorized.

Authorities: AGENTS.md, agent rules, spec-driven-development and maintainer-style references; Wyrd design/doctrine and Bifrost design, particularly per-file tenancy, node-local execution, source lifetime and shutdown/recovery; analytical-operations-reliability. Standing decisions remain accepted: no live-read cap, no shutdown residue publication, chosen tenant-invariant code, SQL columns excluded, FIND-007-3 unchanged. No sibling R6 reviewer report was consulted.

## Deployed path and failure assessment

Wyrd's only serving process hosts selected Bifrost roles. A query leader plans and admits work, then dispatches published workers and Scribe-local fragments. Scribe owns its own staging volume, WAL, source leases and follower execution; the leader never opens a remote local staging path. Persistent control-plane identity/publication fences remain in Postgres and retained objects; losing one Scribe does not transfer its local-file authority to Oracle.

| Reachable path/failure | Actual owner and source evidence | System consequence and recovery | Assessment |
|---|---|---|---|
| Staged live scan or early query cancellation | `oracle/follower.rs:775–843` creates a local HotParquetExec under the follower session pool; `exec.rs:2739` retains its staged lease, and the scan's streams retain execution ownership. Local storage uses async OpenDAL at `follower.rs:719`. | Source files remain protected while consumed; dropping the execution releases query-held references/leases. Work is neither copied into a remote staging reader nor blocking filesystem IO on a runtime worker. | PASS |
| Foreign/missing footer, including cache hits and count/projection reads | `exec.rs:998–1055` verifies published metadata after cache resolution and before reader use; `exec.rs:1115–1150` is shared proof; `exec.rs:2961` proves hot/staged metadata before builder/pruning/decode. | The affected query fails closed before rows from the offending file; the error is a query outcome, not a process crash. Node cache retention cannot bypass proof. | PASS |
| Remote Scribe footer refusal | `peer_service.rs:690` preserves typed tenant refusal; `:658` emits Aborted; dispatcher `:1785,1815` preserves TenantInvariant both opening and streaming; `live.rs:625–645` excludes it from availability degradation. Leader `oracle/mod.rs:2484` audits the first refusal through the authenticated query owner. | Security refusal stays terminal instead of becoming an omitted live source. Other queries/services remain available; no retry or outage classification weakens integrity. | PASS |
| Stopping Scribe with HTTP/2 reader stalled on window credit | `app/server.rs:675–690` selects stopping IO only for Scribe-containing peer listeners; `wyrd-tonic/src/server/mod.rs:268–408` registers cancellation wakeups on accepted socket IO and forwards TCP connect info. Peer fragment body also observes shutdown at `peer_service.rs:578`. | Cancellation can drop streams and their source leases even when the body is not polled. The stopping private Scribe/mixed peer boundary ends accepted connections; Oracle-only peers and public serving retain their ordinary graceful runner. No unrelated server is stopped by a query error. | PASS |
| Graceful shutdown with below-target staged residue | `scribe/mod.rs:1315–1409` closes admission, flushes/shard-drains, closes and drains persistence, then closes execution lanes and finalizes owners under the same deadline. No residue sweep call remains. | Already-admitted target/dwell publication can settle; below-target staged work remains durable for restart. Shutdown neither forces reencoding nor deletes durable evidence to satisfy its deadline. | PASS |
| Deadline/cancelled shutdown or process interruption | `scribe/mod.rs:1418–1464` closes/aborts retained owners and WAL streams without an unbounded cleanup await. `replay_wal_async` at `:2481–2534` restores staging, reconciles publication and resumes durable claims before WAL replay/readiness. | Accepted durable authority survives interruption. Readiness does not advertise unreconciled recovery as serving; staged work reenters ordinary production publication. Existing fences, source identity and WAL settlement are preserved. | PASS |
| Object/catalog publication failure and explicit flush retry | `scribe/persistence.rs:2036–2060` resumes retryable claims under their original identity, then sweeps ready keys. `flush_staged` at `scribe/mod.rs:2450–2468` still deliberately calls this owner with Drain. | Failed publication retains staging/WAL authority rather than dropping members. Explicit flush remains available independently of shutdown; no new retry amplifier or second publication mechanism is introduced by R6. | PASS |

## R6 wording and executable identity

The HEAD-parent diff for `analytical-operations-reliability.md`, `scribe/assembly.rs`, `scribe/staging_runtime.rs`, and `tests/bifrost/scribe/lifecycle.rs` changes only prose/doc comments. It changes no executable statement, assertion, test name, attribute, enum value or runtime dependency. The guide and assembly docs now distinguish admitted-work drain from retained below-target staging and the explicit residue flush. The lifecycle journey prose accurately describes retained-root restart, zero-or-complete claim publication and exact acknowledged-row readback; its assertions are identical.

FIND-007-13 is closed within this system review: the descriptions no longer promise the removed forced shutdown sweep. No executable durability/recovery change is inferred from this wording correction.

## TASK-006 benchmark delta

`Bench::overload_queue` in `bifrost_query_capacity/run.rs:372–452` now waits on the already-exported aggregate local-slot occupancy before creating waiters. `resources.rs:317–327` emits limit and used from the same ledger and updates occupancy on Oracle admission/release. `server.rs` Metrics::sum selects the exact metric family and kind label. The benchmark starts one real server and runs workload phases sequentially; the holders retain their stream while waiting for release. Thus the added observation barrier addresses the reported holders/waiters race at its source.

The wait is bounded by QUEUE_SETTLE. If holders never fill the slots, the phase continues to its existing queue/overflow/drain evidence checks rather than claiming success from the seating loop: `report.rs:203–216` rejects a queue below 1,000, incorrect overflow, or incomplete disconnect drain. No production admission or shutdown behavior is modified, no benchmark assertion is weakened, and holder/waiter cleanup remains intact. Static correctness accepted; no fresh performance result is claimed.

## Proof limits

Static review only: no cargo, nextest, mise, build, test or benchmark execution. Supplied prior evidence records the focused remote refusal/blocked-reader/drop journeys (3 passed), dispatcher classifier (1 passed), peer suite (11/11), redux (872/872), Scribe (20/20), and server (26/26); these are attributed evidence, not fresh executions. The lifecycle journey source proves retained-root replay/readback and later explicit publication; ordinary idle publication has a separate journey, while restoration/publish-tick ordering is source proof. R6 does not require a manufactured runtime test for unchanged executable behavior.

HEAD remained `8955e75b71ded9d39daf7649a0c985be0803e266`. Only this assigned report was written. No material proposed finding remains.
