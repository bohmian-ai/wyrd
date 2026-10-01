---
id: TASK-006-R1
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-077, REQ-079, REQ-083, REQ-084, REQ-111, REQ-130, REQ-131, INV-004, INV-010, INV-012, AC-014, AC-016, AC-027]
depends_on: [TASK-006]
parent_task: TASK-006
remediates: [FIND-TASK-006-1, FIND-TASK-006-2, FIND-TASK-006-3, FIND-TASK-006-4, FIND-TASK-006-5, FIND-TASK-006-6, FIND-TASK-006-7, FIND-TASK-006-8, FIND-TASK-006-9, FIND-TASK-006-10]
---

# Continuous Eval closure

## Authority and subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35, together with the repository owner's explicit System-principal decision below (recorded as approved revision 36).
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`.
- Review and source evidence: `changes/active/verified-change-contract/review/TASK-006-r1/verdict.md` and `findings-validation.md` in the same directory.
- Immutable reviewed base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Immutable reviewed candidate: `55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`.
- Review-artifact commit: `ce8001fbdaefa3d23329323298553dcd0eb432cc`. The owner accepted intervening skill-documentation commit `01146cf87d22147b87d0c9224aa2bdf67decad92`; it did not change the named candidate's product source.

The owner resolved the two review blockers:

> (1) Checkout instability: 'is not a blocker. i approve the commit' -- commit 01146cf8 (skill docs only) is accepted; the named base f8811ac5 / candidate 55c5bff8 review stands.

> (2) FIND-TASK-006-7: 'That is an anti-pattern why are you creating a new user. If this is a server runtime/machinary that is concstantly running and by the nature of its design, doesnt have a direct principal, then it needs to use a system principal (per tenant)'. Approved decision: continuous Eval's internal Bifrost reads use the existing per-tenant PrincipalKind::System principal (stable id, credentialless, no public lifecycle), extended with a narrow server-minted read scope for continuous Eval inputs; never a fabricated User principal.

## Outcome

Make continuous Eval's acknowledged observation, durable run, input reads,
existing Eval execution, and public evidence satisfy all ten validated
findings. Preserve the terminal matrix and accepted best-effort enqueue
ceiling. Update the approved specification and security posture to record the
owner's narrow System-principal read decision alongside its existing
result-write capability.

## Diagnoses and required corrections

### FIND-TASK-006-1 — Sealed replay can freeze the wrong event day

**Obligation and defect.** REQ-077, REQ-079, INV-004, AC-014, and original
Scenario 4 require the run to retain the committed row's exact managed
`wyrd_event_time`. In
`crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:346-357,492-582`, each
ingest attempt gets a fresh receipt instant. Scribe already detects
`AlreadyCommitted` at its batch fence
(`scribe/shards.rs:3623-3710`), but Gate invokes the observation hook for
both first ACK and replay (`gate/mod.rs:979-993`). The hook substitutes the
new attempt receipt for an unstamped frame
(`tables/eval/observations.rs:92-120`). An asynchronous replay hook can win
the unique run-insert race and freeze a different day. The existing SQL replay
test passes an unchanged time twice; the real journey sends no sealed replay.
The runner then prunes the wrong UTC day and can settle a valid observation
`errored`.

**Correction.** Carry Scribe's existing committed-versus-replay disposition
through the ACK and activate the existing post-ACK enqueue only for the ACK
that inserted the batch. Keep enqueue asynchronous and outside the batch-fence
transaction. A failed first activation may still lose the run under the
approved best-effort ceiling. Do not add an outbox, replay timestamp store, or
Bifrost poller.

**Closure proof.** A real Gate/Scribe/Postgres case replays identical unstamped
batch ID and bytes after a different receipt day, including concurrent hook
scheduling, and proves one activation, one run, exact equality with the stored
row's event time, and successful read from the original day.

### FIND-TASK-006-2 — Sampling changes after retry

**Obligation and defect.** REQ-083, INV-010, and AC-016 require one durable
run's `every_nth` decision to survive retry and restart.
`crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:269-279,1295-1312`
recounts visible prior runs on every attempt; the Eval adapter rereads it at
`crates/wyrd/wyrd-server/src/verification/eval.rs:169-198`. A concurrently
inserted earlier row can become visible later and change the count. The
existing pure modulo test cannot prove durable ordering; a run can switch
between execution and sampled-out completion.

**Correction.** In the existing `VerifierRunQueue` enqueue owner, serialize
creation for each binding and assign one immutable observation ordinal in the
insert transaction. Read that stored ordinal on every attempt; preserve
idempotent duplicate handling. Do not introduce a separate sampling service
or recalculate historical rank.

**Closure proof.** A Postgres concurrency test holds an earlier enqueue
uncommitted while a later enqueue commits, then commits the earlier one and
proves the later run's ordinal and sampling result remain fixed across a
second attempt/restart.

### FIND-TASK-006-3 — Media size check misses the effective body

**Obligation and defect.** REQ-131 and AC-027 require bounded bytes and
oversized-media refusal before provider work. `TenantMedia::resolve`
(`crates/wyrd/wyrd-server/src/verification/eval.rs:694-752`) checks
`object_len`, then `StorageHandle::get_object`
(`crates/wyrd/wyrd-storage/src/handle.rs:272-303`) reads the whole body
without a bound or post-read check. Replacement or stale metadata between
operations can cause excessive allocation and provider input. Existing tests
cover only an object already oversized at metadata lookup.

**Correction.** Reuse the existing StorageHandle and installed OpenDAL
capability for a bounded body read of at most the limit plus one sentinel
byte. Reject overflow before base64 encoding or provider invocation.
Metadata may remain a fast rejection, but it is not the authority for the
returned bytes. Keep tenant, MIME, and kind validation.

**Closure proof.** A deterministic storage-backed resolver case makes
metadata report an allowed size while the body exceeds 20 MiB, and proves
overflow is refused with at most limit-plus-one bytes retained and no provider
call.

### FIND-TASK-006-4 — Trace events and links are replaced with empty evidence

**Obligation and defect.** REQ-083, REQ-111, and INV-012 require continuous
Eval to retain the existing trace assertion semantics.
`crates/wyrd/wyrd-server/src/verification/eval.rs:394-496` omits persisted
event/link fields from its query and builds empty vectors and zero dropped
counts. The existing trace task at
`crates/vala/vala-eval/src/tasks/trace.rs:124-146` exposes those fields to
authored selectors, and Bifrost already persists them. Current one-span
journeys do not select an event or link. Valid assertions can therefore
evaluate fabricated evidence and produce false verdicts.

**Correction.** Extend the existing BifrostReader trace projection and
decoder to reconstruct canonical persisted events, links, and dropped counts
into the existing `SpanRecord`. Keep the Vala trace executor and table model.

**Closure proof.** A real-server Eval journey exports a span with an event
and link, selects both through the existing trace task, and checks the
canonical item and common verdict.

### FIND-TASK-006-5 — Multi-span assertions lack a stable order

**Obligation and defect.** REQ-083 and evaluation reproducibility require
the same committed trace to yield the same positional evidence after retry.
The span query at `crates/wyrd/wyrd-server/src/verification/eval.rs:394-423`
has no `ORDER BY`, while the existing trace executor supports selectors
such as `$.spans[0]`. Physical Arrow/Parquet order may change; the shipped
journey tests only one span.

**Correction.** Give that query one total order by persisted start timestamp
and span ID. Keep ordering at the query owner; do not add a second in-memory
sort or configuration setting.

**Closure proof.** A multi-span real-server case inserts spans in reverse
logical order and proves the positional assertion agrees on repeated reads
and after runtime restart.

### FIND-TASK-006-6 — Required cross-day journey is missing

**Obligation and defect.** AC-014 and original Scenario 5 require a real
SDK-to-server journey where client `created_at` and managed event time fall
on different UTC days. The existing helper in
`crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:215-226`
does not author `created_at`; its assertion at lines 762-780 proves only
row/run event-time equality. A regression that reads by client creation day
can leave the journey green.

**Correction.** Extend the existing journey's observation-authoring path to
set `created_at` on a different UTC day. Reuse the current SDK and server
harness.

**Closure proof.** The run completes and its exact observation/result are
read using the frozen managed day while the test asserts the authored
`created_at` day differs.

### FIND-TASK-006-7 — Eval reads fabricate a user and audit identity

**Obligation and defect.** The security posture requires attributable,
authorized internal Oracle reads. `BifrostReader::new`
(`crates/wyrd/wyrd-server/src/verification/eval.rs:270-301`) constructs a
new UUIDv7 `PrincipalKind::User`, self-grants `bifrost_query:read`, and
passes it to Oracle. The reader uses that identity for both observation and
trace reads. The tenant is bounded, but the audit principal has no stored
identity or revocable authority. Current spec text at
`changes/active/verified-change-contract/spec.md:110-115,907-940` and
security-posture text at
`architecture/wyrd-security-posture.md:68-75,140-146` describe the existing
per-tenant System principal as result-write-only. The candidate and its tests
therefore cannot prove stable, authentic read attribution.

**Owner-approved correction.** Reuse that same persisted per-tenant
`PrincipalKind::System` principal for continuous Eval's observation and
trace reads, with a narrow server-minted read scope restricted to those inputs.
Resolve its stable ID from tenant-owned state and use the existing Oracle
authorization and audit path. Keep result publication's exact-Verifier
write-only scope distinct from the Eval read scope. Missing System identity,
wrong tenant, or insufficient read scope fails closed before rows are
returned. Keep the principal credentialless and absent from public principal,
token-exchange, grant, workload, delegation, and lifecycle surfaces. Update
the spec definition and REQ-086 text and the security posture's principal
and token-scope paragraphs to state the approved separately scoped read and
write uses. Follow the spec's revision/approval metadata workflow while
recording this already explicit human approval; do not silently leave
contradictory authority text. No new identity store, principal kind, user,
public permission, token format, or general Bifrost query grant is needed.

**Closure proof.** Through a real Oracle read, assert the audit row names the
same stored tenant System principal across repeated reads and restart.
Missing, wrong-tenant, and under-scoped System authority must return no rows
and show the expected refused decision. Existing result-write-only token
scope and System public-surface exclusion tests remain green.

### FIND-TASK-006-8 — Trace read has unbounded time and row volume

**Obligation and defect.** AGENTS.md §10 and Bifrost resource rules require
bounded analytical reads. `BifrostReader::spans`
(`crates/wyrd/wyrd-server/src/verification/eval.rs:389-423`) has only a
lower event-time predicate and no row ceiling. The existing
`ScheduledQueryCaller` collects the full result, so a trace with many
future rows can repeat expensive allocation on every retry. Existing
terminal-matrix cases do not exercise either bound.

**Correction.** In the existing Eval trace query, close the time interval
using the record/trace deadline and apply a fixed server-owned maximum span
count with one sentinel row. Overflow is a trace-source execution error on
the existing retry path. Keep the generic scheduled-query collector and
avoid a new streaming framework or user-facing setting.

**Closure proof.** A real query case proves both time predicates constrain
the plan, the ceiling decodes, and ceiling-plus-one rejects before
task/provider execution.

### FIND-TASK-006-9 — Raw dependency errors escape in public status

**Obligation and defect.** The `VerificationError` contract and repository
error authority require secret-free, stable public errors.
`crates/wyrd/wyrd-server/src/verification/eval.rs:120-156,169-198,242-267,633-664,694-752`
converts SQL, Bifrost, registry, storage-locator, and provider errors with
`to_string()` into a public message. The runner persists/logs it
(`verification/runner.rs:565-570`) and run status exposes it
(`components/verification/service.rs:183-220`). Storage errors may contain
object keys or private URI material; existing media/provider tests only
inspect provider bodies.

**Correction.** Use the existing EvalEngine error boundary and stable codes
to produce safe, operation-specific public text. Keep raw causes only in
protected structured diagnostics, redacting private locators there as
required. TenantMedia may expose a non-sensitive binding ID and category,
not the URI, key, or backend `Display`. Do not create another error catalog
or public detail field.

**Closure proof.** Inject sentinel strings through Bifrost/SQL,
storage-locator, and provider failures. After settlement, status and
persisted `VerificationError` retain stable codes without sentinels; log
capture confirms only safe diagnostic detail is emitted.

### FIND-TASK-006-10 — Changed fan-out lacks required contract docs

**Obligation and defect.** AGENTS.md §16 and `architecture/agent-rules.md`
require intent, errors, and cancellation behavior for materially modified
async Rust items. `crates/vala/vala-eval/src/executor.rs:385-418` changes
`fan_out_bucket` to propagate the first executor/join error but gives no
rustdoc, `# Errors`, or explanation of sibling-task cancellation when
the `JoinSet` is dropped. Lints passing does not close the repository's
documentation rule.

