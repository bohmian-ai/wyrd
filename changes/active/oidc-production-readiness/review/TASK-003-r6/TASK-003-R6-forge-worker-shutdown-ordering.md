---
id: TASK-003-R6
title: Preserve dedicated Forge-worker shutdown ordering
status: ready
approved_spec: changes/active/oidc-production-readiness/spec.md (revision 7)
original_task: changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md
base: 63c5bffc93cd2f7b5ed558e610a213efcc34fd49
candidate: ad3b92ad0f326917c731383fd3b57cb7ac6a8c82
finding_ids:
  - FIND-TASK-003-19
---

# TASK-003 R6: Forge-worker shutdown ordering

## Authority and scope

Implement this bounded remediation against approved specification revision 7,
the original TASK-003 packet, R2, the explicitly authorized R3-R5 remediations,
and all three human directions. Reassess the cumulative candidate from base
`63c5bffc93cd2f7b5ed558e610a213efcc34fd49`.

This task closes only `FIND-TASK-003-19`. It preserves the completed production
UI, identity, renewal, logout-chain, migration, and router-only in-process
shutdown behavior. Route it directly to `$wyrd-implement`.

## Issue diagnosis

### FIND-TASK-003-19 — Ordinary shutdown settles Bifrost before a live dedicated Forge worker joins

The R5 implementation correctly fixed router-only in-process teardown by
cancelling `AppState::shutdown_token` and draining Bifrost before the Postgres
fixture is dropped. Without that drain, Oracle role work can outlive the
database and abort the test process.

The selection predicate is broader than the diagnosed topology.
`WyrdTestServer::bind` has a dedicated `BifrostTarget::ForgeWorker` branch that
spawns the production Forge worker, stores its live join handle in
`serve_handle`, stores the shared cancellation token, and then assigns
`Mode::InProcess`. Consequently, `Mode::InProcess` does not mean that the
server has no serve task.

`WyrdTestServer::shutdown` currently cancels the token and, for every
`Mode::InProcess`, calls `Bifrost::shutdown` before it takes or joins
`serve_handle`. The dedicated worker has not marked Forge supervision drained,
so Bifrost rejects graceful drain and enters its abort path. Storage can close
while the worker is still draining admitted work that may require object IO.
The later join cannot restore that ordering, and the ordinary shutdown seam may
log the failure and still return success.

This path is reachable through the supported builder composition that selects
`BifrostTarget::ForgeWorker`, calls `start_bound`, and then calls ordinary
`shutdown`. Existing cluster and load teardown use `shutdown_and_inspect`,
which already joins first, so their behavior is not the correction boundary.

## Intended correction outcome

Router-only in-process servers continue to cancel and directly settle Bifrost
before fixture release. A bound dedicated Forge worker instead receives
cancellation and completes its existing worker/serve-handle join before any
direct role or storage settlement. Ordinary shutdown reports real join failure
through its existing error boundary and does not conceal a pre-join Bifrost
abort.

## Decision-complete recommendation

Keep `WyrdTestServer::shutdown` as the sole lifecycle owner. Select the R5
direct Bifrost drain using the ownership fact already present on the struct:
the absence of `serve_handle`. Router-only `start_in_process` servers have no
serve handle and therefore retain the direct drain. The bound dedicated Forge
worker has a serve handle and therefore follows the existing cancellation then
join path before the harness is dropped.

Update the adjacent rustdoc to describe the no-serve-task condition rather than
equating it with `Mode::InProcess`. Preserve the current cancellation, bounded
join, warning, and final blocking-drop behavior outside this predicate.

This is the smallest root-cause correction because it reuses the exact state
that distinguishes the two topologies. Do not add a new mode variant, trait,
shutdown coordinator, dependency, configuration option, or second lifecycle
owner.

## Constraints and preserved behavior

- Preserve the router-only in-process fix: cancel the shared shutdown token and
  await the existing Bifrost shutdown/abort owner before fixture release.
- Preserve production and ordinary bound API-server shutdown behavior.
- Preserve `shutdown_and_inspect` and all cluster/load teardown behavior.
- Preserve bounded join handling and propagation of a panicked serve task.
- Preserve Bifrost's owner order: Forge supervision quiesces before Oracle,
  Scribe, and storage settlement.
- Keep the correction in `crates/wyrd/wyrd-testing/src/server.rs`; do not move
  test-only lifecycle policy into production server code.
- Keep substantive rustdoc aligned with the actual topology and cancellation
  behavior.
- Do not alter the R5 refresh-chain schema, logout, renewal, security, or UI
  implementation.

## Explicit non-goals

- No new product, protocol, public HTTP, SDK, BFF, database, migration, auth,
  or audit behavior.
- No change to Forge scheduling, admitted-work semantics, retry policy, or
  production shutdown architecture.
- No new test harness, lifecycle enum, generalized state machine, dependency,
  feature, retry, sleep, ignored test, or relaxed timeout.
- No refactor of unrelated `WyrdTestServer` constructors or inspection APIs.

## Acceptance criteria

