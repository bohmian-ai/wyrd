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
