---
id: TASK-001-R1
kind: remediation
status: implemented
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 1
original_task: TASK-001
review_candidate: 31af0fe55c4229cee3ecd7bd804f116b1b0fc266
review_base: 66f44aee1
findings: []
---

# Verification reads only Bifrost, never Bifrost's receipts

## Authority and outcome

A maintainer-directed remediation of [TASK-001](../../tasks/TASK-001-fence-lifecycle.md)
against the [approved spec](../../spec.md); no `$wyrd-task-review` produced it.
Implement it through `$wyrd-implement`. The candidate is `31af0fe55` plus the
uncommitted work in progress kept by step 5.

Pending maintainer approval: the changed lifecycle assertions in step 1 and the
REQ-003 rewording in step 7.

## Outcome
Verification stops reading `vala.scribe_batch_commits`. A run in progress keeps
its state in memory, as today. A resume, a status read, an approval check, an
evidence check, and a GitHub "already posted?" check each read Bifrost alone.
Receipt cleanup (TASK-001) can then delete old receipts without breaking any of
them. Nothing is added to Postgres: no new table and no new column.

## The one rule that makes this safe
A Bifrost read can briefly miss a row Bifrost has just acknowledged, if the
Bifrost writer holding that row crashed before the row became readable
(`wyrd-spec/src/vala/api.rs:1078`). Every verification write already carries a
fixed batch id derived from what it is:
- a revision: `revision_batch(execution, revision)` (`verification/state.rs:79`)
- a decision: derived from its approval id
- an evidence acceptance: derived from its source, input and Verifier
- a GitHub outcome: derived from its event, execution and revision

When a second write arrives under an id Bifrost already holds:
- the same contents are absorbed as a retry;
- different contents are refused with `BatchConflict`
  (`scribe/shards.rs:4397-4408`, `refuse_conflict`).

So a reader that misses a just-saved row can never overwrite it. At worst it
does the work again and gets refused, and the next attempt sees the row. That
duplicate check is Bifrost's own retry dedupe, the receipts' one legitimate job.

What each reader does on a miss:

| Reader | On a miss | Why it heals |
|---|---|---|
| Resume | Resumes from the older revision. Its first new save is refused with `BatchConflict`, so its writer breaks: `StateError::Stale` maps to `Transition::Retry` (`runner.rs:1859`). | The retry reads again and finds the row. It can't settle on stale progress, because settling waits for its own saves (`persisted()`). |
| Status / inspection | Shows the slightly older revision. | It only reads. |
| Approvals (`decided`) | The decision reads as not yet made, so the run stays parked. | The park recheck reads again. A repeated identical decision is absorbed; a different one is refused (`approvals.rs`, the `accept` doc). |
| Evidence (`accepted`) | The acceptance reads as absent, so the run stays parked. | The park recheck repairs it (`service/evidence.rs:322`). A resubmission is absorbed. |
| GitHub (`recorded`) | The outcome reads as not recorded, so delivery runs again. | Lookup-before-create finds and updates the same Check and review (`github/delivery.rs:11-17`). Re-recording under the same id is absorbed, and the first outcome stays final. |

A failed Bifrost query (an error, not a miss) still returns unavailable and
defers, as today.

## Step 1 — Resume reads the newest revision in one query
- `verification/inputs.rs`:
  - delete `latest_revision`, which this task added (absent at `66f44aee1`), and
    `state(execution, revision)`, whose only caller is `RunState::accepted`;
  - add
    `pub async fn latest_state(&self, execution: ExecutionId) -> Result<Option<(i64, Value)>, WyrdError>`
    running
    `SELECT sequence, value FROM bifrost.verification.execution_events WHERE execution_id = {id} AND event = 'checkpoint' ORDER BY sequence DESC LIMIT 1`.
    `ORDER BY ... LIMIT` is already used in Oracle SQL (`hosted/traces.rs:61`).
- `verification/state.rs`:
  - change `RunState::accepted` to
    `pub async fn accepted(execution: ExecutionId, records: &InputResolver<'_>) -> Result<(i64, ExecutionState), StateError>`.
    `None` returns `(0, ExecutionState::fresh())`; `Some((revision, value))`
    returns `(revision, ExecutionState::decode(value)?)`; a query error returns
    `StateError::Unavailable`.
  - delete `accepted_revision`, `PROBE` and the `committed_batches` import.
    Keep `revision_fence` (the lifecycle tests build receipts with it) and
    `STATE_TABLE` (`github/delivery.rs` still names it).
  - rewrite the module docs (lines 13-19 and 26-28) and the `accepted` doc to
    state the rule above.
- Update the callers for the dropped `app` and `tenant` parameters:
  - `components/verification/service/inspection.rs:1122`
  - `verification/runner.rs:742`
  - `wyrd-testing/src/verification.rs:839`
  - `wyrd-server/tests/integration/pg_durable_verifier_work.rs:246` and `:2874`
