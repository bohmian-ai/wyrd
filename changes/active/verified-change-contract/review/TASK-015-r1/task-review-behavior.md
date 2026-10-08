# TASK-015 Behavior Review

## Review Findings

No behavior findings.

The candidate satisfies the reviewed TASK-015 behavior. The two changed Eval
journeys align their proof with revision 60 rather than weakening it, and the
reviewed merge resolution retains both the audit-outbox and Verifier-runtime
intent.

## Immutable Subject

- Repository root: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-a0ed64ce133bff9d3`
- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`
- TASK-015 diff: `3f8767a5f6a9b9c8605a53c424c7a7056a3b6786..9b560d00569656ee7fdde19a9c76fa55c8d55fbb`
- Merge resolution: `c5527627a50dd66a9f53d760d80f59bcb59609f9`, parents `ca2950856a37786a5cad25c73c1786c5fa7a1822` and `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 60, REQ-077, REQ-108, AC-014
- Original task: `changes/active/verified-change-contract/tasks/TASK-015-eval-runs-in-the-batch-fence.md`

HEAD was the stated candidate before and after source inspection. The no-conflict
merge `3f8767a5f` and generic-outbox internals were not reopened.

## Acceptance Matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| After Scribe durably acknowledges a first commit, stage one run request per committed Eval record without making the client wait for run creation (REQ-077, AC-014). | `Gate::record` invokes the synchronous hook only for `admission.first_commit` (`crates/vala/vala-bifrost-redux/src/gate/mod.rs:997-1009`). `ObservationEnqueue::acknowledged` decodes the acknowledged frame and calls the non-async outbox `stage` once per record (`crates/wyrd/wyrd-server/src/verification/observations.rs:90-136`). | The task records `mise run test:bifrost:journey:server` as 31/31 passed; `integrated_enqueue_outage_preserves_ack_and_recovers` crosses Gate, Scribe, the readable observation, and later run creation (`eval_verification.rs:1181-1314`). | PASS |
| Implement the Eval run-request outbox as `Outbox<ObservationRunSink>` and delete the hand-written queue/writer. | The alias is exactly `Outbox<ObservationRunSink>` and the sink owns only `WyrdPostgres` plus `VerifierRunQueue` (`observations.rs:27-65`). The TASK-015 diff deletes the local channel, pending counter, cancellation token, task tracker, retained groups, backoff, and writer. No second queue/writer remains in the module. | The task records workspace check/lints and the server journey lane as passed. | PASS |
| Flush each tenant batch through the one durable multi-row run insertion in its own Wyrd SQL transaction, with retry-safe duplicate handling. | `ObservationRunSink::write` opens a tenant transaction, calls `enqueue_observation_batch`, and commits (`observations.rs:67-87`). The durable consumer filters active bindings and performs one `INSERT ... SELECT FROM unnest(...) ... ON CONFLICT DO NOTHING` (`crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:141-167,1640-1707`). | The task records `observation_batches_insert_once_per_binding_and_record` and `mise run test:sql` as passed. The SQL test proves repeated batches add no row or ordinal (`pg_verifier_runs.rs:1860-1967`). | PASS |
| Restrict binding-created work to runtime-active exact owners, with inactive occurrences neither enqueued nor later backfilled (REQ-108). | `enqueue_observation_batch` calls `accepts_records` for each binding; that method requires active `binding_activity` before target resolution (`verifier_runs.rs:1678-1684,1709-1735`). | `observation_batches_insert_once_per_binding_and_record` includes an aged inactive owner and expects no run for it (`pg_verifier_runs.rs:1906-1933`). | PASS |
| No count limit or drop on slow/unavailable Postgres; failed writes remain pending, recover idempotently, graceful shutdown flushes, and deadline loss is reported (REQ-077, AC-014). | TASK-015 delegates these unchanged mechanics to the approved generic outbox and supplies a fallible sink. Server shutdown drains Bifrost before the run outbox and calls `shutdown(deadline)` (`crates/wyrd/wyrd-server/src/app/server.rs:850-878`). | The focused runtime test proves retained pending count, recovery, repeat idempotency, graceful flush, and a reported deadline remainder (`pg_verification_runtime.rs:2411-2488`). The integrated journey separately proves a post-ACK refusal does not retract the readable observation, keeps retrying, and produces exactly one run per binding after recovery (`eval_verification.rs:1181-1314`). The task records both focused and journey lanes passed. | PASS |
| Use generic metrics labeled `outbox="eval_run_requests"`; count/log observable decode loss. | `ObservationRunSink::NAME` is `eval_run_requests`; acknowledged-frame decode failure logs the tenant/request and increments `outbox_events_lost_total` with that label (`observations.rs:62-65,125-134`). The removed module-local failure/retry metric names have no remaining references. | Task evidence reports the reference search and lint/check lanes clean. | PASS |
| Preserve the prohibition on Scribe/`vala-sql` writes to `verifier_runs`, cross-crate transactions, and client waiting. | The only new durable call is from the server sink into the Wyrd-owned `VerifierRunQueue`; the Scribe acknowledgement completes before the synchronous, IO-free hook, and the hook only stages (`gate/mod.rs:299-316,997-1009`; `observations.rs:105-136`). No TASK-015 diff adds a Scribe or `vala-sql` write. | Covered by the integrated journey and source boundary; no separate dynamic proof is necessary for the negative dependency boundary. | PASS |
| Keep the continuous Eval terminal journey valid under revision 60. | The deleted pre-phase installed a permanent, row-specific database refusal and asserted that an acknowledged record never acquired a run. Revision 60 instead requires a failed flush to retain and retry its batch; because the request shares its tenant batch, preserving the old assertion would require the prohibited drop. The terminal journey still proves all required terminal states, frozen record/time behavior, day pruning, results, skipped outcomes, gates, dispatches, sampled behavior, trace wait, and mock judge (`eval_verification.rs:623-974`). The separate integrated outage journey now owns post-ACK failure/recovery proof. | Task evidence diagnoses the old test failure from retry traces and records the full journey lane passed. | PASS |
| Prove a sealed replay is not staged while the original and a distinct sentinel are staged exactly once. | Holding `wyrd.verifier_runs` locks the original request in flight. Gate stages before each `insert` returns, the generic pending count includes queued and in-flight items, and same-tenant serialization keeps the sentinel pending. Therefore the exact count of two after sentinel acknowledgement directly detects any replay staging (`eval_verification.rs:1009-1094`). Final durable assertions still require four runs for each of the original and sentinel, only one stored original, and the original committed event time/day (`eval_verification.rs:1092-1151`). | Task evidence records the server journey lane passed. The replacement is stronger and less topology-dependent than counting blocked Postgres backends, which cannot see a same-tenant request waiting in the process outbox. | PASS |
| Preserve both sides of merge `c5527627a`: stage decisions through `AuditOutbox`, remove audit-unavailable public error codes, and retain TASK-013/014 SYSTEM-token removal. | The combined merge diff keeps `.with_audit(Arc::clone(&audit_outbox))` and both `observation_runs` and `audit_outbox` in `BifrostComposition`; current boot wiring remains at `boot/mod.rs:1117-1133`. Gate stages the decision synchronously on its audit sink and no longer waits for a durable audit append (`gate/mod.rs:478-518`). Comparison with both parents shows the merge removed `AuditUnavailable` variants/mappings while retaining the protobuf's intentionally reserved removed enum name; current error surfaces contain no audit-unavailable public error variant. Parent `52e1144b5` had `issue_system_token` and publisher use; neither exists in the merge result or candidate. | Static parent/merge comparison and `git show --cc c5527627a`; no tests were rerun for this review. | PASS |
| Avoid unrelated behavioral change in the TASK-015 commit. | Besides task evidence, wiring, sink, and tests, the only source edit is the mechanical Clippy-safe `u16` to `u32` conversion in `sdks/wyrd-sdk-rust/tests/observe_run.rs`; it preserves the test values and behavior. | `git diff --check 3f8767a5f..9b560d005` was clean. | PASS |

## Focused Decisions

### Eval journey edits

Deleting the permanently refused pre-phase from
`continuous_eval_runs_the_terminal_matrix` follows revision 60. That phase
proved the retired fail-open behavior by expecting an acknowledged run request
to disappear permanently. Keeping it would either leave that tenant's retained
batch pending forever or force the implementation to violate REQ-077 by
dropping the refused request. Required outage behavior did not lose coverage:
both the integrated Gate/Scribe journey and the focused outbox test retain,
retry, recover, deduplicate, and flush requests.

Replacing the blocked-backend count in
`sealed_replay_on_a_later_day_activates_once` with `pending() == 2` also follows
revision 60. The generic outbox intentionally allows only one in-flight write
per tenant, so the sentinel remains in process rather than creating another
blocked Postgres transaction. The exact pending count observes the new owner of
that state and still falsifies an incorrectly staged replay. The test continues
to validate the durable run count and frozen first-commit time after releasing
the lock.

### Permanently failing tenant item

The implementer's stated poison-item risk is not a TASK-015 finding. Revision
60 requires retained batch retry rather than loss when a flush fails, and it
defines the accepted availability case as Postgres being slow or unavailable.
It does not require item classification, dead-lettering, batch splitting, or
continued same-tenant progress around a permanently invalid request. Adding any
of those would choose new loss/order/persistent-state semantics outside the
approved task. Cross-tenant progress remains the generic outbox's approved
behavior, which this task did not change.

## Verification Notes

- Reviewed the complete TASK-015 diff and the merge's combined conflict
  resolution against both parents.
- Inspected the caller-to-result path from Gate first-commit acknowledgement,
  through `ObservationEnqueue` and `ObservationRunSink`, to
  `VerifierRunQueue::enqueue_observation_batch`, plus shutdown and the relevant
  journeys.
- Did not run Cargo or `mise` lanes because the delegated review scope forbids
  test execution in the shared checkout. The implementation record reports:
  focused SQL and runtime tests, `mise run test:sql`,
  `mise run test:bifrost:integration:server`, the 31-test server journey lane,
  format, lints, workspace check, and diff check all passing.
- `git diff --check 3f8767a5f..9b560d005` was independently clean.

## Overall Result

**PASS**

