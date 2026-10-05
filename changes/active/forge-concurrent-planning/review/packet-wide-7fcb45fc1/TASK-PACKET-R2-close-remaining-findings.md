---
id: TASK-PACKET-R2
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 12
parent_task: packet-wide
remediates: [FIND-TASK-001-1, FIND-TASK-002-2, FIND-TASK-005-R1-3, FIND-PACKET-1, FIND-PACKET-6]
---

# Close the remaining packet findings at their owners

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`, revision 12.
- Task index: `changes/active/forge-concurrent-planning/tasks/README.md`.
- Review verdict: `review/packet-wide-7fcb45fc1/verdict.md`.
- Original base, excluded: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`.
- Remediation base, excluded: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`.
- Reviewed candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`.
- Pinned RisingWave reference: `e23ddf952c3e6ebc03cc254789e84d1179cfacae`.

## Outcome

Close the five validated findings without adding another leader, lifecycle,
coordination protocol, or persistent state owner. Fix each defect once at the
owner shared by every affected caller: `ForgeLeadership`, the consumed fork
surface, Oracle cut acquisition plus the existing prepared expiry claim, and
the packet's authoritative documentation.

## Required remediation

### 1. Linearize leader use and end failed terms synchronously

**Findings:** `FIND-TASK-001-1`.

Keep the fix on the existing `ForgeLeadership` owner.

- Make handler validation and its synchronous schedule operation one
  slot-guarded use. `set_held` must not replace or revoke the selected term
  between validation and `accept`, `serve_pull`, or `serve_report`.
- Make the scheduler's existing RAII lifetime synchronously remove and revoke
  the local held term on every exit, cancellation, drop, or unwind before the
  same-pod restart backoff begins.
- Preserve normal best-effort asynchronous SQL resignation. Do not add a
  second lease, scheduler owner, downstream cancellation checks, or a
  process-wide failure path.

### 2. Delete the unconsumed fork planning surface

**Finding:** `FIND-TASK-002-2`.

In the pinned `iceberg-rust` fork branch `wyrd/narrow-managed-seam` at
`380a4d0717e1786b95c4aa9f257579af496b3c8c`, delete the boundary that no Wyrd
production caller consumes:

- unused `NonCommittingCompaction` planning/accessor methods and the stored
  context needed only by them;
- the test-only second entry point `Compaction::plan_compaction_with_report`.

Keep the production-used `NonCommittingCompaction::new` and `rewrite`,
`CompactionPlanner::plan_compaction_with_report`, governed execution and spill,
cancellation/drain, selection reports, and loose outputs. Commit the narrowed
fork, pin Wyrd to that immutable commit, and update only the dependency and
lockfile entries required by the new pin.

### 3. Use the prepared snapshot-expiry claim as the cut barrier

**Finding:** `FIND-TASK-005-R1-3`.

Implement revision 12's approved rule with existing owners and state.

- Bound the snapshot-expiry exclusive authority scope by the Forge lease TTL.
  If no catalog commit was submitted, roll back and release it. If submission
  occurred but acceptance is unknown at the bound, retain the existing
  `vala.forge_snapshot_expiration_claims` rows in `Prepared`, release table
  authority, and leave reconciliation as their owner.
- In the existing one-statement `vala.oracle_acquire_table_cut`, after taking
  the requested tables' shared authority and before returning any pointer or
  inserting the active-read claim, refuse the acquisition when any requested
  `table_uid` has an unresolved prepared snapshot-expiration claim.
- Surface the stable
  `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` error through the existing typed
  catalog error boundary.
- Remove the barrier only through existing reconciliation after it establishes
  the stable old or new catalog pointer. Do not add a table, claim type,
  protocol, polling loop, retry loop, reader epoch, IO gate, advisory lock, or
  per-query session.
- Keep the proven-unreachable object-deletion TTL exception unchanged; it does
  not change a catalog pointer and needs no cut barrier.

The migration
`crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql`
is unshipped and may be edited in place.

### 4. Correct the Analytical ownership documentation

**Finding:** `FIND-PACKET-1`.

Replace only the stale ownership paragraph in
`crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs`. It must
state that the leader stream owns the complete Analytical graph lifetime and
revokes followers before releasing its active-read claim; the supervisor only
indexes and routes work and may retain failed capacity residue. Already
consumerless remote IO may finish after revocation under the approved rule.

### 5. Repair packet authority metadata

**Finding:** `FIND-PACKET-6`.

- Make `tasks/README.md` name revision 12 as the current approved authority.
- Add `spec_revision: 6` to
  `tasks/TASK-004-compaction-defaults-and-type.md`.
- Preserve every other task's historical derivation revision.

## Acceptance criteria

| Finding | Required result |
|---|---|
| `FIND-TASK-001-1` | Every notify, pull, report, promotion, and maintenance effect is linearized with the live local term; scheduler exit or unwind revokes and removes that term before restart backoff. |
| `FIND-TASK-002-2` | The pinned fork contains only the planning and rewrite surface consumed by Wyrd production code, with no second test-only planning entry point or state retained solely for deleted accessors. |
| `FIND-TASK-005-R1-3` | An acceptance-unknown prepared snapshot expiry releases exclusive authority by the lease bound but blocks new cuts for its table until reconciliation proves one stable pointer; ordinary cuts and the object-delete exception remain unchanged. |
| `FIND-PACKET-1` | Module documentation states leader-stream ownership and no longer attributes query lifetime to the supervisor. |
| `FIND-PACKET-6` | The index names revision 12 as current authority and every task records its actual derivation revision. |

## Required proof

1. Deterministically pause each Forge handler after selecting its term, race
   revocation or replacement, and prove the operation either completes before
   revocation or refuses without a later schedule effect. Force scheduler
   failure and panic after acquisition; during restart backoff prove the old
   pod refuses notify, pull, and report, a standby can acquire, and the rebuilt
   scheduler must contend for the current term.
2. In the fork, run `cargo fmt --check`, workspace Clippy with warnings denied,
   and workspace library tests. In Wyrd, rerun the managed compaction focused
   tests and the Forge journey against the new immutable pin.
3. Add one production-shaped integration test that pauses after snapshot-expiry
   catalog submission with acceptance unknown, crosses the lease TTL, and
   proves a cut fails with
   `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` while the prepared claim remains.
   Reconcile both the accepted and rejected outcomes and prove a later cut
   succeeds with the established pointer. Also prove an ordinary table with no
   unresolved claim still acquires a cut and tenant isolation is unchanged.
4. Run the documentation and packet-metadata scans for the two documentation
   findings.

Use exact focused `mise exec -- cargo nextest run --locked` commands for every
named Rust test. Then run the fork checks, `mise run verify:bifrost`,
`mise run test:principals:integration`, `mise run codegen:check`, the applicable
boundary checks, and one final `mise run gate` because this remediation crosses
SQL, Vala, dependency, server-lifecycle, and packet boundaries. Do not weaken,
delete, ignore, or allowlist a failing behavioral test.

## Non-goals

- No new coordination table or protocol.
- No background query-lifetime owner.
- No indefinite PostgreSQL table-authority hold.
- No change to the approved object-deletion uncertainty rule.
- No broad fork cleanup beyond symbols made unnecessary by Wyrd's consumed
  seam.
- No production behavior change for the two documentation findings.
