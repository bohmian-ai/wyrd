# TASK-003 R6 focused follow-up review

## Immutable subject and uncertainty

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Question: whether `SYSTEM-R6-001` is a reachable regression or whether bound
  dedicated Forge workers retain their prior serve-task-first shutdown ordering.
- Candidate remained fixed at the stated object throughout this review.
- The repository has no `.codegraph/` directory, so ordinary source navigation
  was used.

## Source paths inspected

- `crates/wyrd/wyrd-testing/src/server.rs`: `WyrdTestServer`, `Mode`,
  `shutdown`, `shutdown_and_inspect`, `restart_bound`, `bind`,
  `start_in_process`, `start_bound`, `with_forge_process_role_for_test`, and
  `with_bifrost_target_for_test`.
- `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs`: dedicated-worker
  descriptors, `build_node`, `stop_node`, `shutdown`, and
  `shutdown_and_inspect`.
- `crates/wyrd/wyrd-testing/src/load/matrix.rs`: cluster teardown caller.
- `crates/wyrd/wyrd-server/src/boot/mod.rs`: `spawn_forge_worker` and boot
  rollback ordering.
- `crates/wyrd/wyrd-server/src/app/mod.rs`: production dedicated Forge-worker
  supervision.
- `crates/wyrd/wyrd-server/src/app/server.rs`: bound server supervision,
  `mark_supervision_drained`, and ordered Bifrost shutdown.
- `crates/wyrd/wyrd-server/src/state.rs`: `Forge::begin_shutdown`,
  `supervision_drained`, `Bifrost::shutdown`, and abort behavior.
- `crates/vala/vala-bifrost-redux/src/forge/worker.rs`: worker cancellation and
  admitted-work drain contract.
- `crates/wyrd/wyrd-server/tests/pg_router_smoke.rs`: target-selection caller.
- `architecture/bifrost-design.md`, `AGENTS.md`, and the TASK-003/R5 authority
  and implementation diagnosis.

## Evidence resolving the conflict

The two reports describe different shutdown seams. Bound dedicated Forge
workers do retain serve-task-first ordering through
`WyrdTestServer::shutdown_and_inspect`: it cancels the token and joins
`serve_handle` before inspecting or draining sibling state
(`server.rs:969-1009`). `WyrdTestCluster::stop_node` and cluster shutdown use
that seam (`cluster.rs:2097-2110, 2393-2437`), including the dedicated-worker
and load-matrix topologies. Those existing cluster callers are not regressed by
the R5 change.

The ordinary `WyrdTestServer::shutdown` path is nevertheless independently
reachable through the public supported composition
`builder().with_bifrost_target_for_test(BifrostTarget::ForgeWorker).start_bound().shutdown()`.
`start_bound` first constructs an in-process server and calls `bind`
(`server.rs:4733-4739`). The dedicated-worker branch of `bind` then:

1. spawns the production `ForgeWorker::run` future;
2. stores it in `serve_handle`;
3. stores the shared cancellation token; and
4. deliberately assigns `Mode::InProcess`

at `server.rs:3551-3572`. Therefore `Mode::InProcess` does not imply that no
serve task exists.

The candidate's new `shutdown` branch tests only that mode
(`server.rs:767-783`). It cancels the token, then calls `Bifrost::shutdown`
before taking or joining `serve_handle`. The dedicated branch never calls
`Forge::mark_supervision_drained`; only `BoundServer::run` does that after its
supervised set joins (`app/server.rs:748-782`). Consequently
`Bifrost::drain_selected_owners` deterministically observes
`supervision_drained == false`, returns `"Forge supervision did not join before
shutdown"`, and `Bifrost::shutdown` invokes `abort_selected_owners`
(`state.rs:1990-2001, 2015-2020, 2071-2089`). That abort begins Forge shutdown
and aborts the storage owner before `WyrdTestServer::shutdown` reaches the
worker join.

This ordering conflicts with the worker contract: cancellation stops new
claims but drains already admitted work before `ForgeWorker::run` returns
(`forge/worker.rs:2042-2089`), while Bifrost documents storage as the last owner
because admitted role work may still need object I/O (`state.rs:1967-1982`).
The ordinary seam then logs the forced-abort error, joins the worker if it
settles within two seconds, and can still return `Ok(())`.

The persistence report's statement that “Bound mode retains its production
serve-task drain” is correct for ordinary API-bound servers and for cluster
`shutdown_and_inspect`, but does not cover this explicit exception where a
bound dedicated worker is represented as `Mode::InProcess`. The standards
report did not trace that exception.

## Proposed finding

### FOLLOWUP-R6-001 — Direct in-process drain precedes a bound dedicated worker join

- **Status:** confirms `SYSTEM-R6-001`, with narrowed blast radius.
- **Classification:** `REGRESSION`.
- **Violated obligation:** The R5 harness correction must settle router-only
  in-process Bifrost roles before fixture release without moving role/storage
  teardown ahead of a live Forge worker's cancellation drain.
- **Location:** `crates/wyrd/wyrd-testing/src/server.rs:771-778`, produced by
  the dedicated branch at `crates/wyrd/wyrd-testing/src/server.rs:3557-3572`.
- **Reachability:** The faulty ordering is executable through the public
  builder/start-bound/shutdown path above. No checked-in caller currently uses
  ordinary `shutdown` for a bound dedicated worker: cluster and load callers
  use `shutdown_and_inspect`. The finding therefore does not invalidate current
  dedicated-cluster test results, but it is not a hypothetical state or an
  unreachable branch.
- **Observable consequence:** If ordinary shutdown runs while a dedicated
  worker has admitted work, Bifrost aborts storage before the worker join that
  is supposed to drain that work, logs the lifecycle failure, and may still
  report successful harness shutdown. The harness can manufacture a failed or
  incomplete Forge attempt and conceal the teardown failure.
- **Smallest correction:** Gate the candidate's direct Bifrost drain on
  `self.serve_handle.is_none()` rather than `Mode::InProcess`. That is the
  existing ownership fact the new branch needs: router-only in-process servers
  have no serve task, while the bound dedicated worker does. Keep cancellation
  and join ordering otherwise unchanged; add no mode variant or lifecycle
  abstraction.
- **Focused proof:** Reuse the existing bound `ForgeWorker` builder and Forge
  controls in one focused lifecycle test. Arrange admitted work, invoke ordinary
  `WyrdTestServer::shutdown`, and prove the worker settles through cancellation
  before its required storage is released. Retain the existing router-only
  in-process shutdown coverage that reproduced the Oracle/database-drop abort.

## Resolution

**RESOLVED**

`SYSTEM-R6-001` is source-confirmed, but its claim that existing dedicated
Forge cluster journeys use the faulty seam is too broad. The regression is in
the supported ordinary shutdown composition; current cluster and load teardown
remain on `shutdown_and_inspect` and preserve worker-join-first ordering.
