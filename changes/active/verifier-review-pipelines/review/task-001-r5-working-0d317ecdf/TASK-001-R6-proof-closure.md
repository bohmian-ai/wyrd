---
id: TASK-001-R6
kind: remediation
status: ready
spec: SPEC-verifier-review-pipelines
spec_revision: 21
approved_scope: terminal-settlement amendment, unchanged revision-18 obligations, and user-approved verification-result idempotency
supplement: SUPPLEMENT-verifier-execution-state-and-durability@3
parent_task: TASK-001
previous_remediation: TASK-001-R5
remediates: [FIND-TASK-001-19, FIND-TASK-001-23, FIND-TASK-001-27]
user_directed: [USER-RESULT-IDEMPOTENCY-001]
carried_proof: original hosted concurrent A/B-to-C observation
depends_on: []
---

# Make verification-result writes idempotent and finish closure

## Outcome and contract

Keep R5's accepted production terminal workflow. Make its verification-result
writes idempotent using the existing Scribe batch fence, complete two
already-required integration observations, repair one regressed SQL test, and
finish the local documentation gate. A restarted claim may resend a selected
result, but must not append its output or summary a second time.

USER-RESULT-IDEMPOTENCY-001 records the user's explicit approval, after the R5
review, of deriving stable batch identities from the existing selected result_id
at verification-result handling in the shared server Scribe outbox. It is an
approved bounded addition, not a retroactive review finding. No additional
persisted key, writer, store or generic Bifrost API is authorized.