| Criterion | Finding | Required result |
|---|---|---|
| R6-AC-01 | `FIND-TASK-003-19` | Direct Bifrost drain in ordinary shutdown is selected only when no serve handle exists; `Mode::InProcess` is no longer used as a proxy for that fact. |
| R6-AC-02 | `FIND-TASK-003-19` | A bound dedicated Forge worker is cancelled and joined before required Bifrost role/storage settlement; ordinary shutdown does not trigger the pre-join abort path. |
| R6-AC-03 | `FIND-TASK-003-19` | Router-only in-process shutdown still settles Bifrost/Oracle before the Postgres fixture is released, so the original forced-database-drop abort remains fixed. |
| R6-AC-04 | repository rule | Adjacent rustdoc accurately describes the no-serve-task boundary, ordering, failure, and fallback behavior. |
| R6-AC-05 | regression boundary | Production UI behavior, R5 session-chain-only logout, all three human directions, and `FIND-TASK-003-1` through `FIND-TASK-003-18` remain closed. |

## Focused proof and broader verification

Add focused lifecycle coverage using the existing test-server builder and
Forge controls:

1. Start the existing bound dedicated Forge-worker topology.
2. Ensure work is admitted so worker cancellation has a real drain obligation.
3. Invoke ordinary `WyrdTestServer::shutdown`.
4. Prove the worker completes its cancellation drain before required storage
   is released and that no pre-join Bifrost abort occurs.

Retain or add one focused router-only in-process shutdown case proving the
shared shutdown token is cancelled and Bifrost/Oracle settles before fixture
release. Use existing fixtures and lifecycle observation mechanisms; do not
create a new harness solely for these checks.

Run every newly named Rust test with its exact `mise exec -- cargo nextest run`
selector and required repository-managed environment. Then run the narrowest
existing `mise` tasks that cover `wyrd-testing`, Wyrd server shutdown, and the
Bifrost lifecycle touched by this shared owner. Finish with `mise run fmt`,
`mise run lints`, and `git diff --check`. Do not rerun unrelated identity or UI
lanes unless the implementation changes those surfaces.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R6-AC-01 | `WyrdTestServer::shutdown` (`crates/wyrd/wyrd-testing/src/server.rs`) runs the direct Bifrost drain under `if self.serve_handle.is_none()`. `Mode` is no longer consulted. | Source; focused tests below | PASS |
| R6-AC-02 | A dedicated Forge worker keeps its serve handle, so shutdown only cancels and joins it. | `owner_inspection::dedicated_forge_worker_shutdown_drains_its_claim_before_storage_settles`: the worker is bound and held after a durable claim on a seeded task. After ordinary `shutdown()`, its storage owner is still `Open`, the claim was released to `retryable`, and the observer recorded no errors. **RED** against the `Mode::InProcess` predicate: storage was `Closed`, with trace `in-process Bifrost drain fell back to abort ... Forge supervision did not join before shutdown`. GREEN after the fix. | PASS |
| R6-AC-03 | The router-only in-process path is unchanged: it cancels the token and settles Bifrost before the fixture drops. | `owner_inspection::router_only_shutdown_settles_bifrost_before_fixture_release`: the token is cancelled and storage is `Closed`. **RED** with the direct drain disabled (`shutdown cancels the shared token`); GREEN with it restored. | PASS |
| R6-AC-04 | The adjacent rustdoc covers the no-serve-task condition, the forced-drop abort it prevents, the abort fallback, and why a server with a serve task is only cancelled and joined. | Review of the rustdoc | PASS |
| R6-AC-05 | No production, identity, UI or R5 source changed. The diff is limited to `server.rs` (predicate and rustdoc) and two new tests in `tests/bifrost/server/owner_inspection.rs`. | `git diff --stat` | PASS |

Callers checked: bound servers that run `cancel_and_join_for_test` and then `shutdown()` no longer have a serve handle, so they now go through the direct drain as well. They stay green with no abort fallback logged:
- `published::published_cache_pruning_and_shutdown_are_production_governed`
- `published::expired_process_shutdown_aborts_storage_and_returns_failure`
- `resilience::a_pending_invocation_audit_append_drains_before_shutdown_completes`

The cluster and load teardown seam, `shutdown_and_inspect`, is untouched.

Non-goals stayed excluded. There is no new mode variant, trait, coordinator, dependency, sleep, retry, ignore or relaxed timeout. The two new tests carry the binary's existing journey-lane `#[ignore]` marker, and the lane runs them with `--run-ignored=all`.

Verification, all with `CARGO_TARGET_DIR` set to the shared target:
- The focused `mise exec -- cargo nextest run --locked -p wyrd-testing --test server --test oracle --test gateway -P journey --run-ignored=all -E '<the five exact tests above>'`, under `scripts/postgres/with-test-postgres.sh`, exited 0 with 5 of 5 passed.
- `mise run fmt` and `mise run lints` exited 0.
- `mise run test:wyrd` exited 0: 2348 passed.
- `mise run test:bifrost` exited 0: all 9 lanes passed, including the wyrd-testing `server`, `oracle` and `forge` journey binaries.
- The Rust gateway journeys (`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run test:gateway:native:inner'`) exited 0.
- `git diff --check` exited 0.

Fix commit: `a14896e34`.
