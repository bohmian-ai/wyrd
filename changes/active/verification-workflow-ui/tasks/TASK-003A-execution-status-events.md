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
   - Proof: `verification_runtime::pending_cancellation_records_cancelled_status`
     (command J). RED: `<id> never listed cancelled: {}, history []`. GREEN:
     PASS. The test reads the newest row through `bifrost.query` and checks
     `sequence = 0`, the Verifier, subject and actor ids, ten null content
     columns and a null `value`. Inspection answers `cancelled`, and a repeat
     cancel leaves the history at one row. Status: PASS.
2. **Runner lifecycle.** Hold executions at running, retrying, deferred-input
   waiting and approval waiting; query each while held, then let work complete
   or terminate with timeout/error. Each newest status has the exact serialized
   word. Include exhausted attempts and both grouped completion and the existing
   fallback path in focused owner proof. Release/defer previously claimed work
   without parking and observe the resulting pending/retrying status. Today no
   `event = status` rows exist.
   - Proof: `verification_runtime::runner_transitions_record_lifecycle_status`
     (command J). RED: `never listed completed: {}, history []`. GREEN: PASS.
     The test queries `running`, both parks as `waiting` and `retrying` while
     each is held, and checks that the queue stores no `waiting`. It covers
     completion, `errored` and `timed_out` terminations, and a backpressure
     deferral recording `[running, pending, running, completed]`.
   - Release: `pg_verification_runtime::claim_committed_after_cancellation_is_released_unexecuted`
     (command R) drives the runner's `Transition::Release` and asserts
     `[running, pending]`. GREEN only: the assertion was added after the
     release staging.
   - `retrying` from a release is the same `RELEASE_SQL ... RETURNING status`
     passthrough. No test releases a run after a retry.
   - Exhaustion: `pg_verification_runtime::retryable_engine_failures_exhaust_to_errored`
     (command R) asserts
     `[running, retrying, running, retrying, running, errored]`.
   - Group commit: `pg_verification_runtime::failed_group_commits_retry_as_groups`
     (command R) asserts `[running, completed]` per run through the batched
     statement.
   - Fallback: FAIL, open. Only the `Transition::Complete` arm of
     `settle_alone` is unproved. It runs only when every `Completions` flush
     task is gone, which `Completions::complete` reports as `None`. No test can
     force that today without adding a seam that ends the flush tasks. Source
     proof: `settle` stages from the one `(outcome, status)` pair that
     `apply` returns, and both paths build it with `recorded(.., Completed)`.
     Every other `settle_alone` arm is exercised by the tests above.
   - Release and deferral return the stored status from
     `RELEASE_SQL ... RETURNING status`; command Q asserts `Some(Pending)` and
     `None` for a stale lease.
   - Status: PASS except the fallback's completion arm (FAIL, above).
3. **Running cancellation and revocation.** Cancel held running work through
   the public operation; revoke a principal with multiple unfinished executions
   through the public revocation operation. Each affected execution lists as
   `cancelled`. Release old workers and confirm stale settlements do not publish
   completion/status rows. Preserve existing cancellation fencing behavior.
   Include focused proof for admission-time principal cancellation. Today the
   queue cancels correctly but the per-execution Bifrost rows are absent.
   - Proof: `verification_runtime::running_cancellation_and_revocation_record_cancelled_status`
     (command J). RED: `never listed running: {}, history []`. GREEN: PASS.
     Cancelling held running work records `[running, cancelled]`. After the
     worker is released the queue stays `cancelled` and no stale row appears.
     Revoking the submitter cancels both its running and its pending
     execution, recording `[running, cancelled]` and `[cancelled]`.
   - Admission-time proof is in `pg_verifier_executions` (command Q).
     `revocation_and_admission_order_on_the_principal` asserts the exact
     `CancelledExecution` that the revocation returns and that a submission
     admission returns. `background_admission_of_a_revoked_owner_is_cancelled`
     does the same for scheduled and group admissions.
     The grouped assertion compares the execution and attribution but not
     the Verifier, because the group candidate does not expose it.
   - PR cancellations: closing, drafting or superseding a PR, a policy that
     binds nothing, and repository access loss also cancel unfinished work.
     They sit under the Outcome's "any existing owner" and REQ-015's stale-row
     rule, though the transition table does not list them.
     - `DEACTIVATE_SQL`, `SUPERSEDE_SQL` and `CANCEL_REPOSITORIES_SQL` gain
       `RETURNING` identities. `deactivate_pull_request`,
       `cancel_pull_request_repositories` and `reserve_pull_request`'s
       superseded list return `CancelledExecution`.
     - The PR admission owner and the GitHub connection service (disconnect,
       uninstall, repository removal, suspension) stage them after commit.
     - Proof:
       `pg_github_pull_requests::pull_request_lifecycle_admits_supersedes_and_ends_through_the_webhook`
       (command G) waits until the newest status of every one of the PR's ten
       cancelled executions is `cancelled`. It reads them through the
       editor's SDK `bifrost.query`, using Verify's newest-status shape. RED,
       with the production change stashed and an earlier form of the wait:
       `status rows never landed: {}`. GREEN: PASS. Command Q asserts the
       returned identities.
   - The existing fencing assertions are unchanged: the 40 queue tests
     (command Q) and 23 `wyrd-auth` revocation and issuance tests (command A)
     pass. Status: PASS.
