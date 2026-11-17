---
id: TASK-003A
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 4
requirements: [REQ-015, AC-010]
depends_on: []
---

# Record queued execution status for Verify

## Outcome and activation

When an existing owner changes a queued execution's lifecycle status, it stages
one `event = 'status'` row on the process Scribe outbox. Verify can then read
the current recorded lifecycle through `bifrost.query`, without per-execution
reads. Implement one server capability using the
[status-event design](../design/verify-status-events.md), especially Row
contract, Connected implementation, Reads and recovery, and Proof.

Approved specification revision 4 authorizes this narrow server supplement
under REQ-015 and AC-010. No upstream implementation task is needed: the queue, transition
owners, Scribe outbox and test server already exist in the integrated baseline.

Ordinary release/defer records the resulting `pending` or `retrying` status so
an old `running` event does not remain current. Initial pending work needs no row.

## Inputs, owners and output

Inputs are successfully applied queue transitions and their existing execution,
Verifier and subject/actor identities. Stage only after the owning transaction
commits. Rollbacks, stale leases and repeat cancellation with no change produce
no event. Capture occurrence time at the transition and preserve it across
outbox staging/retries, rather than timestamping the eventual flush.

| Existing transition | Recorded status | Existing staging owner |
|---|---|---|
| Claim | `running` | Runner claim path |
| Scheduled retry | `retrying` | Runner settlement |
| Park for deferred input or approval | `waiting` | Runner settlement |
| Ordinary release/defer after a claim | Resulting `pending` or `retrying` | Runner settlement |
| Complete | `completed` | Runner after committed completion |
| Terminate or exhaust attempts | `timed_out` or `errored`, matching the outcome | Runner settlement |
| Cancel unfinished execution | `cancelled` | Cancel operation |
| Principal cancellation | One `cancelled` row per affected execution | Revocation/admission transaction owner |

The retained row uses the existing execution-events schema: `event = status`,
`status` exactly as `VerificationExecutionStatus` serializes, execution and
Verifier identity, an event UUID and `occurred_at`. All other nullable columns,
including `value`, are null. The required `sequence` column uses `0`, with no
checkpoint revision meaning. Normal managed tenant/subject/actor attribution
still applies. Preserve each event identity and occurrence time across retries.

Extend existing owners:

- `verification/runner.rs` and its claim owner publish successful transition
  outcomes. Parking emits `waiting` even though queue release stores
  `pending`/`retrying`; it does not add a PostgreSQL waiting state.
- `verification/completions.rs` keeps `Completions` → `complete_many` group
  commit. Stage `completed` only for an applied, committed settlement, including
  the existing fallback; do not introduce one commit per event.
- `components/verification/service/inspection.rs` stages cancellation after its
  existing commit. Correct the comment claiming the cancelled runner records
  cancellation state; a lost lease still writes nothing.
- `wyrd-sql/src/queries/verifier_executions.rs` returns affected identities and
  attribution from the existing bulk cancellation UPDATE using `RETURNING`.
  Carry that result through `wyrd-auth/src/revoke.rs` to the server's
  `auth/revoke.rs` commit owner. Cover the existing admission-time
  `cancel_principal` caller too. Add no SELECT, database round trip or new
  mutation statement to obtain event data.
- `scribe_outbox.rs` owns status payload projection and delivery. Reuse its
  ordinary shared destination frames, batching, attribution, memory accounting,
  retries and loss accounting. Add only the minimum payload/row construction
  needed there; checkpoint payloads and per-event fenced frames do not provide
  the requested shared status-row batching. Keep helper layout implementation-owned.

Output is a status event in the existing Bifrost table. TASK-003 consumes the
newest row per execution where `event = 'status'`, ordered by `occurred_at`;
it ignores checkpoint/GitHub delivery rows. Its UI query and dashboard proof
remain TASK-003 work, not part of this server task. The server journey below
must exercise that same public query shape against real transition output.
Deterministic selection among equal occurrence timestamps is consumer-owned;
neither `sequence = 0` nor the event UUID promises causal ordering for a tie.

## Acceptance scenarios and evidence

These ordered scenarios implement REQ-015 and AC-010.
For each, add the actual focused command before RED, then record RED/GREEN
results and proof references here. Use the existing SDK → `WyrdTestServer` +
repository PostgreSQL → public Bifrost query journey owner in
`crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs`.

1. **Pending cancellation.** Enqueue and hold work before claim; cancel through
   the public domain operation. Query status events for that execution: its
   newest row is `cancelled`, with the specified identity/null fields, and domain
   inspection agrees. Repeat cancellation and observe no additional transition
   row. Today this fails because cancellation publishes no status event.
2. **Runner lifecycle.** Hold executions at running, retrying, deferred-input
   waiting and approval waiting; query each while held, then let work complete
   or terminate with timeout/error. Each newest status has the exact serialized
   word. Include exhausted attempts and both grouped completion and the existing
   fallback path in focused owner proof. Release/defer previously claimed work
   without parking and observe the resulting pending/retrying status. Today no
   `event = status` rows exist.
3. **Running cancellation and revocation.** Cancel held running work through
   the public operation; revoke a principal with multiple unfinished executions
   through the public revocation operation. Each affected execution lists as
   `cancelled`. Release old workers and confirm stale settlements do not publish
   completion/status rows. Preserve existing cancellation fencing behavior.
   Include focused proof for admission-time principal cancellation. Today the
   queue cancels correctly but the per-execution Bifrost rows are absent.
4. **Existing write and recovery boundaries.** In focused outbox proof, stage
   several status events and assert they share the ordinary execution-events
   frame, preserve per-row attribution and retain event identity on retry. In
   owner proof, a rolled-back mutation stages no event. Keep existing checkpoint
   recovery and tenant-isolation assertions passing; status rows must never be
   selected as checkpoint revisions. These guard the required unchanged paths,
   rather than defining another recovery mechanism.

## Verification

Run Cargo commands sequentially. The existing journey runner is
`mise run test:bifrost:journey:server`; it supplies repository PostgreSQL,
migrations and the server build. During implementation, select the added tests
by their actual names. For the existing recovery regression named here:

```bash
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::queued_subject_authority_survives_restart)"'
```

Run focused inline outbox tests in `wyrd-server` and queue/auth tests for changed
results/callers, with actual selectors and features recorded before their first
run. Run the affected owner lane, `mise run fmt`, `mise run lints`,
`mise run check:tenant-isolation` and `mise exec -- git diff --check`.
Do not run broad `gate` or UI lanes for this server-only task. If implementation
touches a hot path, apply the repository's required wait inventory and full clean
`bench:l1:<engine>` VALID comparison; background runner writes alone are not a
hot-path change.

## Exclusions and limits

No migration, table, column, endpoint, extra PostgreSQL statement or per-row
database transaction. Existing statement result changes are allowed for the
specified event data. No checkpoint/state revision change, recovery change,
reconciliation pass, new flusher or durability guarantee. Status staging never
waits for or fails the operation because of its Bifrost write. A status row lost
with its process can leave the list stale, as explicitly accepted. Direct
verification, failed-requirement columns, scoped viewers and UI implementation
are outside this task. Preserve existing permissions and tenant isolation.

Planning comprehension pass: an independent reader checked the draft
explanations against the specification and design. The user subsequently
approved revision 4, including the release/defer correction. Equal-time
selection remains consumer-owned. This is document review, not implementation
or test evidence.
