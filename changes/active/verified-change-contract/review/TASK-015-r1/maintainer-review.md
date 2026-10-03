# Maintainer review: TASK-015

## Subject

- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`
- Task range: `3f8767a5f..9b560d00569656ee7fdde19a9c76fa55c8d55fbb`
- Additional scope: only the conflict resolution in merge
  `c5527627a50dd66a9f53d760d80f59bcb59609f9`, compared with both parents
  `ca2950856a37786a5cad25c73c1786c5fa7a1822` and
  `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/references/languages/spec-driven-development.md`,
  `architecture/references/languages/maintainer-style.md`, applicable portions
  of `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, and
  `architecture/wyrd-security-posture.md`, plus approved specification revision
  60 (`REQ-077`, `REQ-108`, `AC-014`) and the approved TASK-015 packet
- Verification: implementation evidence records passing focused SQL and server
  tests, the 31-test Bifrost server journey lane, format, lints, and broader SQL
  and server integration lanes. This reviewer performed a read-only source
  review and did not rerun Cargo or mise, as assigned.

The candidate remained at the stated commit during this review.

## Changed-surface coverage

| Surface | Symbols and paths inspected | Owners, callers, tests, and maintainer assessment |
|---|---|---|
| Eval run-request adapter | `verification/observations.rs`: module contract, `OBSERVATION_RUN_WRITER_CONNECTIONS`, `ObservationRunOutbox`, `ObservationRunSink`, `ObservationRunSink::outbox`, `OutboxSink::write`, `ObservationEnqueue`, and `ObservationAck::acknowledged` | The hand-written queue and writer are gone. The remaining types have cohesive roles: `ObservationEnqueue` owns frame decoding at Gate's callback boundary, the generic `Outbox` owns queueing/retry/lifecycle, and `ObservationRunSink` owns the one Wyrd SQL transaction. Names and argument types expose those roles directly. The alias and construction shape intentionally mirror `vala_sql::audit_outbox::{AuditOutbox, AuditSink}`. One documentation gap is recorded below. |
| Composition and lifecycle | `boot::compose_bifrost`; `state::{Bifrost, BifrostComposition, Bifrost::observation_runs}`; `app::server` shutdown drain | Boot constructs one shared Eval outbox, gives Gate a small `ObservationEnqueue` hook, and retains the same outbox for shutdown. The state field/accessor makes that ownership discoverable, and shutdown drains it only after Gate/Scribe can no longer stage acknowledgements. No parallel owner or hidden lifecycle path remains. |
| Durable sink consumer | `VerifierRunQueue::enqueue_observation_batch`, `ObservationRecord`, and `accepts_records` in `wyrd-sql` | The sink calls the existing batch owner through `TenantConn`, then commits at the sink boundary. The called API's record type and docs make subject, record identity, frozen event time, runtime-active filtering, single-statement insertion, and retry idempotence legible. TASK-015 did not change this owner. |
| Runtime proof | `pg_verification_runtime.rs`: `refuse_run_writes`, `observation_records`, and `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown` | The test now drives the same `ObservationRunSink::outbox` construction as production and uses the generic `settle`/`shutdown` vocabulary instead of polling the deleted writer. Its scenarios separately name retained outage work, recovery/idempotence, graceful flush, and deadline loss. |
| Eval journeys | `continuous_eval_runs_the_terminal_matrix`, `sealed_replay_on_a_later_day_activates_once`, `integrated_enqueue_outage_preserves_ack_and_recovers`, and their helpers in `eval_verification.rs` | Removing the terminal matrix's permanently refused pre-phase removes a contradiction with the retained-batch model; the dedicated integrated outage journey still proves post-ACK retention, retries, recovery, exact run counts, and replay suppression. Replacing backend-blocker counting with `observation_runs().pending() == 2` follows the actual owner: Gate stages before acknowledging, `pending` includes queued and in-flight items, and the held table lock prevents the original request from settling. It is a clearer proof that the two replays staged nothing, not a weakened proof. |
| Incidental Rust test cleanup | `sdks/wyrd-sdk-rust/tests/observe_run.rs`: `drift_burst_survives_a_byte_budget_override` | `u32::from(u16)` states the infallible widening conversion and removes misleading `try_from`/`expect` noise without changing the test. |
| Merge resolution: runtime composition | Combined conflict hunks and both-parent versions of `gate/mod.rs`, `boot/mod.rs`, and `state.rs` | The resolved Gate keeps TASK-013/014's tokenless internal result-writer rule and its reduced `record_write_verdict(auth, table)` signature, while staging decisions on the merged `AuditOutbox` and mapping a missing sink to `Internal` rather than the removed audit-unavailable surface. Boot and state retain both process outboxes and give Gate the correct shared audit sink plus the Eval acknowledgement hook. |
| Merge resolution: auth, tests, and declarations | Combined conflict hunks and both-parent versions of `wyrd-auth/src/issuance.rs`, `pg_grpc_ingest_smoke.rs`, the other conflict-listed server tests, `wyrd.v1.proto`, and the conflict-listed architecture files | The SYSTEM-token mint and its tests remain removed, while the audit outbox type and settling steps remain. The gRPC denial test retains the tokenless principal projection and waits for asynchronous audit staging. The proto reserves the removed audit-unavailable enum name rather than reintroducing it. The resolved architecture text consistently describes tokenless internal result writes and non-blocking audit staging. No generated declaration mismatch attributable to the resolution was found. |
| Explicitly excluded | `wyrd-runtime/src/outbox.rs`; TASK-013/014, audit-outbox, and benchmark implementations outside merge-resolution seams | Read only as needed to verify the public `pending`, `settle`, `shutdown`, and cancellation contracts consumed by TASK-015; no internal generic-outbox redesign or previously passed work was reopened. |