Authority and exact reviewed source are in [subject.md](subject.md) and its SHA
manifests: user-submitted working source on HEAD
`0d317ecdf2d28d2e77b6f394ebe0f443d256d580`, R3 baseline
`d89356985566ab3492699682942438c25c8322fd`, original base
`0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`. The
[validated ledger](findings-validation.md) and [verdict](verdict.md) select these
corrections. Preserve the [original task](../../tasks/TASK-001-bifrost-execution-state.md),
[R4 obligations](../task-001-r3-d89356985/TASK-001-R4-uniform-terminal-settlement.md),
[R5](../task-001-r4-0d317ecdf-working/TASK-001-R5-single-terminal-settlement.md),
and [approved terminal amendment](../../execution-state-and-durability.md#approved-terminal-settlement--revision-3).
Unrelated draft wait/approval work remains excluded. Route to `$wyrd-implement`.

## Diagnosis and selected correction

### USER-RESULT-IDEMPOTENCY-001: reuse the selected result as the batch key

Today `ScribeWrite::Result` preserves result_id, but projects its output and
summary into unkeyed shared frames. A new application delivery gets a new outbox
slice UUID and consequently new batch_ids. Existing same-batch transport retry
deduplication therefore does not stop a later claim appending equivalent rows.
Stable result_id in the rows alone does not provide write idempotency.

Correct this at the existing `wyrd-server/src/scribe_outbox.rs` Result projection
and frame construction. `ResultPayload::result_id()` already supplies the UUID.
Reuse the existing keyed-frame path used by ExecutionState: each of the result's
two rows gets an isolated frame keyed by that same selected result_id. The
existing batch_id derivation already includes tenant and destination table;
the output and summary therefore have distinct, repeatable batch_ids without
another durable identity. For the current one-row keyed frames, frame/chunk/
split positions are zero and do not depend on unrelated writes or slice order.
Preserve output-before-summary delivery and ACK only after both records succeed.

Bifrost's existing durable batch fence remains the only acceptance/dedup owner.
Matching redelivery must be acknowledged without another visible row; changed
contents under an accepted identity must remain a conflict. Preserve stable
selected outcome/timestamps, frozen correlation/attribution and the existing
schema/fingerprint comparison. Do not salt IDs per attempt, discard mismatches,
accept a conflict as an ACK, deduplicate only in readers, or add a second
deduplication registry. Existing volatile receipt/request metadata exclusions
remain unchanged. Prove reconstructed payload equivalence rather than assuming
that equal decoded values always produce equal Arrow fingerprints.

The approved cost is two one-row table frames per selected result, replacing
aggregation of unrelated results into shared frames. Keep the same bounded
process outbox, retry machinery and Scribe route; other write kinds retain their
batching behavior. Record frame/batch counts for a small multi-result case so
the cost is explicit; no new throughput threshold or benchmark project is asked
for. Existing oversized-row limits remain in force. Bifrost-wide row uniqueness,
external-effect exactly-once guarantees and retrospective cleanup of duplicates
already written are outside this addition.

### FIND19: exercise actual reserved-capacity refusal

P3 (`pg_verification_runtime.rs:2242–2286`) revokes only the batch-fence SELECT
grant. The current runtime correctly maps a refused reservation to Defer and an
exhausted Defer to LeaseLost, but the test never enters that earlier refusal
branch. R4 scenario 4 and R5 scenario 2 explicitly require a separate capacity
outage after a durable selection and failed final-attempt queue settlement.
The current GREEN therefore closes the source outage only.

Extend that existing proof with actual capacity refusal on the recovery
runner's outbox. Reuse `Outbox::reserve`, its slot accounting and reservation
drop: holding the reservable item share charges a count, not a GiB of payloads.
The existing runtime builder uses the server's shared Scribe outbox when
`publication_fault` is omitted; the current Harness always supplies that fault.
Use a test-local recovery composition selecting the actual shared outbox and
hold its existing item share. Existing `OutboxFull` information exposes limits;
do not hard-code a capacity or add a production/test capacity configuration.

Observe refusal on an exhausted reclaim, no engine entry, unchanged attempts
and no terminal settlement/refund while blocked. Release capacity, reclaim and
require completion under the original selected result ID. Preserve the source
outage case. This is existing test composition, not an unauthorized new seam.

### Original hosted-dependent proof: exercise the consumer

T6 (`pg_durable_verifier_work.rs:3012–3106`) directly calls RunState settle for
fabricated A/B values, awaits them and manually begins C. It proves shared state
acceptance but never runs the hosted engine or observes C consuming the values.
It would pass if the absent hosted consumer released too early or lost restored
values. This is the original unnumbered proof gap, not a new runtime finding.

Drive the existing hosted engine/checkpoint adapter and held-Scribe/reclaim
facilities with parallel A/B and a real C consuming both complete results.
Observe no C dispatch before required coverage is accepted, actual consumption
of both values afterwards, and recovery reusing accepted A/B without repeating
settled calls. Keep useful existing state-owner assertions. No new engine,
scheduler, harness, state store or performance requirement is needed.

### FIND27: restore the valid SQL plan alternative

The modified library test at `verifier_executions.rs:4397–4400` permits only the
two computation indexes, while the retained delivery-discovery query correctly
uses `verifier_executions_delivery_due`. The exact test fails on that index in
[independent execution](verification.md#independent-focused-execution).

Restore that delivery index to the test's permitted alternatives and rustdoc.
Keep the index-scan-only property: arbitrary/bitmap/table scans remain refused.
Do not change the production query, remove the delivery probe or rewrite the
immutable migration. This is a local test regression from rollback/rebuild.

### FIND23: complete the changed-item documentation

Document SQL `VerifierExecutionQueue::settle_delivery` (`:3397–3411`): matching
claim fence, caller-owned transaction and actual database failure under Errors.
Document `Counted::write/flush` in `verification/state.rs:410–416`: saturating
count, no retained buffer and always-Ok behavior. Do not invent an error path
for infallible methods. These are local rustdoc edits, batched here; no new
checker or separate style round.

## Proof strategy and acceptance

The inherited closure rows add missing observations and repair verification/
static gates; do not change correct production behavior to manufacture RED for
them. FIND27 already has a recorded real RED. The added idempotency behavior
requires the ordered RED/GREEN/REFACTOR proof below. If any test exposes an
unexpected runtime failure, record and independently diagnose it under
implementation-execution before fixing it.

### Idempotency scenario 1 — stable identities across independent staging

**Behavior.** Stage the same selected result in separate slices with unrelated
results interleaved or reordered. Each output and summary retains its own stable
batch identity and logical payload; the two destination tables have different
identities, and different selected results remain independent. Preserve existing
same-sealed-batch retry behavior and bounded outbox admission/release.

**RED.** Planned `scribe_outbox::tests::result_redelivery_keeps_batch_identity`
must fail on the current fresh-slice result identities. Register its actual
nonzero selector before RED; use the existing outbox test owner and sink seams.

**GREEN.** Route Result rows through the existing isolated keyed frames using
selected result_id, retaining existing per-table derivation, ordering and ACKs.

**REFACTOR.** Reuse existing frame/key/fence machinery; add no new writer,
generic identity framework, durable column or second acceptance check.

### Idempotency scenario 2 — restart and partial delivery store one result

**Behavior.** Exercise a failed queue settlement after result acceptance and
an interrupted delivery after output acceptance but before summary acceptance.
Restart/reclaim without engine reentry. Require exactly one retained output
and one retained summary for that selected result after delivery completes,
under its original identity. A conflicting payload with an already accepted
write identity is rejected and cannot retire delivery or replace accepted data.

**RED.** Planned
`pg_verification_runtime::result_redelivery_is_idempotent_after_restart` must
expose duplicate physical rows on current cross-claim restaging. Reuse existing
real Scribe/Postgres fault/reclaim facilities and inspect retained rows filtered
by result_id, not merely sent frames. Include the conflict negative case and
confirm its nonzero selector before RED. Strengthen existing P5's retained-row
assertions to the approved uniqueness outcome; preserve its stale-claim and
all-required-ACK assertions.

**GREEN.** New claims reproduce the same two write identities and matching
contents. Existing Scribe fences absorb accepted writes and accept the missing
record; conflicts or missing required ACKs keep delivery recoverable.

**REFACTOR.** Keep canonical selection, common conclude and queue claim fences;
remove obsolete test expectations that permitted duplicate retained results.
Do not weaken conflict checks or add reader-side DISTINCT as the correction.

| Obligation | Required closing observation |
|---|---|
| USER-RESULT-IDEMPOTENCY-001 | Independent staging/restart uses stable per-table batch_ids; same result yields exactly one retained output and summary; partial ACK recovers, conflicting contents are refused, and no engine/completion effect repeats. Record the two-frame batching cost. |
| FIND19 | Separate real slot refusal during exhausted recovery, no engine/budget/terminal mutation, then original result completion after capacity returns; source outage remains covered. |
| Original hosted A/B→C | Real hosted C consumes both accepted predecessor values; held ACK prevents early C entry; restart reuses accepted settlements without repeated settled calls. |
| FIND27 | Exact existing library test passes on the three approved indexes while retaining its bounded-plan constraint. |
| FIND23 | The three cited new Rust items satisfy local intent/invariant/error documentation rules; no runtime change. |

Run these selectors serially. The two planned names must be registered and
confirmed before RED; packages/targets are the existing test owners. Integration
commands use repository PostgreSQL:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::result_redelivery_keeps_batch_identity)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::result_redelivery_is_idempotent_after_restart)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::cancelled_selected_delivery_waits_for_all_required_acks)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_verification_runtime::exhausted_reconciliation_survives_source_and_capacity_outage)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_durable_verifier_work::concurrent_settlements_release_their_hosted_dependent)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_sql=debug cargo nextest run --locked -p wyrd-sql --lib --test-threads=1 -E "test(=queries::verifier_executions::pg_tests::runnable_scan_walks_the_claim_indexes_however_large_the_backlog)"'
mise run fmt
mise run lints
git diff --check
```

If an existing selector must change for test-local composition, record and run
its actual exact replacement before claiming closure. Add only owner regressions
affected by the final diff. No full language journeys or broad gate for this task.
Append concise criterion/source/command/result evidence and the final tracked
and untracked diff audit. No commit or push without caller direction.

## Preserved behavior and next review scope

Keep common conclude/result/ACK/settlement, no second acknowledged-failure pass,
immediate revocation with durable pending custody, exact identity, original
bounds, existing claim fences, stable checkpoint accounting, protobuf retirement
and direct fast return. Apply the user-approved idempotency guarantee only to
selected verification-result output/summary writes, including direct results
that use the same ScribeWrite::Result projection. Direct return still does not
wait for final-record ACKs. Leave generic Bifrost batch-fence semantics and all
other write kinds unchanged.

No new material decision is open. Report a specific authority conflict or an
actual failing supported path; private fixture choices remain implementation-owned.
The next review checks the four inherited closure rows plus the user-approved
result-idempotency addition and affected writer/consumer/test paths, carrying
unaffected R5 evidence forward. It must not restart Task 001 discovery or
prescribe another terminal redesign. If payload reconstruction cannot preserve
the existing fence's content contract without a new persisted protocol or wire
change, report the exact boundary before adding such machinery.

## Implementation evidence

Candidate: working tree on HEAD `0d317ecdf`, R5 production terminal workflow
unchanged. R6 write set: `wyrd-server/src/scribe_outbox.rs` (production +
unit tests), `wyrd-server/src/verification/state.rs` (rustdoc only),
`wyrd-sql/src/queries/verifier_executions.rs` (rustdoc + test restore),
`wyrd-server/tests/integration/pg_verification_runtime.rs`,
`wyrd-server/tests/integration/pg_durable_verifier_work.rs`. Migration
`20261108000000_verifier_execution_terminal_delivery.sql` unchanged
(`git diff 0d317ecdf -- crates/wyrd/wyrd-sql/migrations` empty).

### Reuse map

| Need | Existing owner reused | Gap closed |
|---|---|---|
| Stable result write identity | `Frame.key` keyed-frame path and `batch_id` (ExecutionState), `ResultPayload::result_id()` | `ScribeSink::frames` pushes each Result row as its own frame keyed by `result_id`; batch id = tenant + table + `(result_id, 0, 0)` + 0. No new key, writer, store or API. |
| Dedup / conflict | Bifrost durable batch fence; `ScribeError::BatchConflict` → `ScribeRefusal::Rejected` → write lost → `acked` dropped | none; unchanged |
| Capacity outage | `Outbox::reserve`, `OutboxFull { admitted, limit }`, `Reservation` drop; runtime builder without `publication_fault` | test-local composition only |
| Hosted A/B→C | `HostedBlocks::engine`, `with_checkpoints` + `RunState` (runner's adapter), `HeldRevisions`, `Leased::reclaim_on/reclaim` | test only |

### RED / GREEN

| Scenario | RED (production hunk reverted) | GREEN |
|---|---|---|
| Idempotency 1 `scribe_outbox::tests::result_redelivery_keeps_batch_identity` (selector listed 1 test before RED) | FAIL `two one-row frames per result write: left 6, right 10` (results shared slice frames) | PASS |
| Idempotency 2 `pg_verification_runtime::result_redelivery_is_idempotent_after_restart` (selector listed 1 test before RED) | FAIL `the settled result is retained once: left (2, ["failed","failed"]), right (1, ["failed"])` — duplicate retained rows filtered by result_id | PASS: settlement-failure reclaim, output-accepted/summary-refused restart, and conflicting rows under the accepted identity (never ACKed, retained result unchanged) |

Batching cost (unit case): 3 slices, 5 result writes of 3 results → 10 one-row
frames, 6 distinct batch ids; the same slices previously formed 6 shared frames.
Other write kinds keep their batching.

### Criterion table

| Obligation | Implementation | Verification | Result |
|---|---|---|---|
| USER-RESULT-IDEMPOTENCY-001 | `scribe_outbox.rs` `ScribeSink::frames` keyed Result frames; rustdoc on `ScribeWrite::Result`, `Frame.key`, `frames`, `chunks` | idempotency 1 + 2; P5 now asserts retained `(1, ["failed"])`; `lost_result_ack_replays_…` PASS; reconstructed rows byte-equal (`sent[0] == sent[1]` per table) and retained once | PASS |
| FIND19 | `exhausted_reconciliation_survives_source_and_capacity_outage`: source outage kept; then whole reservable item share of the shared outbox held (`reserve(usize::MAX)` → `OutboxFull`, `reserve(limit - admitted)`), recovery runtime built without `publication_fault`; refusal observed as `wyrd_verification_run_failures_total{mode="queued",outcome="stale_lease"}` increment; status `running`, attempts 1, only claim commits; share dropped → completes under the original result id, engine never entered | exact selector PASS; mutation (share released before reclaim) FAILS `timed out waiting for the reclaim to be refused capacity` | PASS |
| Original hosted A/B→C | `concurrent_settlements_release_their_hosted_dependent` rewritten on `HostedBlocks` Python code steps `left`, `right`, `combine(a, b)`; every revision held and released one at a time; when `combine` first appears in the writer's state the accepted state holds both settlements and no `combine`; crash there, reclaim, rerun: verdict Passed with `[1, "two"]`, left/right settlements unchanged, attempts left 1 / right 1 / combine 1 | exact selector PASS | PASS |
| FIND27 | `runnable_scan_walks_the_claim_indexes_however_large_the_backlog` alternatives and rustdoc restored to HEAD (adds `verifier_executions_delivery_due`; Index/Index Only Scan only) | exact selector PASS; hunk identical to HEAD | PASS |
| FIND23 | rustdoc on SQL `settle_delivery` (claim fence, caller transaction, `# Errors`); `Counted::write/flush` (saturating, no buffer, always Ok) | `mise run lints` PASS | PASS |

