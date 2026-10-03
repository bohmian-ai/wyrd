# Concurrency and durability domain review

## Reviewed boundary

- Immutable candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- Task surface: TASK-015's acknowledged Eval observation -> synchronous
  run-request stage -> generic per-tenant outbox ->
  `VerifierRunQueue::enqueue_observation_batch` -> transaction commit path,
  including retry, idempotence, pending/settle, and shutdown loss reporting.
- Merge surface: only the conflict resolution in
  `c5527627a50dd66a9f53d760d80f59bcb59609f9`, compared through its combined
  diff and both-parent deltas. The review checked that the resolution retained
  both the generic audit-outbox staging path and TASK-013/014's removal of
  SYSTEM-token/result-write behavior.
- Explicit exclusions honored: merge `3f8767a5f`, previously passed TASK-013,
  TASK-014, audit-outbox and benchmark implementation, and generic-outbox
  internals except the consumer contract required to assess
  `ObservationRunSink`.

## Authority and source coverage

| Concern | Authority | Source traced |
|---|---|---|
| Post-ack derived work and accepted loss boundary | `architecture/bifrost-design.md` durability and visibility; `architecture/wyrd-design.md` Trigger/Eval outbox contract; spec revision 60 REQ-077 | `gate/mod.rs:999-1009` -> `verification/observations.rs:105-136` -> `wyrd-runtime/outbox.rs:143-227` -> `verification/observations.rs:62-87` |
| Per-tenant batching, retry safety, and exact-once durable effect | REQ-077; TASK-015 Scenarios 1-2 | `wyrd-runtime/outbox.rs:266-440`; `verifier_runs.rs:115-167,1640-1707`; `20260601000029_verifier_runs.sql:120-127` |
| Runtime-active admission and no later backfill | REQ-108; spec Trigger definition | `verifier_runs.rs:1640-1735`; `verification.rs:96-117,521-548`; `pg_verifier_runs.rs:1858-1967` |
| Graceful shutdown and observable loss | REQ-077; AC-014 | `app/server.rs:850-887`; `wyrd-runtime/outbox.rs:189-227`; `pg_verification_runtime.rs:2411-2488` |
| Eval journey proof edits | AC-014; `AGENTS.md` and `testing-workflows.md` user-journey priority | `eval_verification.rs:623-974,1009-1151,1181-1315` |
| Conflict resolution preserves both parents | repository audit and tokenless internal-writer rules | combined conflict hunks in `gate/mod.rs`, `issuance.rs`, `boot/mod.rs`, `state.rs`, `pg_verification_runtime.rs`, and `observe_run.rs` |

## Boundary assessment

The acknowledgement path is correctly ordered. Scribe first earns the durable
batch acknowledgement, Gate invokes the hook only for `first_commit`, and the
hook synchronously stages one item per decoded committed record before the
request returns (`gate/mod.rs:999-1009`, `observations.rs:105-123`). A replay
suppressed by Scribe therefore contributes no second request.

`ObservationRunSink` satisfies the generic outbox consumer contract for
ordinary and uncertain commit failures. It opens one tenant transaction,
invokes one batch enqueue, and commits (`observations.rs:76-87`). The insert is
one `INSERT ... SELECT FROM unnest(...) ... ON CONFLICT DO NOTHING`, and the
partial unique index on `(data_tenant_id, binding_id, input_record_id)` makes a
retry after an unknown commit outcome idempotent without consuming another
ordinal (`verifier_runs.rs:130-167`; migration lines 120-127). The outbox keeps
one in-flight write per tenant, restores failed items ahead of later arrivals,
backs off per tenant, and keeps other tenants moving. Shutdown occurs only
after Gate/Scribe drain, so no normal acknowledgement can race behind the
outbox's one-way shutdown fence (`app/server.rs:873-881`). Deadline remainder
and late staging are counted and logged by the generic owner.

The merge resolution retained both sides' relevant intent. Gate stages audit
decisions on the shared `AuditOutbox` without an audit-unavailable error while
continuing to refuse every public write to internal result tables; boot/state
retain both the audit outbox and Eval run-request outbox; issuance retains the
removal of SYSTEM token mints. No concurrency or durability regression was
found in those resolution hunks.

## Requested journey-edit judgments

### Terminal-matrix refused pre-phase deletion: valid

Deleting the permanently refusing pre-phase from
`continuous_eval_runs_the_terminal_matrix` does not weaken revision 60's
required proof. That pre-phase asserted the old drop/fail-open behavior: a
permanently refused request never created a run. Revision 60 instead requires
a failed flush to retain the request and retry it. Keeping the old pre-phase
would intentionally poison that tenant's retained batch and prevent the later
terminal matrix from progressing.

