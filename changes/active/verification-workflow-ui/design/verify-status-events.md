# Queued execution status events for Verify

Status: finalized — approved server supplement to TASK-003, spec revision 4.

## Problem and outcome

Verify must read queued execution lifecycle status through `bifrost.query`,
without per-execution inspection requests. The table already has a scalar
`status` column. Today's checkpoint writer fills it with only `running` or
`terminal`; claim, retry and cancellation do not independently publish their
queue status. The list therefore cannot read those facts from Bifrost.

Add one status event at each requested transition, through the process Scribe
outbox. For example, cancelling an unfinished execution still cancels its queue
row, and its existing cancellation owner also stages a row with `event = status`
and `status = cancelled`. This is one server capability, with no UI work in this
supplement.

## Row contract

Use the existing `bifrost.verification.execution_events` definition:

| Column | Value |
|---|---|
| `event` | `status` |
| `status` | The `VerificationExecutionStatus` snake-case word |
| `event_id` | A new event UUID, retained across outbox retries |
| `execution_id`, `verifier_card_uid` | The affected execution's identities |
| `occurred_at` | The transition's recorded occurrence time, preserved through staging and retries |
| `sequence` | `0`; this existing required column has no ordering meaning for status events |
| `value` and other nullable content/correlation columns | null |

Normal managed tenant and subject/actor attribution still applies. The schema
requires `sequence`, so it cannot be null; using zero avoids allocating checkpoint
revisions or introducing a new sequence source. The list orders status events by
`occurred_at`, not `sequence`.

## Connected implementation

1. Keep each queue mutation in its existing transaction and owner. Prepare the
   status event from that transition's outcome and known execution identity.
2. Stage only after the existing commit succeeds, and only if the mutation
   applied. A rejected stale lease, repeat cancellation or rollback stages no row.
3. Use one shared status-row construction path and a narrow status payload on
   the existing `ScribeWrite` owner. Route it through the existing ordinary row
   projection and shared frames for `ScribeTable::ExecutionEvents`. This small
   payload addition is needed because the current execution-state payload owns
   a checkpoint, while the existing generic fenced payload gives each event its
   own frame. Neither should be changed to mean a status event.
4. Preserve completion group commit: `Completions` still calls `complete_many`,
   commits once for the tenant batch, and replies with each settlement. The
   runner stages `completed` only for a successful committed settlement. Keep
   the existing single-settlement fallback using the same status staging rule.
5. Scribe batches status rows through its ordinary shared frame path, with its
   existing retry identity and loss accounting. Staging adds no wait for Bifrost
   acknowledgement, no per-event PostgreSQL transaction and no separate flusher.

| Existing transition | Status event | Staging owner |
|---|---|---|
| Successful claim | `running` | Runner claim owner, after its claim transaction commits |
| Retry scheduled | `retrying` | Runner settlement owner |
| Park for deferred input or approval | `waiting` | Runner settlement owner |
| Ordinary release/defer after a claim | Resulting `pending` or `retrying` | Runner settlement owner |
| Successful completion, including grouped completion | `completed` | Runner, after committed settlement |
| Termination or exhausted retry budget | `timed_out` or `errored`, matching the actual outcome | Runner settlement owner |
| Cancel unfinished execution | `cancelled` | Existing cancel operation, after commit |
| Cancel unfinished executions during principal revocation | One `cancelled` row per affected execution | Existing revocation operation, after its transaction commits |

`waiting` is the requested display status for a successful park. It is already
a `VerificationExecutionStatus` variant, but is deliberately not stored as a
PostgreSQL queue status. Emit it from the existing park outcome; do not change
the queue schema or checkpoint. A later claim emits `running` again.

Bulk principal cancellation currently returns only a count. Extend that same
UPDATE with `RETURNING` for the affected execution/Verifier identities and
subject/actor attribution; carry those records through the existing revocation
result to its commit owner. This changes an existing statement's result, adding
no statement, read query or database round trip. It is the minimum data needed
to produce the explicitly requested per-execution events. Cover the existing
admission-time `cancel_principal` caller too, so it does not discard those records.