### Diagnoses

- **FIND19 barrier, first draft.** Symptom: the mutation without the capacity
  hold still passed. Evidence: the `stale_lease` failure counter rose with no
  refusal. Cause: the first runtime's attempt from `stage_unsettled_result`
  stays alive until its renewal fails after the reclaim, then records its own
  `stale_lease`. Fix site: the test only. Stop every earlier runtime before
  taking the baseline. The mutation then fails as expected. No production
  change.
- **Unit count.** The first GREEN run reported 10 frames against an expected 8.
  Cause: the expectation was mine and miscounted the slices (2+2+1 = 5 writes,
  so 10 frames is correct). Fix: the expectation only.
- **Aggregation tests.** `writes_group_by_destination_whatever_their_attribution`
  and `dense_projection_preserves_records_and_attribution` asserted shared
  result frames, which the approved cost replaces. They were updated to one
  frame per result row and keep their attribution, scope and row checks.

### Commands (serial, repository PostgreSQL, `WYRD_LOG=info,wyrd_server=debug`)

| Command | Result |
|---|---|
| `cargo nextest run -p wyrd-server --lib -E 'test(=scribe_outbox::tests::result_redelivery_keeps_batch_identity) \| test(/^scribe_outbox::/) \| test(/^verification::state::/)'` | 28/28 PASS (includes T5) |
| exact selectors: `result_redelivery_is_idempotent_after_restart`, `cancelled_selected_delivery_waits_for_all_required_acks`, `lost_result_ack_replays_the_identical_sealed_batch_and_scribe_deduplicates`, `exhausted_reconciliation_survives_source_and_capacity_outage`, `pg_durable_verifier_work::concurrent_settlements_release_their_hosted_dependent` | each PASS |
| owner regressions `test(/^pg_verification_runtime::/) \| test(/^pg_durable_verifier_work::/) \| test(/^pg_verification_groups::/)` | 84/84 PASS |
| wyrd-sql `queries::verifier_executions::pg_tests::runnable_scan_walks_the_claim_indexes_however_large_the_backlog` | PASS |
| `mise run fmt`, `mise run lints`, `git diff --check` | PASS |

### Non-goals and limits

- No new persisted key, writer, store, generic Bifrost API, reader DISTINCT,
  capacity config or production log. Generic batch-fence semantics and other
  write kinds are unchanged. Direct results share the projection, and direct
  return still does not wait for ACKs.
- Duplicates written before this change are not cleaned up. Dedup holds while
  the Bifrost batch fence retains the identity.
- Not run: language journeys, `wyrd-testing` Bifrost journeys and
  `pg_grpc_ingest_smoke` (they read results by ID or test raw ingest), and
  broad gates.
- Diff audit: R6 adds `scribe_outbox.rs` to the tracked diff. The other R6
  files were already in the R5 diff. `architecture/*` and
  `changes/active/verifier-review-pipelines/` are pre-existing and not
  committed. No commit was made.

Status: IMPLEMENTED