- Tests:
  - RED, then GREEN. In `wyrd-testing/tests/bifrost/scribe/lifecycle.rs`,
    change the second half of `execution_recovery_requires_exact_accepted_state`
    (~`:1345`) and of `execution_recovery_outlives_its_revision_receipts`
    (~`:1474`). They insert a receipt for revision `last + 1` with no readable
    row. Today they assert the takeover returns `StateError::Unavailable`. The
    new assertions:
    1. the takeover recovers `accepted`, the newest readable revision;
    2. its next change is refused (`persisted()` returns `Err(StateError::Stale)`);
    3. after the receipt is withdrawn, a further takeover recovers `accepted`.

    This proves a missed revision is never overwritten. The current code fails
    assertion 1. These are existing assertions whose expected behaviour changes,
    so they need your approval.
  - unchanged and expected to pass:
    - `execution_recovery_outlives_its_revision_receipts`, first half
    - `restart_recovers_an_acknowledged_unpublished_checkpoint`
    - the takeover/conflict lifecycle test
    - `state.rs` unit test `a_refused_revision_breaks_a_queued_writer`

## Step 2 — Approvals, evidence and GitHub read only Bifrost
- `verification/approvals.rs` `decided`: delete the `tenant_conn` and
  `committed_batches` block and the "not visible yet" branch. It returns the
  visible decisions for `approvals`. Rewrite the module doc lines 6-9 and the
  `decided` doc.
- `verification/evidence.rs` `accepted`: the same edit. Rewrite its doc
  (~lines 140-151).
- `components/github/delivery.rs` `recorded`: the same edit. Return
  `records.delivery(...)` directly. Rewrite its doc (~lines 349-359).
- Tests, RED then GREEN. Each of these tests already revokes the tenant role's
  `SELECT` on `vala.scribe_batch_commits` around part of its flow:
  - `pg_github_delivery::a_recorded_outcome_outlives_its_retry_receipts` (`:756-768`)
  - `pg_deferred_evidence::accepted_evidence_outlives_its_retry_receipt` (`:910-912`)
  - `pg_approvals::a_decision_outlives_its_retry_receipt` (adds the same revoke)

  Widen each revoke so it also covers the reader call. The current code then
  fails, because it reads receipts first and permission is denied (503). After
  this step it passes. This proves the readers never touch receipts.

## Step 3 — Delete the shared receipt reader
- Remove the `committed_batches` re-export from
  `crates/vala/vala-bifrost-redux/src/contracts.rs:85`. The build then proves no
  verification or gateway code reads receipts.
- Bifrost-internal users stay:
  - `scribe/ingress.rs`, `shards.rs`, `replay.rs`, `receipt_cleaner.rs`
  - `vala-sql`'s own query
  - the lifecycle test's direct call (`lifecycle.rs:739`)

## Step 4 — Capacity test waits instead of deadlocking
`wyrd-testing/tests/bifrost/server/verification_runtime/isolated_capacity.rs`
`connected_work_waits_out_pressure_with_its_own_attribution` (`:613`):
- today: it reads status while pressure is held and releases pressure only
  afterwards (`:661-674`);
- change: spawn the status read alongside the gateway call and the query, assert
  it is still waiting (`!status.is_finished()`) like them, release pressure,
  then assert it returns `Ok`;
- the run waits under pressure, so its status needs a Bifrost read, and that read
  waits for memory like any other Bifrost read. The test now proves it waits and
  then finishes, with no deadlock;
- update the doc comment to say so.

## Step 5 — Keep the uncommitted work as is
- `wyrd-sql/src/queries/verifier_executions.rs`: `claimed_before` on claim and
  `claimed` on status. Both are derived from the existing `lease_token` column,
  with no schema change. A never-claimed run skips the Bifrost read, which keeps
  pending status at 0 Bifrost queries and a first start at 0 extra queries, as
  before this task.
- `wyrd-sql/tests/integration/pg_verifier_executions.rs`:
  `a_released_execution_still_reads_as_claimed`.
- `verification/runner.rs` `accepted()`: resume waits for memory under
  admission and defers on refusal instead of hanging.
- `inspection.rs` `accepted()`: skips the read for a never-claimed run.

## Step 6 — Check the query budget
`verification_runtime::completed_result_lookup_preserves_evidence`
(`verification_runtime.rs:7203`) allows 2 queries and 4 files for a completed
status. Today's code uses 4 and 6. After step 1, a completed status makes one
state query plus one result query, which is 2. Run the test; leave the budget
unchanged.

## Step 7 — Task and spec wording
- `spec.md` REQ-003 (lines 77-79). Replace "Recent acceptance receipts bridge
  accepted records that are not yet query-visible: unavailable authority or an
  accepted-but-invisible row must produce the existing unavailable/retry
  behavior, never historical absence." with "Consumers read only their Bifrost
  records. A record accepted but not yet visible reads as absent; its fixed
  batch id makes any second write for it a retry or a refused conflict, never
  an overwrite, and the consumer's next read finds it. A failed read is
  unavailable and retries."