**Correction.** Document the existing helper's bucket role, first-error
path, and cancellation/partial-progress consequence. Keep its current
execution shape.

**Closure proof.** Inspect the touched item against AGENTS.md §16, run
`mise run lints`, and keep the existing executor-error propagation
regression green.

## Preserved boundaries and non-goals

- Preserve one Vala Eval scoring and judging path, current pass gate,
  sampled-out and trace-wait terminal matrix, capture policy, canonical
  result/item tables, generic retry/dispatch machinery, tenant isolation,
  and real SDK/server/provider flows.
- Keep post-ACK enqueue best-effort and outside Scribe's batch-fence
  transaction. Its failure cannot delay, roll back, or fabricate an
  observation ACK.
- Preserve the existing System principal's stable UUIDv7 identity,
  credentialless internal lifecycle, exact-Verifier result-write scope,
  and public exclusion. The approved Eval read scope is separate and
  restricted to continuous Eval inputs.
- No outbox, Bifrost queue polling, offline dataset execution, second Eval
  engine/judge, provider file lifecycle, new public auth surface, broad
  System query grant, or speculative configuration knob.

## Acceptance and proof

Each numbered diagnosis above is one acceptance obligation: its stated
correction must be present in the named owner and its focused proof must
exercise the failing path, including negative/error behavior. In particular,
the read-authority proof for `FIND-TASK-006-7` must verify the real Oracle
audit identity and fail-closed tenant/scope checks, and the authority texts
must agree with that behavior.