Capture occurrence time when the transition is applied, rather than when its
outbox write eventually flushes. Do not re-read PostgreSQL to construct an event.

## Reads and recovery

The consumer contract is newest row per execution with `event = 'status'`,
ordered by `occurred_at`. Checkpoint and `github_delivery*` rows do not participate.
Initially pending queued work needs no event. Direct executions are outside this
supplement. Changing the UI query remains TASK-003 work, not part of this server
supplement.

Recovery continues to select `event = 'checkpoint'`, using the same checkpoint
revisions and PostgreSQL coordination. Status events neither authorize another
claim nor restore execution state. Cancellation already fences further claims;
retry already schedules work in the queue. Neither mechanism changes.

Process loss may lose staged status rows; the list can consequently be stale.
This is accepted. Add no reconciliation, durable handoff or recovery guarantee.

## Evidence

- [Table definition](../../../../crates/vala/vala-bifrost-redux/src/tables/verification/execution_events.rs): existing `status`, nullable `value`, required `sequence` and normal managed dimensions.
- [Checkpoint builder](../../../../crates/wyrd/wyrd-server/src/verification/results.rs), `ResultPayloadBuilder::checkpoint`: existing status is `running` or `terminal`; leave it unchanged.
- [Queue](../../../../crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs): claim/settlement/cancellation statements, `cancel_principal` count result, and admission-time cancellation caller.
- [Runner](../../../../crates/wyrd/wyrd-server/src/verification/runner.rs), `settle`/`settle_alone`: stale leases settle nothing; park uses queue release.
- [Completion group commit](../../../../crates/wyrd/wyrd-server/src/verification/completions.rs), `Flusher::commit`: `complete_many` commits before replying.
- [Scribe outbox](../../../../crates/wyrd/wyrd-server/src/scribe_outbox.rs), `ScribeWrite::rows`/`ScribeSink::frames`: ordinary rows share destination frames; checkpoint and fenced writes use their own frame identities.
- [Cancel operation](../../../../crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs), `VerificationControl::cancel`: existing mutation and commit owner; correct its comment claiming the runner records cancellation state.
- [Revocation](../../../../crates/wyrd/wyrd-server/src/auth/revoke.rs) and [transactional auth owner](../../../../crates/wyrd/wyrd-auth/src/revoke.rs): cancellation is part of the existing revocation transaction.

These are source findings, not tested implementation claims.

## Proof

Extend the existing [server verification journeys](../../../../crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs), using real SDK operations,
`WyrdTestServer`, PostgreSQL and public Bifrost queries. Cancel one pending and
one running execution through the domain operation; revoke a principal with
multiple unfinished executions. Hold other executions at running, retrying and
waiting, then let work complete or terminate. Query `event = 'status'` and assert
the newest status per execution is the expected serialized word. Assert nullable
payload fields are null and that repeated cancellation/stale settlement adds no
transition event. Observe running/retrying/waiting while held, before later
completion replaces the latest status. Keep the existing checkpoint/recovery
journeys passing. Use the existing group-commit and outbox tests for the shared
frame path rather than creating a separate test harness.

## Boundaries and handoff

No migration, table, column, endpoint, additional PostgreSQL statement, checkpoint
change, recovery change, reconciliation pass or crash-durability guarantee.
Failed requirement IDs, Verifier-scoped viewers and UI implementation are excluded.
Specification revision 4 records the approved narrow exception to the existing
UI-only boundary. This design does not change architecture or authorize work
beyond REQ-015 and AC-010.

The approved release/defer correction resolves the original wording conflict:
absence means *initially* pending; when previously
claimed work is released without parking, emit its resulting `pending` or
`retrying` word through the same path. Otherwise its last event stays `running`
until another listed transition. Parking still emits the requested `waiting`.

All other private coding choices stay with implementation. The
[TASK-003A](../tasks/TASK-003A-execution-status-events.md) carries this server
supplement and its proof against approved specification revision 4. No production
implementation or passing test evidence is claimed by design/spec approval.
