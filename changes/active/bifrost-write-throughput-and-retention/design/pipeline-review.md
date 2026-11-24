# Pipeline design review

Status: exploring — recommendation only, 2026-11-23.

Reviewed `group-commit-pipeline-proposal.md` as the requested code-pipeline
proposal. Static inspection only; no production edits or experiments.

## Evidence

Only [l1-baseline-74034459d.md](l1-baseline-74034459d.md) is benchmark
authority for this review. Disregard older stage budgets, comparisons and
projected gains. The aggregate baseline does not isolate group occupancy,
WAL or SQL waits and cannot establish the benefit of overlap.

Inspected `crates/vala/vala-bifrost-redux/src/scribe/shards.rs`: `run`
(1449), `process_group` (3598), retained post-commit retry, rotation,
`commit_batch_control_fences` and `write_group`. Governing authority is
`architecture/bifrost-design.md`, Scribe append, recovery and receipt handoff.

## Assessment and refinements

The core is plausible: one ordered WAL producer, two bounded groups, ordered
fencing and independent earlier ACK completion. It is not yet decision-complete.

- **Failure progression:** “N+1 fences after N settles successfully” leaves
  known refusal of N without a continuation rule. Distinguish terminal refusal,
  partial tenant acceptance, ambiguous SQL and accepted-but-not-visible state.
  Preserve accepted portions; retain ambiguous ownership and block subsequent
  fencing until reconciliation or restart. The current retained ambiguity slot
  and assertions assume one serial owner.
- **Rotation:** make rotation a dispatch barrier. Project N's pending memory
  alongside N+1; if rotation is needed, settle N, close/freeze through the
  existing owner, then dispatch N+1. Selective sealing, pressure and shutdown
  also need serialization against dispatched work. Preserve FlushAll semantics.
- **Ownership:** retain both group states and leases in one shard coordinator.
  Reuse the WAL IO lane and tenant SQL owner. Service N completion while N+1 IO
  runs; do not hide it behind synchronous preparation or a join barrier. Never
  drop pending IO as a cancellation strategy. Two slots bound count, not bytes.
- **Identity and collection:** keep duplicate identities queued until their
  predecessor resolves, then reuse retained-version/conflict handling. Collection
  must service predecessor completions rather than wait for clients whose next
  request depends on that ACK. Preserve hierarchical fairness and byte bounds.
  The suggested soft target and interval are hypotheses, not established optima.
- **Retention:** synced N+1 remains provisional. Its progress cannot advance
  the recovery-safe frontier across unresolved ownership. Keep memory charged
  until actual ownership transfers or settles; reuse existing admission.

## Recommended flow and proof

Existing scheduler → complete generation projection → ordered WAL write/sync
→ N fencing while N+1 writes only when identity, rotation and accounting permit
→ apply N resolutions, insert accepted rows, publish visibility and ACK
→ N+1 fencing after N reaches a safe terminal state.

Proposed proof: hold N+1 IO and observe N's ACK/visibility; hold N fencing and
prove N+1 stays invisible; crash/replay at durable boundaries; exercise duplicate
and contradictory retries across groups/pods, partial tenant acceptance,
ambiguous SQL, insertion failure, rotation, pressure, shutdown and receipt
expiry. Use real SDK/server/Postgres journeys and existing recovery owners.
These tests were not run. Later performance comparisons must use the designated
baseline; no improvement is established by this review.

Adoption changes the selected serial design in `spec.md` (serial durability at
line 203 and the earlier pipeline exclusion at line 220). Reconcile design and
spec if the maintainer selects overlap. This review does not approve adoption
or reopen settled retention defaults and cleanup mechanisms.