Run one scenario at a time using the repository's RED → GREEN → REFACTOR
discipline. Record the failure's trace and root cause before changing a
test, assertion, timeout, retry, sleep, skip, or allowance. For every new
specifically named test, record and run its exact focused `mise exec -- cargo
nextest run --locked -p <crate> --lib|--test <target> -E
'test(=<exact-name>)'` command, including the repository-managed environment
wrapper when required; confirm the selector runs a test.

Rerun these existing focused engine regressions:

```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=executor::end_to_end::full_spec_against_in_memory_record_and_spans_aggregates_correctly)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=tasks::trace::trace_executor::trace_assertion_unavailable_errors)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=tasks::judge::llm_judge_executor::retry_budget_exhausted_errors)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=results::results_aggregation::context_capture_redact_drops_actual)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=results::results_aggregation::pass_gate_all_pass_on_all_skipped_subject_set_fails)'
```

Then run the smallest environment-owning `mise` tasks that cover the
Scribe/SQL/storage/auth/Oracle and SDK/server journeys, plus the applicable
repository gates. The original task's broad evidence set is the starting
point: `mise run test:vala`, `mise run test:sql`, `mise run test:wyrd`,
`mise run test:bifrost`, `mise run test:wyrdstate:journey`, `mise run fmt`,
and `mise run lints`. Run `mise run codegen:check` if the authority or
contract edits affect generated schema/contract output, and the matching
boundary checks if touched. Finish with `git diff --check`. A red required
gate blocks completion; do not weaken or suppress it.

Implementation records the approved authority update and verification
evidence. The next `$wyrd-task-review` must inspect the complete cumulative
base-to-new-candidate range against the original TASK-006, this remediation,
and the preserved stable finding IDs.

## Evidence — FIND-TASK-006-1

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scribe's committed-vs-replay disposition reaches the ACK; Gate activates the post-ACK enqueue only for the ACK that inserted the batch; enqueue stays async and outside the fence transaction | `scribe/shards.rs` (`GroupWalState::inserted_batch_ids`, `insert_committed_group`, `acknowledge_visible`), `scribe/preprocess.rs` (`DurableCompletion`), `scribe/ingress.rs` (`FrameAdmission.first_commit`), `contracts.rs`, `gate/mod.rs` (hook gated on `admission.first_commit`) | `eval_verification::sealed_replay_on_a_later_day_activates_once`: RED with the Gate filter removed (`"3 activations for one original and one sentinel"`), GREEN with it | PASS |
| Real Gate/Scribe/Postgres replay of identical unstamped batch ID and bytes after a different receipt day, with concurrent replays: one activation, one run per binding, run event time == stored `wyrd_event_time`, row read from the original day | `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`; test-support receipt shift `ScribeImpl::shift_receipt_clock_for_test` (`scribe/ingress.rs`) | same test (run inserts held behind a `SHARE` lock; lock waiters count activations; sentinel frame is the barrier) | PASS |
| Retained post-commit retry ACK still reports first commit | `scribe/shards.rs` test assertion | `scribe::shards::tests::post_commit_insertion_failure_retries_without_black_hole_or_early_ack` | PASS |
| No regression in existing Eval journey, server and Scribe journeys, Scribe unit tests | — | `mise run test:bifrost:journey:server` (18/18), `mise run test:bifrost:journey:scribe` (21/21), redux `--lib` under Postgres (925/925) | PASS |