The required negative and recovery behavior remains at the user-journey tier
in `integrated_enqueue_outage_preserves_ack_and_recovers`
(`eval_verification.rs:1181-1315`): it proves the observation is acknowledged
and stored while run inserts fail, observes repeated flush attempts, proves no
run appears during the outage, removes the refusal, then proves exactly one run
per binding for both records and no activation from the replay. The focused
Postgres runtime test additionally covers retained pending count, idempotent
replay, graceful flush, and deadline loss reporting
(`pg_verification_runtime.rs:2411-2488`).

### Sealed-replay pending-count barrier: valid

Replacing blocked-backend counting with `pending() == 2` follows the generic
outbox contract and is a more direct proof. The held `verifier_runs` table lock
keeps the original request pending; generic per-tenant serialization permits
only that one tenant write in flight. Gate stages synchronously before each
ingest acknowledgement returns, so after the sentinel response the pending
count must be exactly the original plus sentinel. Either replay being staged
would raise the count above two (`eval_verification.rs:1053-1090`). The former
`pg_blocking_pids` assertion depended on the deleted per-frame transaction
shape and could not observe requests waiting in the generic outbox's in-memory
tenant backlog.

## Material proposed finding

### DOMAIN-CONCURRENCY-1 — REQ-108 activity is evaluated at delayed flush time, allowing later authentication to backfill an inactive Eval occurrence

- **Classification:** INCORRECT.
- **Violated obligation:** REQ-108: an inactive occurrence creates no run and
  must not be backfilled after later authentication. The specification defines
  continuous Eval's trigger occurrence as the successfully committed
  observation.
- **Locations:**
  `crates/wyrd/wyrd-server/src/verification/observations.rs:114-122`;
  `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:1678-1693,1709-1734`;
  `crates/wyrd/wyrd-sql/src/queries/verification.rs:96-117,521-548`.
- **Evidence:** the staged `ObservationRecord` freezes only subject, record ID,
  and input event time. When the outbox eventually writes, `accepts_records`
  calls `binding_activity`, whose SQL derives activity from the principal's
  *current* status and `last_authenticated_at` against that flush statement's
  `statement_timestamp()`. No value preserves whether the owner was active at
  the committed-observation occurrence. Thus, if a run write is delayed (for
  example by the required Postgres-outage retry), an observation committed
  while the owner is inactive can remain queued; a qualifying authentication
  before the successful retry makes the current predicate true and the old
  observation then creates a run.
- **Observable consequence:** later authentication can activate historical
  Eval input that REQ-108 says must remain skipped. The existing SQL test proves
  only that an owner stale at the moment `enqueue_observation_batch` executes
  creates no run (`pg_verifier_runs.rs:1906-1941`); it does not cover the
  delayed-flush/later-authentication sequence.
- **Required testable correction:** preserve enough occurrence-bound admission
  evidence on the post-ack request so a later authentication or reactivation
  cannot turn an occurrence that was inactive when committed into eligible
  work, while retaining the non-blocking acknowledgement and current-owner
  check before insertion. Add a focused regression that holds the run request
  in the outbox, makes the owner inactive for the committed observation,
  authenticates/reactivates it before the retry succeeds, and proves the old
  record never creates a run while the next eligible observation does. The
  correction must not introduce per-observation synchronous Postgres IO on the
  acknowledgement path.

## Open-risk classification

The implementer's stated risk that one permanently failing item blocks that
tenant's backlog is **not a finding against TASK-015**. REQ-077 requires
retention and retry when Postgres is slow or unavailable and explicitly
accepts loss only at hard kill or an observed shutdown deadline; it does not
authorize dropping, dead-lettering, splitting, or bypassing a permanently
invalid item. The task also requires one all-or-nothing tenant batch. Requiring
poison-item isolation would add concurrency and durability semantics not
approved by revision 60.

## Verification limits

- Per assignment, no Cargo, mise, or live Postgres tests were run by this
  reviewer.
- Static review covered the current source, TASK-015 evidence, commit
  `9b560d005`, and the `c5527627a` combined/both-parent resolution. The reported
  implementation evidence says the focused SQL and runtime tests, SQL family,
  Bifrost integration/journey lane, format, and lints passed; this review did
  not independently reproduce those runs.
- Generic outbox behavior was inspected only to validate the sink-facing
  contract and lifecycle; its previously reviewed implementation was not
  reopened.

## Result

**FAIL** — the outbox, retry, idempotence, shutdown, conflict resolution, and
the two user-directed journey edits satisfy their reviewed durability
contracts, but `DOMAIN-CONCURRENCY-1` leaves REQ-108's no-backfill behavior
unmet on a delayed Eval flush followed by later authentication.
