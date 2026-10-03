---
id: TASK-015
kind: implementation
status: blocked
spec: SPEC-verified-change-contract
spec_revision: 59
requirements: [REQ-077, AC-014]
depends_on: []
---

# Insert Eval runs in Scribe's batch-fence transaction (blocked)

## Outcome and Value

REQ-077 requires Scribe's batch-fence transaction for a
`vala.eval.observations` batch to insert the matching `verifier_runs` rows
through one `wyrd-sql` statement reached through the `vala-sql` re-export, so
an acknowledged observation always has its runs, and AC-014 requires proof
that both commit in the same transaction.

## Blocker — SPEC_REVISION_REQUIRED

The required mechanism is a cross-crate transaction, which a live repository
authority forbids:

- `architecture/v1/00-foundations/sql-foundation.md` (Transaction Rules):
  "Cross-crate transactional coordination is not supported. Cross-crate work
  does not extend a transaction by importing another crate's private query
  modules. A cross-owner durable effect uses its declared committed handoff
  and idempotent consumer semantics."
- `crates/vala/vala-sql/src/lib.rs` documents that `vala-sql` "does not extend
  Wyrd write transactions or call Wyrd query modules for cross-crate
  transactional coordination", enforced by the live checks
  `tests::vala_sql_does_not_call_wyrd_query_modules` and
  `tests::cross_crate_transaction_boundary_is_documented`.

Two semantics are also unspecified: a WAL batch restored after a crash between
its commit and its fence has no fence transaction to insert runs in
(`resolve_replay` is read-only), and a resend whose slices the memtable already
holds skips the fence entirely.

Implementation needs explicit approval to amend the SQL foundation and the
`vala-sql` boundary (with its two checks) for this statement, and a decision
on the replay and dedup paths. Until then the existing post-acknowledgement
enqueue stays in place.

## Authority Links

- [Approved spec revision 59](../spec.md): REQ-077, AC-014.
- `architecture/v1/00-foundations/sql-foundation.md`.