Commands run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::sealed_replay_on_a_later_day_activates_once)"'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::shards::tests::post_commit_insertion_failure_retries_without_black_hole_or_early_ack) | test(=scribe::shards::tests::accepted_mailbox_material_remains_owned_until_visible_ack)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p vala-bifrost-redux --lib'
mise run test:bifrost:journey:server
mise run test:bifrost:journey:scribe
mise run fmt
mise exec -- cargo clippy --locked -p vala-bifrost-redux -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
git diff --check
```

No outbox, replay-timestamp store, or poller was added. A failed first activation still loses its runs under the approved best-effort ceiling.

## Evidence — FIND-TASK-006-2

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `VerifierRunQueue` enqueue serializes observation-run creation per binding and assigns one immutable ordinal in the insert transaction | `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs` (`LOCK_OBSERVATION_BINDING_SQL` binding row lock `FOR NO KEY UPDATE` taken in `VerifierRunQueue::insert` before `INSERT_RUN_SQL`, which stores `max+1` as `observation_ordinal`); migration `crates/wyrd/wyrd-sql/migrations/20260601000032_verifier_run_observation_ordinal.sql` (column, creation-order backfill, shape CHECK, per-binding unique index) | `pg_verifier_runs::observation_ordinal_is_fixed_at_serialized_enqueue`: RED before the change (`a later enqueue for the binding waits for the uncommitted earlier one`, `left: Some(run)`, i.e. the later run committed while the earlier was uncommitted), GREEN after | PASS |
| Every attempt reads the stored ordinal; recount query deleted | `OBSERVATION_ORDINAL_SQL` and `VerifierRunQueue::observation_ordinal` removed; `CLAIM_RUN_SQL` returns `observation_ordinal` into `ClaimedRun::observation_ordinal`; `crates/wyrd/wyrd-server/src/verification/eval.rs` `sampled` reads `run.observation_ordinal` (no Postgres read, `EvalEngine.queue` removed) | same test: ordinals `(1, 2, 3)` by serialized position; the later run released and reclaimed on fresh runtime handles reads ordinal 2 again, so `every_nth` (pure over record, context, ordinal) cannot flip | PASS |
| Idempotent duplicate handling preserved; duplicates consume no ordinal | `ON CONFLICT DO NOTHING` stores nothing, `EXISTING_RUN_SQL` path unchanged | same test (duplicates of records 1 and 2 answer `AlreadyEnqueued`; record 3 gets ordinal 3); `observation_enqueue_targets_active_ready_bindings_once`; `observation_runs_are_unique_per_input_record` | PASS |
| FIND-1 replay journey still proves one activation | Its lock probe counted only `RowExclusiveLock` waiters on `wyrd.verifier_runs`; the sentinel activation now correctly waits on the binding lock. Probe `blocked_activations` now counts blocked backends of the test's own database (`pg_blocking_pids`, `datname = current_database()`), which still counts a wrongly activated replay | `eval_verification::sealed_replay_on_a_later_day_activates_once` GREEN focused and in `mise run test:bifrost:journey:server` | PASS |

Diagnosis (replay journey red after the change). **Symptom:** `only 1 of 2 activations reached the queue`. **Evidence:** trace shows the original activation's INSERT blocked 120 s behind the test's SHARE lock; `blocked_activations` filtered on `mode = 'RowExclusiveLock' AND relation = 'wyrd.verifier_runs'`. **Cause:** the sentinel activation (same subject, same bindings) now waits on the original's binding row lock, not the table lock, so the probe did not see it. **Fix site:** the test probe (the serialization is the intended behavior); an independent read-only diagnostician reached the same cause and fix site. The first lane rerun then counted sibling journeys' blocked backends (`5 activations`) because `pg_stat_activity` is cluster-wide; restricted to `current_database()`. `pg_verification_runtime` probes cover scheduled/runner paths without the observation lock and stay green (19/19).

Open blocker, not caused by this finding: `eval_verification::continuous_eval_runs_the_terminal_matrix` failed in 2 of 3 `test:bifrost:journey:server` runs. The `eval-gated` foreign-media run's third attempt hit `QueryAdmissionRejected` (fail-fast Oracle planning semaphore, 2 permits vs 4 concurrent runner slots) on its record read. That retry spent the last attempt and overwrote `eval_execution_failed` with `eval_record_unavailable`. An independent diagnostician confirmed the diff does not touch this path (no `every_nth` there; the removed Postgres read ran only for `every_nth`). Fix site is `crates/wyrd/wyrd-server/src/verification/eval.rs` error classification: admission rejection should requeue without spending an attempt. That overlaps FIND-TASK-006-9's error-boundary work, so it is left for that finding.

Commands run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E "test(=observation_ordinal_is_fixed_at_serialized_enqueue)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs -E "test(=observation_ordinal_is_fixed_at_serialized_enqueue) | test(=observation_enqueue_targets_active_ready_bindings_once)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::sealed_replay_on_a_later_day_activates_once)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test pg_verification_runtime'
mise run test:sql
mise run test:bifrost:journey:server
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-sql -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
git diff --check
```

