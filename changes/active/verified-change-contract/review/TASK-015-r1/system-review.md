# System-resilience review — TASK-015 r1

## Subject and scope

- Immutable candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- Reviewed implementation change: commit `9b560d005`.
- Reviewed integration seam: only the conflict resolution in merge commit
  `c5527627a50dd66a9f53d760d80f59bcb59609f9` against parents
  `ca2950856a37786a5cad25c73c1786c5fa7a1822` and
  `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`.
- Governing obligations: approved specification revision 60, `REQ-077`,
  `REQ-108`, and `AC-014`, plus the original TASK-015 packet.
- Excluded as directed: merge `3f8767a5f`; already-passed TASK-013,
  TASK-014, audit-outbox, and benchmark implementation; generic `Outbox`
  internals except as the unchanged runtime owner TASK-015 now consumes.

Candidate identity was rechecked before writing this report and remained
`9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.

## Deployed paths and ownership

### Eval acknowledgement to durable run

1. Public Eval ingest reaches Gate and awaits Scribe inline. Scribe's returned
   admission is therefore the observation durability boundary
   (`crates/vala/vala-bifrost-redux/src/gate/mod.rs:992-1000`). Gate invokes the
   observation hook only when `admission.first_commit` is true, before the
   request returns, so a suppressed same-batch replay cannot stage another run
   request (`gate/mod.rs:1001-1009`).
2. `ObservationEnqueue` decodes the acknowledged Arrow frame into the exact
   server-resolved subject, record identity, and committed event time, then
   performs one synchronous, non-I/O `stage` for each record
   (`crates/wyrd/wyrd-server/src/verification/observations.rs:90-136`). The
   client does not wait for run creation.
3. TASK-015 now defines `ObservationRunOutbox` as
   `Outbox<ObservationRunSink>` and leaves queueing, per-tenant batching,
   bounded cross-tenant concurrency, retained retry, metrics, and shutdown with
   the already-merged generic owner. The former hand-written queue, writer,
   backoff, stop token, and pending counter are absent
   (`observations.rs:27-65`; commit `9b560d005` deletes the old writer body).
4. `ObservationRunSink::write` opens a tenant-scoped Wyrd transaction, calls
   `VerifierRunQueue::enqueue_observation_batch` once for the tenant batch, and
   commits it (`observations.rs:67-87`). This keeps tenant RLS on the Wyrd
   control-plane connection and keeps Scribe/Vala outside the
   `verifier_runs` transaction. The unchanged queue operation applies the
   runtime-active-owner filter required by REQ-108 and uses the existing
   binding/record idempotency fence.
5. The composition root constructs one Eval run-request outbox per server
   process, retains it in `Bifrost`, and gives Gate a narrow
   `ObservationEnqueue` hook rather than the durable sink itself
   (`crates/wyrd/wyrd-server/src/boot/mod.rs:1101-1133`;
   `crates/wyrd/wyrd-server/src/state.rs:1559-1608,1614-1648`). This is a
   process-local derived-work boundary, matching `architecture/wyrd-design.md`
   and `architecture/bifrost-design.md`.

### Audit and SYSTEM-token merge seam

The `c5527627a` conflict resolution retained both independent process owners:
the existing Eval observation-run outbox and the audit side's single shared
`AuditOutbox` are both built, retained in composed state, and drained. Gate's
write authorization stages allowed and denied decisions without waiting
(`crates/vala/vala-bifrost-redux/src/gate/mod.rs:478-518`), while the merge
kept TASK-013/014's rule that verification result tables and gateway capture
are internal, tokenless writes (`gate/mod.rs:490-529`).

The resolution also kept the audit side's public error removal. The current
public `WyrdError`, `BifrostError`, Gate rejection taxonomy, client mapping,
and HTTP mapping have no audit-unavailable variant/code. The remaining
`AuditErrorCode::AuditUnavailable` value and reserved protobuf name are
historical compatibility for retained audit rows and the removed wire enum
member, not errors that can refuse an operation. The SYSTEM principal remains an attribution and
tokenless read/write identity, but there is no `issue_system_token` path;
`wyrd-auth` explicitly documents and tests that every public grant refuses it
(`crates/wyrd/wyrd-auth/src/issuance.rs:1-12,1248-1305`).

## Failure and recovery assessment

| Failure or transition | What stops | What remains available | State and recovery | Proof assessment |
|---|---|---|---|---|
| Wyrd Postgres is slow or unavailable during an Eval run flush | New `verifier_runs` visibility for the affected queued tenant | Scribe acknowledgement has already completed; the client does not wait; other tenants can use the generic outbox's independent tenant slots; the audit outbox is a separate process owner | The failed tenant batch remains in memory and retries with capped backoff. Recovery commits the retained batch; binding/record uniqueness absorbs repeated requests | Covered through the real Gate/Scribe/Eval journey by `integrated_enqueue_outage_preserves_ack_and_recovers`, and at the sink/shutdown seam by `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown` |
| Ambiguous or repeated Eval sink write | Nothing beyond that tenant's retry interval | Other tenants and unrelated capabilities remain independent | The same record/binding identities are retried; `ON CONFLICT DO NOTHING` makes replay idempotent | The runtime test stages a repeated `r-1` and proves one durable record; the integrated journey replays the Scribe batch and proves one run per binding |
| Graceful server shutdown / rolling replacement | Gate closes and Bifrost/Scribe stop admitting and acknowledging before the Eval outbox fence | Previously accepted Scribe data and already-created runs remain durable | `BoundServer::run` drains Bifrost first, then shuts down the Eval outbox, then the audit outbox (`crates/wyrd/wyrd-server/src/app/server.rs:850-887`). Nothing can stage a new Eval request after the outbox is fenced. Deadline residue is counted and logged by the generic owner | Sink test proves a successful drain and exact nonzero loss reporting at an expired deadline |
| Hard process kill or restart | In-memory Eval requests not yet flushed are lost | Scribe's acknowledged observation remains durable; already-flushed `verifier_runs` remain durable | No startup replay reconstructs the lost in-memory requests. Revision 60 explicitly accepts this recovery point; observable graceful-deadline loss is metered/logged, while an unobservable hard kill cannot report itself | Contractually accepted by REQ-077 and the revision-60 history; not a finding |
| Sink future cancellation at the graceful deadline | The in-flight tenant write may be abandoned | Durable outcomes that committed are protected by the run uniqueness fence; unrelated process shutdown continues | Remaining pending requests are counted lost after the writer joins. A later duplicate cannot create a second run | This is the generic outbox's unchanged shutdown contract; TASK-015 adds no alternate cancellation path |
| One tenant has a permanently failing request | That tenant's later requests remain behind the retained batch | Other tenants can continue through separate per-tenant scheduling; audit is a separate outbox | The batch retries until shutdown or the cause is corrected | Not a TASK-015 finding. REQ-077 requires retention for slow/unavailable PostgreSQL and forbids dropping; it does not define poison-item quarantine, per-item isolation, or permission to abandon a permanent failure. The user-directed classification rule therefore excludes this open risk |
| Binding owner is inactive at flush time | No run is created for that record/binding | The acknowledged observation remains queryable; other active bindings can create runs | REQ-108 explicitly forbids backfill after later authentication; activity is evaluated before creating new work | Preserved by the unchanged `VerifierRunQueue::enqueue_observation_batch` sink target; TASK-015 does not move admission into Scribe or a client |

The four-tenant sink concurrency is bounded, a failing tenant does not share
its queue with another tenant, and the sink acquires tenant-scoped
`TenantConn` state for every write. TASK-015 introduces no cross-tenant retry
state, global lock, alternate audit path, or server-crashing failure policy.
The outbox itself is intentionally unbounded because REQ-077 explicitly makes
retention preferable to dropping acknowledged run requests.

## User-directed test-edit assessment

### Deleted permanently refused pre-phase

Deleting the pre-phase from
`continuous_eval_runs_the_terminal_matrix` does not weaken a revision-60
obligation. That phase installed a data-specific trigger that permanently
refused one run insert, then expected that request never to produce a run while
later requests of the same tenant continued. The generic outbox intentionally
keeps the failed item at the front of its tenant backlog; abandoning that item
would violate the task's no-drop rule, and isolating/quarantining a permanently
invalid item is not required by REQ-077, REQ-108, or AC-014.

The required negative system proof was not removed: the separate integrated
journey still forces the post-acknowledgement insert failure, observes repeated
attempts with no invented run or result, removes the outage, and proves exact
recovery and replay deduplication
(`crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:1154-1310`).
The server integration test separately proves retained count, recovery,
idempotency, graceful flush, and deadline loss reporting
(`crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:2400-2505`). The
terminal-matrix journey remains focused on Eval terminal behavior instead of
asserting an outbox behavior revision 60 does not promise.

### Exact pending-count replay barrier

Replacing the blocked-transaction count with `pending() == 2` is a valid and
more direct proof for the new topology, not a weaker wait. The test holds a
`SHARE` lock on `wyrd.verifier_runs`, so the original request remains pending
in the single in-flight slot for that tenant. Gate stages synchronously before
each successful ingest call returns. Both same-batch replays have
`first_commit == false`, while the distinct sentinel has `first_commit ==
true`; after the sentinel response the stable exact count must therefore be
original plus sentinel, and any incorrectly staged replay would raise it above
two (`eval_verification.rs:1009-1090`; `gate/mod.rs:1004-1009`). After releasing
the lock, the test still proves eight exact runs (four bindings for each of the
two records), the original committed event time, and original-day pruning
(`eval_verification.rs:1092-1151`). It no longer assumes one database
transaction per acknowledged frame, an assumption revision 60 deliberately
replaced with one in-flight write per tenant and batched retry.

## Affected capabilities and availability boundary

- During a run-request sink outage, the affected capability is eventual Eval
  run creation for queued records. Scribe observation durability and the
  ingest acknowledgement remain intact and non-blocking.
- A retained failure delays only that tenant in this outbox. Other tenants,
  audit staging, query, and unrelated server capabilities do not share its
  retry queue, although normal Postgres pool contention remains a bounded
  shared dependency effect.
- A missing audit sink is a boot/composition defect and fails the write request
  internally; an audit commit failure after a real permission decision does
  not fail or delay the request. The conflict resolution preserves that
  boundary.
- Hard-kill loss of unflushed Eval requests is an explicitly approved
  process-level recovery limit, not a service promise silently inferred from
  Scribe durability.

## Findings

No material system-resilience findings.

The permanently failing per-tenant item is a documented residual behavior,
not a violated approved obligation. No remediation may add poison-item drop or
quarantine semantics without a specification decision.

## Verification limits

No Cargo or `mise` command was run in this review, as explicitly directed. I
inspected the immutable diff, current callers and lifecycle owners, the
conflict-only combined merge diff, the approved spec/task, and the applicable
Wyrd, Bifrost, security, operations, and reliability authorities.

Implementer evidence reports the focused Postgres outbox test, SQL suite,
Bifrost server integration suite, complete server journey lane (31/31),
format, lints, and `git diff --check` passing. The evidence names the two
required recovery tests above; this review treats those results as supplied
evidence rather than independently reproduced execution.

## Result

**PASS**

TASK-015 changes the deployed Eval run-request path exactly at the approved
process-local outbox boundary, its failure and shutdown behavior matches
revision 60, the two journey edits retain the required proofs, and the
`c5527627a` resolution preserves both non-blocking audit intent and TASK-013/014
SYSTEM-token removal without a reachable regression in the reviewed scope.