## Material findings

### MNT-015-001 — The new sink write omits its cancellation and partial-progress contract

- **Changed location:** `crates/wyrd/wyrd-server/src/verification/observations.rs:67-87`,
  `ObservationRunSink::write`.
- **Governing rule:** `architecture/agent-rules.md` requires every new or
  materially modified Rust item to document async cancellation or partial
  progress when applicable, with missing substantive rustdoc blocking merge.
  `architecture/references/languages/maintainer-style.md` likewise requires an
  async operation's cancellation/partial-progress behavior to be visible to a
  caller.
- **Evidence:** this method opens a tenant transaction, inserts the batch, and
  awaits `commit`. The generic outbox explicitly cancels in-flight sink writes
  when its shutdown deadline expires. The method documents errors and retry
  idempotence, but not what cancellation means before or during commit, even
  though this is the exact boundary at which a maintainer must distinguish a
  rolled-back transaction from an uncertain completed commit and understand
  why repeating `(tenant, binding, record)` is safe.
- **Concrete maintenance cost:** a maintainer changing shutdown or commit
  handling must reconstruct the sink/outbox interaction and idempotency reason
  from separate modules. That makes it easy to incorrectly add a downstream
  guard or assume cancellation proves the transaction did not commit.
- **Smallest testable correction:** add a substantive `# Cancellation` section
  to `ObservationRunSink::write` stating that shutdown may cancel the in-flight
  transaction at the deadline, identifying the possible commit/rollback
  boundary, and explaining that the unique `(tenant, binding, record)` key
  makes a later repeat harmless while the outbox reports deadline-abandoned
  items. No production behavior, new helper, or new test harness is needed;
  verify with the existing documentation/lint lane and the focused outbox test.

## User-directed questions

- **Deleted permanently refused pre-phase:** correct for revision 60 and not a
  weakened proof. A permanently poisoned tenant batch is not the transient
  Postgres outage AC-014 asks the journey to recover from. The retained-batch
  semantics make the old terminal-matrix phase block all later same-tenant
  records. The separate integrated outage journey still supplies the required
  end-to-end proof and recovers after the injected refusal is removed.
- **Exact pending count for sealed replay:** correct and stronger aligned proof.
  It measures the specified outbox owner instead of inferring activations from
  PostgreSQL backend waits, an inference invalidated by one in-flight write per
  tenant. With the table lock held, the original remains pending; after the
  sentinel acknowledgement returns, the original and sentinel are exactly two,
  while either replay incorrectly staged would increase the count.
- **Permanently failing item blocks one tenant:** no TASK-015 finding. Revision
  60 requires failed batches to remain at the front and retry without dropping;
  it requires recovery from slow/unavailable Postgres, but does not define a
  poison-item quarantine or skip policy. Adding one would require new behavior
  rather than maintaining this approved task.
- **Merge intent:** preserved. The resolution combines asynchronous staging on
  `AuditOutbox`, removal/reservation of audit-unavailable error surfaces, and
  TASK-013/014's removal of SYSTEM-token issuance and Gate-based internal result
  writes.

## Uncertain preferences (non-blocking)

- `pg_grpc_ingest_smoke.rs::write_decisions` calls
  `wait_oracle_audit_staged(...).await.expect("audit outbox settles")` without
  asserting that the returned residual is zero. As a diagnostic preference,
  asserting zero would localize a timeout more clearly, but the subsequent
  exact decision assertion already fails if required rows are absent, and a
  shared outbox may retain unrelated work. This is not a material finding.

## Result

**FAIL**

The implementation and merge resolution are structurally clear and preserve
the requested behavior, but `MNT-015-001` is a repository-mandated rustdoc gap
on a new async durable-write boundary.