`codegen:check` not run: no schema, stub, MCP, or error-catalog output changed. Verifier-run queries are raw `sqlx::query` (see the module's raw-query allowlist), so there is no offline sqlx metadata to regenerate. No sampling service was added and no historical rank is recalculated.

## Evidence — admission refusal blocker (from FIND-TASK-006-2)

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Oracle admission refusal of an Eval input read is backpressure and does not spend a verification attempt | `verification/engines.rs` `EngineOutcome::Deferred`; `verification/eval.rs` `ReadError` (typed match on `WyrdError::Vala { error: BifrostError::QueryAdmissionRejected }` → `Deferred`, everything else `Retry`) for record and span reads; `verification/runner.rs` `Transition::Defer` settled through the existing refunding `VerifierRunQueue::release`, now taking a delay (`RELEASE_RUN_SQL` `$3`, shutdown drain passes zero, deferral passes `poll_interval`), outcome label `deferred` | `pg_verification_runtime::deferred_engine_admission_requeues_without_spending_attempts`: RED (no `Deferred` outcome; compile failure), GREEN: four deferrals then completion settle `completed` with `attempts = 1`, no error | PASS |
| The terminal matrix is stable | — | `eval_verification::continuous_eval_runs_the_terminal_matrix` traced after the fix: 7 `deferred by admission backpressure` lines, 0 `attempt failed … eval_record_unavailable`; `mise run test:bifrost:journey:server` green twice in a row (20/20 each) | PASS |
| Existing release callers unchanged | shutdown drain and late-claim refund pass `Duration::zero()` | `pg_verification_runtime` 20/20, `pg_verifier_runs` 17/17 | PASS |

Diagnosis. **Symptom:** `continuous_eval_runs_the_terminal_matrix` intermittently ended the `eval-gated` foreign-media run with `eval_record_unavailable` instead of `eval_execution_failed`. **Evidence:** with `WYRD_LOG=info,wyrd_server::verification=debug`, three runs of the test each logged `verification attempt failed … code=eval_record_unavailable query admission rejected` for several concurrently claimed runs (e.g. run `01a0d0f0-6a9d-…` at 01:03:32.827 after two `eval_execution_failed` attempts); Oracle's `QueryPlanner::try_planning` (`vala-bifrost-redux/src/oracle/planner.rs`) is `try_acquire_owned` on a small planning semaphore and returns `QueryAdmissionRejected` immediately, by design (`architecture/bifrost-design.md` "Admission and memory": refusal rather than an unbounded queue), while the runner claims up to `tenant_permits` (4) runs at once. **Cause:** `eval.rs` stringified every read error into `EngineOutcome::Retry`, so shared-capacity refusal was charged to the run; on the final attempt it replaced the real terminal error. **Fix site:** the engine/runner outcome contract (`EngineOutcome` → `Transition`) plus the queue's existing attempt-refunding release; Oracle's fail-fast admission is the approved design and was not changed. Other callers checked: `ScheduledQueryCaller` is used only by the Eval reader; Drift does not read Bifrost; `refund_late_claim` and shutdown drain keep an immediate release. The earlier independent diagnostician's cause and fix site (recorded under FIND-TASK-006-2) match. No sleep, timeout, or retry count was changed.

Commands run:

```bash
WYRD_LOG=info,wyrd_server::verification=debug,vala_bifrost_redux::oracle=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --no-capture -E "test(=eval_verification::continuous_eval_runs_the_terminal_matrix)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test pg_verification_runtime -E "test(=deferred_engine_admission_requeues_without_spending_attempts)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test pg_verification_runtime && cargo nextest run --locked -p wyrd-sql --test pg_verifier_runs'
```

## Evidence — FIND-TASK-006-4

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| BifrostReader projects persisted `events`, `links`, and the three dropped counts and decodes them into the existing `SpanRecord` (`SpanEvent`, `SpanLink`); Vala trace executor and table model unchanged | `crates/wyrd/wyrd-server/src/verification/eval.rs` `BifrostReader::spans` projection; `span`, `event`, `link`, `nested` (list-of-struct elements viewed as a batch, reusing the scalar readers), `attribute_column`, `count` | `eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence`: tasks select `$.spans[0].events[0].name`, `…events[0].attributes.doc`, `…links[0].span_id`, `…links[0].dropped_attributes_count`; verdict `passed`, 6 items, canonical items exactly `event="retrieval"`, `event_doc="d1"`, `link="2b3c4d5e6f708192"`, `link_dropped=2` (+ order items). Before the change the projection built empty vectors, so these selectors resolved to null | PASS |

## Evidence — FIND-TASK-006-5

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| The span query has one total order by persisted start timestamp and span ID at the query owner; no in-memory sort or setting | `eval.rs` `ORDER BY start_time_unix_nano, span_id` | same test exports `second-span` then `first-span` (reverse start order) in one request; `first=$.spans[0].name`, `second=$.spans[1].name` pass. Mutation proof: with `ORDER BY` removed the test fails (`read [Some("failed")]`); a debug read showed physical order `second-span, first-span` | PASS |
| Positional evidence agrees across repeated reads and after runtime restart | — | same test: a second in-window record over the same committed trace is scored by a freshly spawned runtime; its canonical items equal the first run's | PASS |

## Evidence — FIND-TASK-006-8

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| The trace read's event-time interval is closed at the record/trace deadline | `EvalEngine::new` takes the runtime `trace_deadline` (`verification/mod.rs` passes `limits.trace_deadline`) and keeps `trace_window = trace_deadline + 1 day`; the span read covers `[start of (record day − 1), end of the day containing event_time + trace_window)` | `continuous_eval_reads_ordered_bounded_trace_evidence`: spans land today; a record caller-stamped five days ago (spans after its window) and a record received three receipt days ahead (spans before its window) both settle `timed_out` with no result, while the in-window record passes. RED before the upper bound: the five-days-ago record `completed` with a result | PASS |
| Fixed server-owned span ceiling with a `LIMIT max+1` sentinel; overflow is a trace-source execution error on the retry path before task/provider execution; no config knob or streaming | `eval.rs` `pub const TRACE_SPAN_LIMIT: usize = 10_000`, `LIMIT {TRACE_SPAN_LIMIT + 1}`, row count checked before any row decode → `ReadError::Failed` → `Retry(eval_trace_unavailable)` | `eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling`: 10 000 spans decode and the run passes (trace task + judge); 10 001 spans retry three times with `trace … exceeds 10000 spans` and settle `errored`/`eval_trace_unavailable` with no result; the provider saw exactly one call (the ceiling run). Mutation proof: with the check disabled the overflow run `completed` | PASS |

Diagnosis (new evidence journey, first GREEN attempt). **Symptom:** `QueryVisibilityUnavailable` on record reads and on the test's own query. **Evidence:** trace shows `private Scribe tail listing failed … tail authorization failed: tail ticket replay detected`; `crates/wyrd/wyrd-server/src/oracle/tail_authority.rs` returns that error both for a reused nonce and when the replay cache holds `replay_capacity` (256) unexpired nonces, each retained up to `MAX_TICKET_TTL` (30 s). **Cause:** two runs awaiting an out-of-window trace, polled every 200 ms by the shared journey runtime, spent single-use tail tickets faster than they expire and filled the bounded cache, so every Oracle read with a hot tail was refused. **Fix site:** the new journey's runtime (`spawn_runtime_polling`, 2 s trace poll for `TraceJourney`); the matrix keeps its 200 ms poll. No production limit changed. **Risk noted for review:** sustained tail-reading query rate per server is bounded by 256 nonces / 30 s; production `trace_poll` is 5 s, but many runs awaiting traces could approach it.

A lower test-only ceiling seam was not needed; ceiling-plus-one is exported as six 2 000-span OTLP requests.

Commands run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_refuses_a_trace_over_the_span_ceiling)"'
mise run test:bifrost:journey:server   # twice, 20/20 both times
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-server -p wyrd-sql -p wyrd-testing --all-features --all-targets -- -D warnings
git diff --check
```

`codegen:check` not run: no schema, stub, MCP, or error-catalog output changed (`TRACE_SPAN_LIMIT` is a Rust constant; error codes are the existing `eval_*` codes). Spans' OTLP receipt path ignores `ScribeImpl::shift_receipt_clock_for_test` (only native transport frames honor it), so the lower bound is exercised by shifting the record ahead and the upper bound by a caller-stamped past record.

## Evidence — FIND-TASK-006-7

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Eval observation and trace reads use the persisted per-tenant System principal; the UUIDv7 `User` + self-granted `bifrost_query:read` construction is deleted | `crates/wyrd/wyrd-server/src/verification/eval.rs` `EvalReadAuthority::resolve` reads `system_principal_id` (active, UUIDv7) under the tenant RLS bind and builds `PrincipalKind::System` with an empty Card scope, no roles, no credential; `BifrostReader::new` binds its `ScheduledQueryCaller` to that context | `eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence`: RED before the change (`Eval input reads must be audited as the tenant System principal …: []`), GREEN after: every retained `bifrost.query.read_decision` with `principal_kind = 'system'` names `seed.system_principal()`, and the count grows across the runtime restart with the same ID | PASS |
| Narrow server-minted read scope, separate from the result-write token; no general grant | `EVAL_INPUT_TABLES`; one `bifrost_query:read` grant per existing input table scoped `Bifrost(Table{vala, schema, table_uid})` from `vala.bifrost_tables` (a not-yet-created span table gets no grant and stays not-found); no token is minted; `issue_system_token` (write) unchanged | `eval_verification::continuous_eval_read_authority_fails_closed`: resolved principal has exactly two table-scoped read grants, `System{card_ref_scope: empty}`, no roles or credential; reads its own observation (1 row); `vala.system.audit_log` is refused `QueryForbidden` | PASS |
| Missing System identity fails closed before rows | `SystemPrincipalMissing` → engine `Retry(eval_record_unavailable)` before any Oracle call | same test: with the stored row relabelled out of `system`, `resolve` returns `SystemPrincipalMissing`; the run ends `errored`/`eval_record_unavailable`, no result, no dispatch, and no System read decision is retained | PASS |
| Wrong tenant fails closed | `AuthorizedQueryContext::try_new` tenant invariant; RLS-bound resolution | same test: the System principal presented for another tenant is refused `QueryTenantInvariant`; a tenant with no System principal resolves no authority | PASS |
| Under-scoped authority fails closed and is audited as refused through the existing path | `crates/wyrd/wyrd-server/src/query/scheduled.rs` `ScheduledQueryCaller::record_object_denial` records Oracle's `QueryForbidden` via `QueryAuthority::record_object_denial` (`query/service.rs`, now `pub(crate)`), the same `vala.query.sync` denial the public entry writes | same test: the authority narrowed away from `vala.traces.spans` is refused; retained audit holds exactly one allowed read decision and two `vala.query.sync` `denied` rows, all naming the stored System principal. Mutation proof: with the denial append removed the test fails (`…, []`) | PASS |
| Result-write-only token scope and System public-surface exclusion stay green | — | `mise run test:principals:integration` (includes `pg_admin_principals::pg_tests::system_principal_is_absent_from_public_principal_paths`, `…provisioning_is_idempotent_and_stable`, `issuance::pg_tests::issue_system_token_round_trips_one_verifier_scope_without_audit`, `…refuses_bad_scopes_and_missing_or_malformed_writers`, `public_grants_refuse_the_system_principal`); `mise run test:principals:unit` (System token verifier/issuer contract tests) | PASS |
| Authority text agrees | `changes/active/verified-change-contract/spec.md` revision 36 (System principal definition, REQ-086 Eval-read paragraph, open-decisions line, revision history with the owner's quote, approved 2026-09-23); `architecture/wyrd-security-posture.md` principal and token-scope paragraphs; this task's `spec_revision: 36` | — | PASS |

Commands run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_reads_ordered_bounded_trace_evidence)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_read_authority_fails_closed)"'
mise run test:principals:integration
mise run test:principals:unit
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test pg_verification_runtime'
mise run test:bifrost:journey:server   # 21/21
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
git diff --check
```

