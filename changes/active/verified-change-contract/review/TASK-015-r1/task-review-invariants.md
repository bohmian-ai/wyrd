# TASK-015 invariant review

## Immutable subject

- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`
- Task change: commit `9b560d005` over parent `3f8767a5f`
- Additional review surface: only the conflict resolution in merge
  `c5527627a`, with parents `ca2950856` and `52e1144b5`
- Authority: approved `changes/active/verified-change-contract/spec.md`,
  revision 60; `REQ-077`, `REQ-108`, and `AC-014`
- Original task:
  `changes/active/verified-change-contract/tasks/TASK-015-eval-runs-in-the-batch-fence.md`

The candidate remained at the stated commit throughout this review. The
generic outbox implementation was inspected only at its public staging,
pending, settle, and shutdown contract; TASK-015 does not change it.

## Producer-to-sink trace

1. Gate awaits Scribe's ingest result and invokes the observation hook only
   when `admission.first_commit` is true
   (`crates/vala/vala-bifrost-redux/src/gate/mod.rs:997-1008`). A sealed replay
   therefore cannot produce another request.
2. `ObservationEnqueue::acknowledged` decodes the committed Eval frame using
   Scribe's receipt time and synchronously stages one `ObservationRecord` per
   decoded row before Gate returns the acknowledgement
   (`crates/wyrd/wyrd-server/src/verification/observations.rs:90-136`). The
   staged value carries the authenticated tenant, observed subject, record ID,
   and exact committed event time.
3. `ObservationRunOutbox` is an alias for the repository's generic
   `Outbox<ObservationRunSink>`; `ObservationRunSink::write` opens one tenant
   transaction, calls `VerifierRunQueue::enqueue_observation_batch` once for
   that tenant batch, and commits it
   (`crates/wyrd/wyrd-server/src/verification/observations.rs:27-87`). The
   previous task-local queue, writer, retry state, and backoff are absent.
4. `enqueue_observation_batch` locks all matching `observations_ready`
   bindings in stable binding order, admits only runtime-active exact owners
   whose resolved target accepts Eval, and issues one multi-row insert
   (`crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:1640-1735`). The SQL
   deduplicates `(binding, record)` before ordinal assignment and uses
   `ON CONFLICT DO NOTHING`, while freezing the request's record ID and event
   time (`crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:115-167`).
5. Application shutdown first closes/drains Bifrost, then drains the retained
   Eval run-request outbox, then drains the audit outbox
   (`crates/wyrd/wyrd-server/src/app/server.rs:850-887`). No acknowledgement
   producer remains live when the Eval outbox's staging fence closes.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-077: Scribe acknowledgement does not wait for run creation; exactly one request is staged per record of the first committed Eval frame | Gate calls the synchronous hook only after Scribe succeeds and only for `first_commit`; `ObservationEnqueue` stages each decoded row on `Outbox` (`gate/mod.rs:997-1008`; `verification/observations.rs:105-136`) | The sealed-replay journey retains the first-commit and frozen-time assertions (`eval_verification.rs:1010-1148`); implementation evidence reports the server journey lane green | PASS |
| TASK-015 required reuse: `ObservationRunSink` over generic `Outbox`; task-local queue and writer deleted | Alias, sink, and hook are the complete current `verification/observations.rs`; `git diff 3f8767a5f..9b560d005` removes the hand-written writer and does not change `wyrd-runtime/src/outbox.rs` | Static diff and caller trace; recorded workspace check/lint evidence | PASS |
| REQ-077: one multi-row `verifier_runs` insert per tenant batch, retry-safe without duplicate rows or ordinal consumption | Sink makes one batch call per tenant transaction; SQL deduplicates before numbering and has one `INSERT ... ON CONFLICT DO NOTHING` (`verifier_runs.rs:130-167`, `1640-1706`) | `observation_batches_insert_once_per_binding_and_record` covers multiple subjects/bindings, duplicate requests, inactive owner, and ordinal continuity (`pg_verifier_runs.rs:1858-1967`); recorded focused test PASS | PASS |
| REQ-108: only runtime-active exact owners create binding-driven Eval runs | `accepts_records` evaluates `binding_activity` for the exact locked binding before target resolution and insertion (`verifier_runs.rs:1709-1735`) | The batch SQL test includes a stale owner that creates no run (`pg_verifier_runs.rs:1906-1966`) | PASS |
| REQ-077 / AC-014: no count limit; a slow or unavailable PostgreSQL write is retained and retried; recovery produces exactly one run; shutdown flushes or reports observable loss | Required queue/retry/shutdown semantics are delegated unchanged to generic `Outbox`; server shutdown retains and drains the alias (`app/server.rs:873-878`) | `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown` asserts two pending through refusal, recovery plus replay deduplication, successful shutdown flush, and one reported deadline loss (`pg_verification_runtime.rs:2411-2488`); recorded focused test PASS | PASS |
| REQ-077 ownership constraints: Scribe/`vala-sql` do not write `verifier_runs`; no cross-crate transaction | Gate only stages the server-owned request; sink opens `WyrdPostgres::tenant_conn`; durable insert remains in `wyrd-sql` | Source/caller search and changed-file diff | PASS |
| AC-014 continuous Eval journey remains meaningful under revision 60 | Terminal matrix still proves the authored Eval matrix, frozen managed day, execution outcomes, detail rows, gates, dispatch, media, and trace behavior; only the pre-revision-60 permanent-refusal/drop phase was removed (`eval_verification.rs:630-1006`) | Recorded `test:bifrost:journey:server` 31/31 PASS | PASS |
| AC-014 replay proof detects any replay request and proves the durable result | With the run table locked, the original request remains pending/in flight; after both replay acknowledgements and one sentinel acknowledgement, `pending() == 2` is exact because Gate stages synchronously before returning. The test then proves exactly four runs per non-replay record and the original frozen event day (`eval_verification.rs:1010-1148`) | Recorded journey lane PASS; source establishes the barrier and downstream durable assertions | PASS |
| Scoped merge resolution preserves non-blocking audit staging through the one `AuditOutbox` | The combined conflict resolution retains `.with_audit(Arc::clone(&audit_outbox))`, retains both `observation_runs` and `audit_outbox` in `BifrostComposition`, and Gate stages allowed/denied decisions without awaiting an audit commit (`c5527627a`, `boot/mod.rs`, `state.rs`, `gate/mod.rs:490-518`) | `git show --cc c5527627a` and both-parent comparison | PASS |
| Scoped merge resolution removes audit-unavailable public error codes | Merge-side diff removes `WYRD_AUDIT_503_UNAVAILABLE`, `WYRD_VALA_500_AUDIT_UNAVAILABLE`, and `WYRD_VALA_503_QUERY_AUDIT_UNAVAILABLE` from the error catalog/client projections; the removed protobuf number/name are reserved | Source search finds no such public error code or error variant. `AuditEventFailureReason::AuditUnavailable` is an audit-event detail value, not a public operation error. | PASS |
| Scoped merge resolution keeps TASK-013/014's SYSTEM-token removal | Merge resolution rejects the audit parent's SYSTEM-token test additions and retains the tokenless internal capture path; current token verification/issuance docs state that the tenant SYSTEM principal never holds a token | Combined diff for `wyrd-auth/src/issuance.rs`; source search finds no `issue_system_token` | PASS |
| No regression or prohibited scope expansion | State, boot, shutdown, active-owner filtering, tenant transaction ownership, metrics label, and exact replay behavior remain connected; no generic outbox source changed | Complete TASK-015 diff, scoped combined merge diff, and task-recorded targeted lanes | PASS |

## Review of the two journey edits

### Removed permanent-refusal pre-phase

This edit follows revision 60 and does not weaken a required proof. The deleted
phase installed a trigger that permanently refused one observation-run insert
and then asserted that the observation never acquired a run. Under the approved
outbox contract, a failed PostgreSQL flush retains its tenant batch and retries;
the old assertion depended on the pre-outbox drop behavior and conflicts with
REQ-077 when interpreted as an outage test. AC-014's required failure scenario
is a *forced PostgreSQL outage during a flush* followed by recovery, exact-once
creation, replay deduplication, and shutdown drain. Those behaviors have direct
coverage in `observation_outbox_retains_through_an_outage_and_flushes_at_shutdown`.
The terminal matrix continues to prove its actual Eval execution outcomes.

### Replaced blocked-backend count with exact pending count

This edit preserves and sharpens the proof for the revised architecture. The
old barrier assumed one PostgreSQL transaction per acknowledged frame. The
generic outbox permits only one in-flight write per tenant, so later requests
correctly remain in memory and cannot appear as blocked PostgreSQL backends.
Holding `wyrd.verifier_runs` locked prevents the original request from
completing; the synchronous Gate hook guarantees each acknowledged first
commit is already counted by `pending()`. Consequently, exact pending count
`2` proves that only the original and sentinel were staged, while `3` or `4`
would expose a replay request. The later run-count and frozen-day assertions
continue to prove the durable outcome, so this is not a replacement of a
durability assertion with an internal-only assertion.

## Proposed findings

None.

## Open risk classification

A permanently failing item can retain and block that tenant's batch and later
tenant requests. This is **not a finding against TASK-015**. Revision 60
requires retention and retry specifically when PostgreSQL is slow or
unavailable and intentionally defines an all-or-nothing per-tenant sink write;
it does not require poison-item isolation, dead lettering, error
classification, or progress past a permanent constraint/data failure. Adding
one of those policies would change approved concurrency/durability semantics
and the shared generic-outbox boundary. The task's recoverable-outage proof is
the required case.

## Verification limits

- Per user direction, this reviewer did not run Cargo, mise, or journey tests.
- The implementation record reports `fmt`, `lints`, SQL, server integration,
  focused PostgreSQL tests, and the 31-test server journey lane passing. Those
  results were treated as claims and checked for fit against the inspected
  source, not as substitutes for source review.
- `git diff --check 3f8767a5f..9b560d005` is clean.
- Generic-outbox internal algorithms and previously passed TASK-013, TASK-014,
  audit-outbox, and benchmark behavior were not reopened.

## Overall result

**PASS**

The cumulative scoped candidate satisfies TASK-015's revision-60 obligations,
the two journey edits express rather than weaken the approved outbox contract,
the scoped merge resolution preserves both branches' required intent, and no
task-local invariant finding remains.
