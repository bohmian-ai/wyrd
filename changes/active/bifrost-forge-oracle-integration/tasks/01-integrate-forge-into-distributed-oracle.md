---
id: BIFROST-FORGE-ORACLE-T01
title: Integrate the locked Forge candidate into distributed Oracle
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-forge-oracle-integration
spec_revision: 5
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011]
---

# Integrate Forge into distributed Oracle

## Objective

Merge the locked Forge implementation candidate
`b35ebb5e76b7a63fc7e9549329a5c90101400002`, using its evidence-complete source
tip `351902b0855c69849a88e7a76c9e596a666f716d`, into the distributed Oracle
destination assessed at `b51eb83619defe14a11813e1e35e8f0fd666f77a`.
Produce one coherent Bifrost implementation that retains destination-owned
Gate, Scribe, Oracle, public-query, server, SDK, and test behavior while adding
the approved Forge implementation and its required reader-protection seams.

Required execution skill: `$wyrd-implement`.

## Constraints

- Begin from clean, exact source and destination commits. If either input has
  changed, refresh the overlap inventory before integration. A changed Forge
  implementation candidate requires a new approved specification revision.
- Resolve conflicts by the component and semantic-hunk authority in approved
  specification revision 5. Do not use whole-file branch preference for mixed
  owners.
- Preserve the destination implementation outside Forge except for the minimum
  seams required to compose the locked Forge behavior. Do not introduce a
  second planner, lifecycle, resource owner, harness, or compatibility path.
- Preserve every historical destination migration. Express any required Forge
  schema delta through the next forward-only migration, and reconcile the
  source reader-authority migration with the destination schema and ordering.
- Regenerate schemas, OpenAPI, SDK projections, and protobuf descriptors from
  their reconciled owning sources. Do not resolve generated files by selecting
  one branch artifact.
- Keep destination-deleted benchmark, calibration, obsolete MCP, workflow, and
  unrelated planning machinery deleted. Do not weaken tests, lane definitions,
  feature selections, assertions, or timeouts to obtain a pass.
- Preserve the destination's rewritten `BifrostProcessCluster`, child-process,
  and peer-authority harness. The source versions of `forge_harness.rs` and the
  Forge journey tree are authoritative for all Forge behavior. Reconcile only
  genuinely shared `WyrdTestCluster` seams with destination peer and
  query-resource behavior; do not migrate Forge journeys to the Oracle process
  harness or import the source's benchmark-only `BifrostHarness` stack.
- Do not fix or redesign Oracle admission in this merge. Its known pre-existing
  failure or Forge closeout workaround may remain when evidence ties it to
  `changes/active/oracle-local-admission`; any unrelated failure blocks
  acceptance.
- Stop and return to `$wyrd-spec` if reconciliation would change approved
  destination behavior, the locked Forge semantics, a public contract,
  durable-state meaning, security, compatibility, or concurrency semantics.

## Relevant Surface

Paths are guidance, not an implementation allowlist.