`codegen:check` not run: no wire type, permission/scope enum, error catalog, schema, or stub changed (`EvalReadAuthority` and its error are server-internal Rust types). The missing-identity case is modelled by relabelling the stored row because the schema keeps a System principal permanently `active`. No new identity store, principal kind, user, public permission, token format, or general Bifrost grant was added; the stored row keeps its `verification-results-writer` name.

## Evidence — FIND-TASK-006-3

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Media body read is bounded to limit + 1 bytes through the existing StorageHandle and installed OpenDAL, independent of metadata | `crates/wyrd/wyrd-storage/src/handle.rs` `StorageHandle::get_object_bounded` (raw accessor read with no range, so no `stat` sizes it; streams chunks and stops at `limit + 1`; object key not recorded on its span) | `handle::tests::get_object_bounded_holds_at_most_one_byte_past_the_limit` (whole body within limit, `limit + 1` prefix past it, `ObjectNotFound` when absent); the same assertions added to the shared `handle_crud` scenario pass on local, S3 (RustFS), GCS, and Azure emulators | PASS |
| Overflow refused before base64 encoding or provider invocation; metadata only a fast rejection; tenant, MIME, and kind checks kept | `crates/wyrd/wyrd-server/src/verification/eval.rs` `TenantMedia::resolve`: `object_len` fast check, then `get_object_bounded(path, MEDIA_LIMIT_BYTES)` and a terminal refusal when the body exceeds the limit, before `encode` | `verification::eval::tests::media_refuses_a_body_past_the_limit_its_metadata_hides`: an OpenDAL test layer makes `stat` report 1 byte for a 20 MiB + 1 KiB body; the bounded read holds exactly `limit + 1` bytes and the resolver returns `JudgeError::Terminal`. RED before the change: the resolver returned the whole body as base64 content. `media_resolves_only_authorized_bounded_supported_objects` still green | PASS |
| No provider call after a media refusal | Resolution runs in `SkaldJudgeInvoker::bind_media` before the provider request (`vala-eval/src/orchestrator/judge.rs`) | `vala-eval` `named_media_reaches_provider_as_native_content`: a refusing resolver fails the call and the capturing provider still holds one request | PASS |