4. **Existing write and recovery boundaries.** In focused outbox proof, stage
   several status events and assert they share the ordinary execution-events
   frame, preserve per-row attribution and retain event identity on retry. In
   owner proof, a rolled-back mutation stages no event. Keep existing checkpoint
   recovery and tenant-isolation assertions passing; status rows must never be
   selected as checkpoint revisions. These guard the required unchanged paths,
   rather than defining another recovery mechanism.
   - Proof: `scribe_outbox::tests::status_rows_share_one_frame_and_keep_identity_on_retry`
     (command O). Status writes for three executions share one
     execution-events frame. Each row keeps its attribution pair, status,
     `sequence = 0`, null content and occurrence time. A refused attempt and a
     redelivery resubmit one batch id with identical bytes and event ids.
     GREEN: PASS. There is no RED, because the test was written after the
     `Status` variant.
   - Rollback: in the group-commit test above the first batch fails, and each
     run still records exactly one `completed`. In the revocation journey,
     released stale workers add no row.
   - Recovery: `verification_runtime::queued_subject_authority_survives_restart`
     (command J) passes alongside status rows. Exclusion from checkpoint
     selection is source proof only: the fence reads filter
     `event = 'checkpoint'` (`verification/inputs.rs`). Tenant isolation is
     command T. Status: PASS.

Commands, run sequentially:

- J: `mise run build:server`, then
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::<name>)"'`
- Q: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(pg_verifier_executions::)"'`.
  40 tests pass.
- A: the same wrapper around
  `cargo nextest run --locked -p wyrd-auth --test-threads=1 -E "test(revoke) | test(issuance::pg_tests)"`.
  23 tests pass.
- O: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::status_rows_share_one_frame_and_keep_identity_on_retry)'`
- R: the same wrapper around
  `cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::<name>)"`.
  The owner lane `mise run test:bifrost:integration:server`: 125 of 125 pass on the final code.
- G: the same wrapper around
  `cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(pg_github_pull_requests::)"`.
  22 tests pass.
- T: `mise run check:tenant-isolation`, `mise run fmt`, `mise run lints` and
  `mise exec -- git diff --check`: all pass.

TDD compliance: VIOLATED for part of the work. Scenarios 1–3 had a journey
RED before any production code. These proofs were written after the code
they cover and have no RED, although each is verified GREEN:

- the scenario 4 outbox test;
- the admission-time `CancelledExecution` assertions;
- the exhaustion and group-commit history assertions;
- the command Q `release` assertions (`Some(Pending)`, `None`), rewritten
  with the signature;
- the release history assertion.

For the PR cancellations the test was written first, but the production
change was drafted before RED was run. RED was then observed with the
production change stashed. Four existing tests were re-sequenced (see the
diagnosis above), and their assertions are unchanged. The fallback's
completion arm has no test.

Hot path: not touched. The change adds nothing to the queued engine
execution, direct verify, gateway calls, Bifrost query or Bifrost append.
The runner, the submission writer and PR admission are background writers.
The cancel and revocation routes add only a non-blocking Scribe stage after
their existing commit. They add no PostgreSQL statement or write, and they
neither wait on nor fail because of the stage. Revocation reuses its
existing `UPDATE`, now with `RETURNING`. The task excludes a bench run when
no hot path is touched.

Risk: the timestamp comes from the pod clock at the transition, as the
task's "capture at the transition" asks. Newest-row ordering across replicas
can therefore invert under clock skew, and tie handling is consumer-owned.

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