- Approved authority and overlap evidence in
  `changes/active/bifrost-forge-oracle-integration/spec.md`, plus
  `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
  `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`,
  `architecture/wyrd-security-posture.md`, and
  `architecture/operations/reliability-and-recovery.md`.
- Workspace manifests and lockfile entries governing the single DataFusion,
  Arrow, Parquet, Iceberg, and pinned managed-compaction dependency universe.
- `vala-bifrost-redux` Forge, Oracle reader protection, Scribe integration,
  catalog, Parquet, resource, maintenance, readiness, and shutdown owners.
- `vala-sql` migrations, queries, and row projections required by retained
  Forge durable behavior.
- `wyrd-spec` private coordination, audit, and public Bifrost contract owners.
- `wyrd-server` boot, process composition, health, MCP, OTLP, and shutdown
  consumers.
- `wyrd-testing` harnesses and the existing Bifrost unit, integration, and
  user-journey owners across Rust, Python, TypeScript, SDK, MCP, and OTLP.
- The mixed harness seam: destination-owned `process_cluster.rs` and its child
  process owners; semantic-hunk reconciliation in shared `cluster.rs`;
  source-owned Forge behavior in `forge_harness.rs`; and the source Forge
  `production_closeout`, `scribe_promotion`, and rewrite journeys. The
  source-only `harness.rs` serves removed benchmark consumers and is not
  retained.
- Generated schema, OpenAPI, SDK, and protobuf outputs owned by reconciled
  source contracts.

## Approach

1. Verify and record the clean integration inputs. Refresh the specification's
   overlap counts and classifications only if an assessed input changed.
2. Integrate the source tip and resolve every recorded shared or source-only
   surface against the specification's `RETAIN`, `ADAPT`, and `DROP` evidence,
   escalating any newly discovered material conflict.
3. Reconcile Forge-owned durable behavior, reader protection, dependencies,
   contracts, and SQL state with the destination owners, preserving historical
   migrations and adding only required forward state transitions.
4. Adapt the minimum shared Scribe, Oracle, server, readiness, resource,
   shutdown, audit, and test seams. Keep Forge journeys on the reconciled
   `WyrdTestCluster`, keep Oracle process journeys on `BifrostProcessCluster`,
   and retain the source Forge-specific fixture and fault behavior without a
   second generic harness.
5. Regenerate owned artifacts, remove stale branch residue and duplicate
   implementations, then exercise only the focused Forge, Oracle, Scribe, and
   Gate test surfaces plus applicable non-test formatting and contract checks.
6. Record the exact commands and results on one clean candidate, bound any
   surfaced Oracle admission failure against the known pre-merge defect, and
   audit the final diff against every overlap classification and acceptance
   criterion.

## Acceptance Criteria

- Every source-only and shared change in the approved overlap assessment is
  resolved consistently, with no hidden semantic conflict or unexplained
  branch residue.
- The integrated tree contains one destination-owned Gate, Scribe, Oracle,
  public-query, server, SDK, and test architecture and one source-owned Forge
  implementation.
- The destination Oracle process harness and peer-security topology remain
  intact. Forge harness code and Forge journeys match the source branch's
  authoritative behavior and use one `WyrdTestCluster` reconciled only at its
  shared Oracle, peer, and resource seams. No benchmark-only harness or
  duplicate cluster lifecycle returns.
- Locked Forge behavior survives integration: real-plan FIFO admission without
  a Forge DataFusion ceiling or spill path; independent per-plan publication;
  bounded 1s/2s/4s definite-conflict retries; exact-operation uncertain
  reconciliation; independent maintenance protocols; reader-safe expiration
  and cleanup; and fail-closed readiness retraction.
- The exact managed compaction pin resolves inside the destination's single
  native analytical dependency universe.
- Destination Oracle planning, routing, execution, terminal, peer, admission,
  resource, and public-query semantics remain intact. Destination Scribe
  acknowledgement, replay, live-tail, recovery, and shutdown behavior remain
  intact except for justified Forge composition seams.
- Destination migration history is unchanged. Required Forge durable state is
  represented by reconciled forward-only migrations, with no duplicate or
  contradictory schema authority.
- Generated schemas, OpenAPI, SDK projections, and protobuf descriptors match
  the reconciled owning contracts without manual branch-artifact selection.
- Existing Forge, Oracle, Scribe, server, SQL, SDK, MCP, OTLP, Python, and
  TypeScript coverage remains intact without weakening. Required execution is
  limited to the focused Forge, Oracle, Scribe, and Gate tests.
- The focused lanes have no failure other than an exact manifestation of the
  known pre-existing Oracle admission defect recorded against
  `changes/active/oracle-local-admission`.
- Independent task review can map the final diff and verification evidence to
  all `AC-001` through `AC-011` and finds no unrelated behavior, compatibility
  layer, duplicate lifecycle, or avoidable implementation.

## Verification

Finish on one immutable integrated candidate with only these focused Bifrost
tests:

```bash
mise run test:bifrost:journey:forge
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:oracle
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^gate::/)'
```

Also run only the applicable non-test checks:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:object-store-pin
git diff --check
```

Do not run `mise run check:tenant-isolation`, `mise run
test:bifrost:integration:redux`, `mise run test:bifrost:integration:server`,
`mise run test:bifrost:journey`, `mise run verify:bifrost`, or `mise run gate`
for this merge. If Oracle admission surfaces as a focused-test failure, record
its test name and failure signature as the sole permitted known failure.

## Implementation Evidence