- `TASK-001` lines 35-41 and 108-124: describe the readers above. Record steps
  1-4 as RED/GREEN evidence with their commands.

## Not changed
- the verification engine and its background saves
- direct verification and the gateway
- Bifrost reads and writes, including the receipt write before acknowledgement
- receipt cleanup and the cleanup write gate
- the queue table's structure

## Verification (exact commands)
- `mise run fmt && mise run lints`
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/verification::state::/)'`
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --no-fail-fast -E "test(/^(pg_approvals|pg_deferred_evidence|pg_verification_runtime|pg_github_delivery|pg_durable_verifier_work)::/)" && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(/^lifecycle::/)"'`
- the same wrapper for
  `cargo nextest run --locked -p wyrd-sql --test integration -E "test(/^pg_verifier_executions::/)"`,
  and for `wyrd-testing` `-P journey --run-ignored=all` with
  `test(=verification_runtime::completed_result_lookup_preserves_evidence)` and
  `test(/connected_work_waits_out_pressure/)`
- `git diff --check`


## Evidence

- Step 1 — `lifecycle::execution_recovery_requires_exact_accepted_state` and
  `lifecycle::execution_recovery_outlives_its_revision_receipts`: with a receipt
  for revision `last + 1` and no readable row, a takeover recovers `accepted`,
  its next change fails `persisted()` with `StateError::Stale`, and a later
  takeover still recovers `accepted`. RED: `lifecycle.rs:1349` and `:1481`
  "the newest readable revision recovers: Unavailable(\"revision 2 is not
  visible yet\")". GREEN: lifecycle 15/15.
- Step 2 — receipt `SELECT` revoked around the read:
  - `pg_github_delivery::a_recorded_outcome_outlives_its_retry_receipts`:
    status is 200 and `delivered`. RED `pg_github_delivery.rs:768`: 503.
  - `pg_deferred_evidence::accepted_evidence_outlives_its_retry_receipt`: a
    republish (read only) replays. RED `pg_deferred_evidence.rs:912`: 503
    "deferred evidence cannot be read now: ... permission denied for table
    scribe_batch_commits". After the reader fix a fresh publication still
    returned 503 `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE`: Scribe's own
    write path reads receipts under the same role, so a fresh publication
    cannot prove the reader and the assertion uses a republication.
  - `pg_approvals::a_decision_outlives_its_retry_receipt`: with a held
    receipt and no visible row, status is 200 and the occurrence is
    `pending`; a new decision is 503, so the held one is never
    overwritten. RED `pg_approvals.rs:746`: 503.
- Step 2, deleted: the `approvals::tests::an_unavailable_read_hides_its_cause`
  unit test, together with `unavailable()`, the receipt-read error it
  covered; the `fence()` helpers and `unavailable()` in evidence and delivery
  had no remaining callers.
- Step 3 — `committed_batches` is no longer re-exported;
  `cargo check --workspace --all-targets` is clean. Remaining callers: Scribe's
  `receipt_cleaner` tests, `vala-sql` tests, and `lifecycle.rs:739`.
- Step 4 — `connected_work_waits_out_pressure_with_its_own_attribution`: the
  status read is counted as waiting Bifrost work under pressure and returns
  `Ok` after recovery. GREEN in the `verification_runtime::` journeys.
- Step 6 — `completed_result_lookup_preserves_evidence` passes on its existing
  budget of 2 queries / 4 files.
- Unplanned: `pg_verification_runtime::exhausted_reconciliation_survives_source_and_capacity_outage`
  timed out. Its source outage revoked receipt access, which recovery no
  longer reads. A read-only Codex diagnostician confirmed the cause; fix site
  is the test setup. It now arms `fail_next_query_after_schema()`, which
  internal reads honor (`query/scheduled.rs:159`), and passes 3 of 3 runs.
- Commands (all through
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && ...'`):
  - `cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --no-fail-fast -E "test(/^(pg_approvals|pg_deferred_evidence|pg_verification_runtime|pg_github_delivery|pg_durable_verifier_work|pg_github_pull_requests)::/)"`
    → 139/139
  - `cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(/^lifecycle::/)"`
    → 15/15
  - `cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(/verification_runtime::/)"`
    → 35/35
  - `cargo nextest run --locked -p wyrd-sql --test integration -E "test(/^pg_verifier_executions::/)"`
    → 41/41
  - `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/verification::(state|approvals|evidence)::/) | test(/github::delivery::/)'`
    → 12/12
  - `mise run fmt && mise run lints` clean; `git diff --check` clean.
- TDD: steps 1 and 2 were RED before production changes. Step 5's code
  predates this remediation.
