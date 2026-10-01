# Concurrency and native lifecycle review

Overall: **PASS**. Proposed findings: **none**.

## Immutable subject and boundary

Candidate `9c3d7ecb982435919924dfa8e6930352b27a9b7e`; cumulative base `a7582db587c6170a290760f1741673125612b797`; final-delta parent `23eafa368bca19208faf8311eb7b5421e3660b38`. HEAD remained this candidate during inspection.

Reviewed concurrency, partition ownership, native-versus-wire completion, cancellation, staged-file protection, and query-local scan-counter ownership in cumulative TASK-007/008 and the final delta. Authority: spec revision 20 REQ-014/015, AC-016/017 and INV-001/002/003; original TASK-007 and TASK-008, retained remediation packets and prior verdicts; AGENTS.md, agent rules, spec-driven development, maintainer style, and Bifrost architecture's live-read/resource boundaries. No CodeGraph index exists.

## Source coverage and assessment

| Boundary | Source evidence | Result |
|---|---|---|
| Leader live partitions | `oracle/live.rs:210-230,380-414`: the leaf advertises the session count, deals each route to exactly one partition via `skip(partition).step_by(partitions)`, and preserves the shared accumulator when cloned for execution. Empty partitions open no fragments. | PASS |
| Scribe partition shape and pool | `resources.rs:1248-1280`, `oracle/follower.rs:781-867`, `peer_service.rs:338-356`: pod-derived follower session partitions, fixed-size memory groups including sparse/empty cuts, and staged HotParquetExec use the same session count. The follower keeps its private view over the existing process memory root and cannot spill. | PASS |
| Staged protection and cancellation | `scribe/tail_rpc.rs:562-587,660-690`, `scribe/hot_source.rs:242-264`, `oracle/follower.rs:810-848`, `oracle/exec.rs:2668-2673,2735-2741,2885-3000`: source resolution leases staged authority before filesystem metadata/open; the owning leaf and every partition stream retain the lease. Dropping the stream cancels pending metadata work and releases retained ownership; construction failures release the local lease. Existing registry cleanup remains lease-aware. | PASS |
| Native Arrow completion | `peer_service.rs:358-394`, `oracle/dispatcher.rs:607-658`, `oracle/live.rs:776-797`: production tallies each yielded batch's rows/native bytes and closes with fingerprint-bound completion. The consumer reconciles all three against delivered output. No encoding/hash is invoked on this native route. | PASS |
| Remote completion parity | `peer_service.rs:541-592`, `oracle/live.rs:800-842`: only the gRPC adapter encodes schema/batches and creates the existing wire footer. The decoder retains schema order, encoded bytes, rows, digest, fingerprint and completed-flag checks. | PASS |
| Terminal/drop behavior | `oracle/dispatcher.rs:1943-1987`, `oracle/live.rs:513-602`, `oracle/follower.rs:1431-1467`: open is bounded by query deadline/cancellation; subsequent polls select on the same grant. Still-needed EOF without completion fails. Early plan drop owns no drain obligation and drops the source stream. Availability loss before rows remains degraded; loss after rows and non-availability errors remain terminal. | PASS |
| Final live scan metric | `oracle/exec.rs:291-329,365-415,448-451`, `oracle/live.rs:195-230,530-537,776-842`: one leaf-owned Arc accumulator is shared by partition decoders and pinned by the query collector. A completion contributes only after native or wire validation; the completed guard refuses repeat frames before another fold. Atomic additions avoid lost updates across simultaneous routes. Collector finalization is idempotent and occurs after execution, preserving absent bytes for memory-only reads. Counter ownership contains no staged lease or runtime, so collecting evidence does not prolong source lifetime. | PASS |
| Final harness interaction | The final removal affects the process-cluster module/binary and its dead fixture helper, not the production native dispatcher, follower execution, staged lease, or peer encoder. Retained PeerCluster/WyrdTestServer paths continue to call production service owners. Supervisor per-node attempt counters are test-support observations, not concurrency/admission policy. | PASS |

The tenant-column/tripwire removal leaves the shared file-read producer and its existing cancellation/lease owners intact. Every hot/staged partition proves its retained footer before decoding its row groups (`exec.rs:2950-2971`); the native decoder carries terminal accounting rather than introducing a new row check, transport pool or lifecycle owner.

## Proof and limits

Static inspection included the cumulative and final diffs, producer/consumer bodies, native and gRPC dispatch callers, and focused tests: `scribe_live_sources_keep_the_session_partition_count`, `a_dropped_staged_scan_releases_its_lease_immediately`, `scribe_staged_scan_prunes_non_matching_row_groups`, `native_completion_reconciles_the_delivered_output`, `live_frames_release_batches_incrementally_and_validate_the_footer`, `live_staged_read_reports_scanned_bytes_to_the_query`, `scribe_follower_execution_shape_contract`, and `an_open_live_read_keeps_staged_runs_across_publication`.

Furnished exact-tree evidence is credible corroboration: redux live/follower 19/19, Oracle journeys 40/40, server journeys 26/26, testing library/binaries 61/61, lints clean, and first-class Python/TypeScript lanes passed. The sole full-lane failure belonged to the now-deleted process harness. These are user-furnished results, not execution by this reviewer. No build, test, benchmark or commit was run. The capacity benchmark remains caller-owned.

FIND-007-3, Postgres tenant columns, and the known in-process lost-Scribe deadline behavior are explicitly excluded by standing maintainer decisions; none is re-raised. R2 changes only evidence whitespace and introduces no concurrency/lifecycle effect. Its whitespace-gate closure is the standards/validation review's responsibility.

## Findings

None. No reachable concurrency or native-lifecycle regression requiring correction was established against the approved TASK-007/008 scope.