Commands run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::eval::tests::media_refuses_a_body_past_the_limit_its_metadata_hides) | test(=verification::eval::tests::media_resolves_only_authorized_bounded_supported_objects)'
mise exec -- cargo nextest run --locked -p wyrd-storage --lib -E 'test(=handle::tests::get_object_bounded_holds_at_most_one_byte_past_the_limit)'
mise exec -- cargo nextest run --locked -p wyrd-storage --lib
mise run test:storage:handle:emulators
mise exec -- cargo nextest run --locked -p vala-eval --test orchestrator_judge_skald -E 'test(=named_media_reaches_provider_as_native_content)'
```

Absent object and non-not-found storage failures are now distinguished: an
absent object stays terminal, any other storage failure (metadata or body) is
retryable, matching the `MediaResolver` contract.

## Evidence — FIND-TASK-006-9

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Public `VerificationError` messages carry only a stable code and fixed operation text; causes go to structured diagnostics | `eval.rs` `failed(run_id, code, step, cause)` logs `warn` with `run_id`, `code`, `step`, `cause` and returns `failure(code, step)`; used for the read authority, record and trace reads (`ReadError::outcome`), sampling, spec planning, scoring, and report capture; awaiting-trace uses fixed text. `runner.rs` `load_verifier` (registry SQL), result encoding, and publication failures use fixed text and log the cause | `eval_verification::continuous_eval_failures_publish_only_stable_errors` (journey): provider 400 with a sentinel body, an in-tenant media URI whose object key is a sentinel, and a restrictive RLS policy on `wyrd.auth_service_accounts` raising a sentinel during Eval read-authority resolution. Each gated run settles `errored` with no result; public `GET /v1/verification/runs/{id}` (SDK `Verification::get_run`) keeps `eval_execution_failed`/`eval_execution_failed`/`eval_record_unavailable`, and neither status nor any persisted `wyrd.verifier_runs.error` contains a sentinel. RED before the change: the provider run's status message held `…openai rejected request: {"error":{"message":"PROVIDER-SENTINEL-4f1c"…}}` | PASS |
| TenantMedia exposes only the binding ID and a category; URI, object key, and backend `Display` are redacted from diagnostics too | `TenantMedia::refused` / `TenantMedia::storage_failed`: reasons `media `<id>`: <category>`, logs `binding`, `category`, and for storage failures only the backend kind | `verification::eval::tests::media_failures_name_only_the_binding_and_a_category`: absent, cross-tenant, and unreadable (directory) objects named with a sentinel yield binding-plus-category reasons (the last retryable) and a captured log without the sentinel. Mutation proof: logging the storage error `Display` fails the test. Journey trace (`WYRD_LOG=info,wyrd_server::verification=debug`): 0 occurrences of the locator sentinel; the locator run logs `category="the object does not exist"` | PASS |
| Raw dependency causes remain in protected diagnostics | `failed` / `ReadError::outcome` | `verification::eval::tests::read_failures_keep_the_cause_in_the_log_only`: public message is the fixed step, the captured log carries run ID, code, and the SQL sentinel cause; journey trace shows the provider body and `SQL-SENTINEL-c93e` only in `continuous Eval step failed … cause=` events | PASS |
| No new error catalog or public detail field | Existing `eval_*`, `verifier_unavailable`, `result_*` codes and `VerificationError { code, message }` unchanged | `pg_verification_runtime` 20/20; `mise run test:bifrost:journey:server` 22/22 | PASS |

Commands run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_failures_publish_only_stable_errors)"'
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --no-capture -E "test(=eval_verification::continuous_eval_failures_publish_only_stable_errors)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::eval::tests::media_failures_name_only_the_binding_and_a_category) | test(=verification::eval::tests::read_failures_keep_the_cause_in_the_log_only)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::/)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test pg_verification_runtime'
mise run test:bifrost:journey:server
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-storage -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings
git diff --check
```

`codegen:check` and `check:client-tier` not run: no schema, error catalog,
wire type, or dependency changed. Raw provider and SQL causes stay in the
`warn` diagnostic by design (the approved correction keeps them in protected
diagnostics); only storage locators are redacted there. Bifrost query error
`Display` is logged as a cause unchanged.

## Evidence — FIND-TASK-006-6

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Real SDK-to-server journey where client `created_at` and managed event time fall on different UTC days | `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs` `continuous_eval_runs_the_terminal_matrix`: the four matrix records are emitted through the SDK (`observe.eval`, which mints `created_at` from the client clock) while Scribe's receipt clock is shifted one `DAY` ahead (`shift_receipt_clock_for_test`), reset after the SDK lifetime shuts down; so every row's `wyrd_event_time` is at least one day after its `created_at` | same test: the `eval-gated` pass run completes (`passed`, 2 items); its frozen `event_time` equals the row's `wyrd_event_time`; the row is read back by `record_id` bounded to the frozen managed UTC day, and its `created_at` day is asserted to differ from that day | PASS |
| A read by client creation day turns the journey red | — | Mutation: `BifrostReader::record` pruned on `created_at` instead of `wyrd_event_time` (temporary, reverted). The test fails: `eval-gated did not complete: … event_time: 2026-09-25T02:23:00.539693Z, state: RunRow { status: "errored", attempts: 3, result_id: None, error_code: Some("eval_record_unavailable") }`. GREEN after revert | PASS |

The SDK exposes no caller `created_at`, so the day split is made on the managed side through the existing test receipt-clock seam the trace-window journey already uses; no SDK or server code changed.

Commands run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_runs_the_terminal_matrix)"'   # GREEN 1/1; RED 1 failed under the created_at mutation
mise run test:bifrost:journey:server   # 22/22
mise exec -- cargo clippy --locked -p vala-eval -p wyrd-testing --all-features --all-targets -- -D warnings
mise run fmt
git diff --check
```

## Evidence — FIND-TASK-006-10

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `fan_out_bucket` documents its bucket role, first-error path, and cancellation/partial-progress consequence; execution shape unchanged | `crates/vala/vala-eval/src/executor.rs` `fan_out_bucket` rustdoc: per-kind role under `fan_out_all_buckets`, completion-order collection, `# Errors` (first executor `EvalExecError` unchanged, or `DagInvalid` for a panicked/cancelled task), `# Cancellation` (early return drops the `JoinSet` and aborts sibling tasks, collected outputs are discarded, a `try_join!` short-circuit aborts the bucket the same way, performed executor side effects are not rolled back). Doc-only diff | `executor::tests::executor_error_propagates_and_false_comparison_fails` PASS; clippy `-D warnings` clean | PASS |

## Evidence — engine regressions

All PASS (1 test run each, 102 skipped):