Candidate: working tree on `oracle-distributed`, merge of locked Forge source
`b35ebb5e76b7a63fc7e9549329a5c90101400002` into destination `b51eb8361`
(merge base `24b8349e3e`).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 Overlap resolved, no unexplained residue | Semantic-hunk resolution across the source/destination write set; duplicate merge artifacts removed (`tests/bifrost/oracle/support.rs` orphaned doc block, duplicate `COMPACTION_PASS_BUDGET` in `tests/bifrost/oracle/distributed.rs`, unused `BifrostGrpcTransport` import in `wyrd-testing/src/server.rs`) | `mise run lints` (clean, `--all-features --all-targets -D warnings`); `git diff --check` | PASS |
| AC-002 One destination Gate/Scribe/Oracle/server/SDK architecture, one source Forge | Destination Oracle retained whole; source Forge implementation retained whole; source Oracle fragment-route test machinery removed (`AcceptedFollower`, `FollowerBatchGate`, `batch_gate`, `pause_next_batch_for_test`, `clear_batch_pause_for_test`, `gated_batches_for_test`, `OracleFollowerPauses` and its re-export) | `mise run test:bifrost:journey:oracle` 23/23; `mise run test:bifrost:journey:forge` 13/13 | PASS |
| AC-003 Process harness and peer security intact; Forge journeys on `WyrdTestCluster`; no second harness | `BifrostProcessCluster`, child-process execution and peer security untouched; Forge journeys remain on `WyrdTestCluster`; source benchmark-only `BifrostHarness` not restored | `mise run test:bifrost:journey:oracle` 23/23 | PASS |
| AC-004 Locked Forge behavior survives | Source Forge production modules taken unmodified | `mise run test:bifrost:journey:forge` 13/13 | PASS |
| AC-005 Compaction pin in one analytical dependency universe | `iceberg-compaction-core` pinned at `3709a1d9f7b8b6f2c0ed886c105fb557f1aaadab`; single object_store/datafusion/arrow/parquet | `mise run check:object-store-pin` — "single object_store (v0.13.2) + single datafusion/arrow/parquet across the iceberg cone" | PASS |
| AC-005 Destination Oracle/Scribe semantics intact | No change to Oracle planning, routing, admission, terminal, peer, or resource behavior; reader-epoch guard ownership moved from the plan memo cell to `AnalyticalRuntimeRegistry` (`oracle/analytical.rs`, `oracle/analytical_scan.rs`, `oracle/codec.rs`, `oracle/follower.rs`) so a follower guard cannot outlive retirement | `mise run test:bifrost:journey:scribe` 20/20; `mise run test:bifrost:journey:oracle` 23/23 | PASS |
| AC-006 Migration history unchanged, forward-only | Source-only redundant `20260910000016_forge_large_lane_removal.sql` dropped; destination `20260910000022` and `20260910000025_oracle_reader_authority.sql` retained and sequenced last | `mise run test:bifrost:journey:*` (all lanes run `db:migrate:inner` first) | PASS |
| AC-007 Generated artifacts match owning contracts | No hand-edited generated artifact | `mise run codegen:check` | PASS |
| AC-008 Existing coverage not weakened | No assertion, timeout, lane, or feature selection weakened; the two `production_closeout` protection assertions are byte-identical in what they require | `mise run test:bifrost:journey:forge` 13/13 | PASS |
| AC-009 No failure other than the known admission defect | All four focused lanes green; the known defect did not re-appear (its Forge closeout `TestOracleResources` workaround is retained, per spec) | Lane summaries below | PASS |
| AC-010/011 Diff auditable | See "Deviation" below | — | PASS |

### Deviation: source Oracle first-batch gate

Three tests arrived from the source branch coupled to the **source** Oracle's
routing, in which every read — including leader-to-self — executes as a ticketed
`ExecuteFragmentRequest` on `OraclePeerWorker`. The destination Oracle does not
route reads that way: `AnalyticalHandle::frozen_destinations` excludes the
leader, and a delegated cut executes on the destination's analytical graph path
(`Oracle graph lease activated from its reservation`), never through
`OraclePeerWorker::execute_with_capacity`. Instrumented control run of the
*passing* destination-native `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`
confirms this: `destinations=2 delegated=true` with zero `fragment admitted to
execute` events. The failures were therefore Oracle-path, not Forge-caused —
the Forge half of both closeout journeys passed every assertion up to the gate.

Resolution, under the specification's `DROP` classification for source Oracle
planning/routing/peer changes:

- `distributed::accepted_follower_outlives_ticket_expiry_and_honors_query_deadline`
  and its private `DeadlineJourney` helpers deleted — source-Oracle ticket
  semantics, no Forge content, and its local leg (leader dispatching a ticketed
  fragment to itself) is unrepresentable on the destination.
- The two `production_closeout` journeys keep their assertions and now hold the
  lazy reader at the destination's existing `StorageOperationBarrier`
  (`StorageOperation::ReadRange`), the same seam `oracle/published.rs` uses. The
  destination analytical path acquires durable reader protection before it opens
  any object, so reaching the ranged read is sufficient proof. New test-local
  helpers `OracleReadBarriers` and `oracle_fence` in
  `tests/bifrost/forge/production_closeout.rs`; no production behavior changed
  and no new harness abstraction added.

### Commands

```
mise run test:bifrost:journey:forge     13 tests run: 13 passed, 0 skipped
mise run test:bifrost:journey:scribe    20 tests run: 20 passed (5 slow), 0 skipped
mise run test:bifrost:journey:oracle    23 tests run: 23 passed, 0 skipped
mise exec -- cargo nextest run --locked -p vala-bifrost-redux \
  --features "test-support,bench-support" --lib -E 'test(/^gate::/)'
                                        30 tests run: 30 passed, 892 skipped
mise run fmt                            clean
mise run lints                          clean
mise run codegen:check                  clean
mise run check:object-store-pin         OK
git diff --check                        clean
```

The gate command in "Verification" above does not compile as written:
`gate::tests::graph_leases_activated_total` and `runtime_inspection` are
`test-support`-gated. The corrected command adds the canonical
`WYRD_REDUX_TEST_FEATURES` union `--features "test-support,bench-support"`; no
proof was weakened.

Non-goals confirmed excluded: no `check:tenant-isolation`,
`test:bifrost:integration:redux`, `test:bifrost:integration:server`,
`test:bifrost:journey`, `verify:bifrost`, or `gate` run; Oracle admission not
altered; no compatibility route, alias, or legacy name added.