```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=executor::end_to_end::full_spec_against_in_memory_record_and_spans_aggregates_correctly)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=tasks::trace::trace_executor::trace_assertion_unavailable_errors)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=tasks::judge::llm_judge_executor::retry_budget_exhausted_errors)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=results::results_aggregation::context_capture_redact_drops_actual)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=results::results_aggregation::pass_gate_all_pass_on_all_skipped_subject_set_fails)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=executor::tests::executor_error_propagates_and_false_comparison_fails)'
```

## Diagnosis — `audit_publication::a_stalled_tenant_does_not_block_another_tenants_history` (journey:server lane)

**Symptom.** `mise run test:bifrost:journey:server` failed 21/22. After 92s the test failed with `the idle tenant kept 1 staged row(s) through 90s, lowest seq Some(2)`. Run alone, it passed in 21.7s.

**Evidence.**
- Lane trace (`/tmp/t006-bifrost.log`): the server's first sweep published both tenants, and Scribe settled their audit batches at 02:53:48.95. The stalled tenant's settlement `UPDATE vala.audit_chain_head SET published_seq = GREATEST(...)` then logged `slow statement … elapsed=90.36s`. It returned only when the failing test dropped its fence connection. The only `audit drain failed` warning names the stalled tenant, and it came at shutdown.
- Solo trace (`/tmp/t006-stalled-alone.log`): the fence landed before the freeze. The stalled tenant's cycle failed fast at `freeze_publication_range` (`FOR UPDATE NOWAIT`, logged as `audit staging read failed`) on every tick, and the healthy tenant drained.
- An independent read-only diagnostician reached the same cause separately. It recommended fixing the run loop, and rejected both a settlement NOWAIT/`lock_timeout` and a test change.

**Cause.** This is a product defect in `AuditPublisher::run`. The loop awaited each `sweep()`, and `sweep()` awaited every tenant cycle through `for_each_concurrent`. Freeze never waits (`NOWAIT`), but settlement waits on the tenant's chain head and staged rows. So a cycle that froze and appended before another transaction took that tenant's chain head blocked inside settlement, and no later sweep could start. The healthy tenant's first decision (seq 1) was published in the same sweep. Its seq 2 is the Oracle read decision staged by the test's own polling. Only a later sweep could publish it, and none started. That contradicts the publisher's documented rule that a tenant blocked on its chain head delays only itself. Production can hit the same stall: `append_audit` holds the chain head for the whole audited transaction, so one slow audited transaction overlapping a tenant's settlement would stop publication for every tenant on the replica. The branch did not touch publication. The heavier lane only moved the fence into the window after the freeze.

**Fix site.** `crates/wyrd/wyrd-server/src/audit/publication.rs`
- Each tenant cycle now runs as its own task in a new `TenantCycles` owner, held by the run loop: a `JoinSet` plus a task-id → tenant map, capped at `PUBLICATION_TENANT_CONCURRENCY`.
- A sweep reaps finished cycles, skips any tenant whose cycle is still running (that cycle already owns the frozen range), and waits only for a free slot, never for a particular cycle.
- On shutdown the set is dropped, which aborts in-flight cycles. That is safe because of the freeze/replay/dedup design.
- The SQL and the replay semantics are unchanged. `frozen_audit_range_replays_once_while_its_tail_waits` still relies on settlement waiting behind its staged-row fence, and it still passes.
- The only change to the test file is its rustdoc. It now describes the settle-blocked interleaving the test can hit and the publisher's new ceiling. No assertion, timeout, or wait changed.
- New unit test `audit::publication::tests::a_blocked_cycle_holds_only_its_own_tenant`: a cycle that never finishes does not stop a free slot from being granted, does not stop another tenant's cycle from finishing and being reaped, and stays reported as running.
- A deterministic journey reproduction is not feasible. Every lane server's publisher sweeps the shared tenant directory on its own tick phase, so no test can pin a cycle between its freeze and its settlement.

Commands run:

```bash
WYRD_LOG=info,vala_bifrost_redux=debug,wyrd_server=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=audit_publication::a_stalled_tenant_does_not_block_another_tenants_history)"'   # before fix: PASS alone (21.7s); after fix: PASS (21.7s)
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=audit::publication::tests::a_blocked_cycle_holds_only_its_own_tenant)'   # PASS
WYRD_LOG=info,vala_bifrost_redux=debug,wyrd_server=debug mise run test:bifrost:journey:server   # 22/22 PASS
mise run fmt
mise exec -- cargo clippy --locked -p wyrd-server -p wyrd-testing --all-features --all-targets -- -D warnings   # clean
git diff --check   # clean
```

## Final verification — TASK-006-R1 (HEAD 86c638a3)

`mise run test:bifrost` runs longer than one foreground command's 10-minute limit, so every lane in
`scripts/run-bifrost-tests.sh` (and every capability in `scripts/run-bifrost-journeys.sh`) was run in the
foreground under its own `scripts/postgres/with-test-postgres.sh` lifecycle, mirroring the parent task:

| Command | Result |
|---|---|
| five vala-eval engine regressions (exact `-E 'test(=…)'` commands above) | PASS, 1 test each |
| `mise run fmt` | PASS |
| `mise run lints` | PASS |
| `mise run codegen:check` | PASS |
| `mise run test:vala` | PASS (1298) |
| `mise run test:sql` | PASS (162/4/113/2) |
| `mise run test:wyrd` | PASS (2129) |
| bifrost `unit:rust`, `unit:python`, `unit:typescript`, `integration:sql`, `integration:server` `:inner` | PASS (221, 2, 6, 113, 81) |
| bifrost `integration:redux:inner` | PASS (988) |
| bifrost journeys `sdk`, `observe`, `forge`, `oracle`, `otlp`, `mcp` `:inner` | PASS (17, 1, 13, 28, 10, 10) |
| bifrost journey `server:inner` | PASS (22) |
| bifrost journey `scribe:inner` | PASS (21) |
| bifrost `journey:python:inner`, `journey:typescript:inner` | PASS (18 TS tests) |
| `mise run test:wyrdstate:journey` | PASS |
| `mise run check:unwrap-audit`, `mise run check:client-tier` | PASS |
| `git diff --check` | PASS |

Non-goals stayed excluded: no outbox, Bifrost queue poller, second Eval engine/judge, provider file
lifecycle, new public auth surface, broad System query grant, or configuration knob.
