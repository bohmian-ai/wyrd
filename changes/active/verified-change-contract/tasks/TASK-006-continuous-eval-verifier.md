---
id: TASK-006
kind: implementation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-077, REQ-083, REQ-084, REQ-085, REQ-111, REQ-130, REQ-131, REQ-152, INV-004, INV-010, INV-012, INV-015, AC-014, AC-016, AC-020, AC-027, AC-033]
depends_on: [TASK-004, TASK-010]
---

## Outcome and Value

An acknowledged Eval observation best-effort creates durable work for each
matching active binding, then the existing Vala Eval engine executes sampling,
trace wait, deterministic assertions, LLM judges, media, workflow, gate, and
capture semantics. Completed work persists canonical summary/items; execution
errors remain errors rather than false assertions. No second online Eval engine
or offline dataset path is introduced.

## Owners, Scope, Consumers, and Prohibited Changes

`vala-eval` owns `ScenarioScoring`, `EvalExecutor`, judge/task executors,
reports, summary, pass gate, and context capture. Skald owns provider-native
multimodal input. Scribe's post-ACK server seam triggers best-effort `wyrd-sql`
run insertion; the generic runtime owns claims/status/results/dispatch. Oracle
reads the exact record/day and authorized trace/media sources.

The post-ACK insert reuses TASK-010's PostgreSQL-owned immediate queue time;
the committed observation's `wyrd_event_time` remains a producer-owned event
fact rather than a coordination clock.

Do not join enqueue to Scribe's batch-fence transaction, add an outbox, poll
Bifrost as a queue, synthesize `passed:false` from executor errors, add an
online-only judge, expose offline dataset execution, or send private URIs as
prompt/provider text.

## Approach

1. Correct the existing Eval engine so only returned `AssertionResult` values
   attest and errors propagate unchanged.
2. Add named media preservation/resolution through the existing judge path and
   apply context capture before persistence/public output.
3. Add the asynchronous idempotent post-ACK enqueue using exact record ID and
   server-managed event time.
4. Implement Eval input loading, sampling, trace lifecycle, engine invocation,
   common-verdict mapping, and canonical item/summary projection as one adapter
   to TASK-004.
5. Prove fail-open enqueue, terminal matrix, restart, media, and real SDK flows.

## Ordered Implementation Scenarios

### Scenario 1 — Assertion outcomes attest; execution errors propagate

**Behavior.** Successful true/false deterministic and judge results remain
normal attesting outcomes evaluated only by an authored gate. Missing context,
malformed judge output, provider/input/media failures return errors and never
become synthetic failed assertions.

**RED.** Add focused engine cases around the current executor fanout; today it
synthesizes `passed:false` on executor error.

**GREEN.** Remove that conversion and preserve typed error propagation through
the existing executor path.

**REFACTOR.** Keep one execution path for continuous and future offline record
sources.

### Scenario 2 — Gate, skipped tasks, and capture map exactly

**Behavior.** At least one attesting task plus gate maps pass/fail; no gate or
zero attestation maps inconclusive. Every Ran/Skipped outcome is retained for
items. Capture full/hash/redact changes stored/public evidence only, never
execution or verdict, and redacted messages do not leak raw values.

**RED.** Add report-to-common-verdict and persistence projection cases for
gated pass/fail, ungated, all-skipped, mixed skipped/ran, and each capture mode.

**GREEN.** Retain the existing `EvalReport` through the adapter and perform the
approved common mapping outside the engine's gate aggregate.

**REFACTOR.** Share one capture application point before any persistence or
public projection.

### Scenario 3 — Named media reaches native provider content safely

**Behavior.** Media binding IDs match declared prompt variables; the server
resolves only tenant-authorized supported object URIs, bounds bytes, validates
kind/MIME, and passes native content through both deterministic and production
judge invokers. Missing/cross-tenant/unsupported/oversized media is an
execution error with no result/dispatch.

**RED.** Add local object-store and provider-capture cases proving bytes/native
media rather than URI/JSON text plus all refusal modes.

**GREEN.** Preserve media through `ScenarioScoring`/`JudgeTaskExecutor` and
reuse Skald's native media binding at the production invoker.

**REFACTOR.** Keep provider encoding in Skald and avoid upload/file-ID caches.

### Scenario 4 — Post-ACK enqueue is idempotent and fail-open

**Behavior.** After Scribe ACK, a tracked asynchronous step inserts once per
matching active exact binding under `(tenant,binding,record)`, freezing
`record_id` and exact server `wyrd_event_time`. Duplicate ACK/replay converges.
Insert failure/crash leaves the observation acknowledged, emits structured
error, and creates no run/dispatch; inactive/wrong-tenant bindings create none.

**RED.** Add Scribe/server/Postgres integration cases for success, duplicates,
forced failure, process interruption, activity, and tenant routing.

**GREEN.** Attach the minimal post-ACK callback/task outside the batch-fence
transaction and call the existing run insert operation.

**REFACTOR.** Retain no process-local retry queue or outbox.

### Scenario 5 — Sampling and trace lifecycle precede execution

**Behavior.** The runner reads the exact record from its frozen UTC-day
partition, samples before trace/tasks, writes sampled-out completed/inconclusive
with zero-count summary and no items/dispatch, requeues `AwaitingTrace` until a
fixed deadline, then times out without results. Trace-source failures retry and
end errored when exhausted.

**RED.** Add partition-pruning cases where client `created_at` differs from
managed event day, plus sampled-out, trace-arrives, trace-deadline, source-error,
restart, and stale-lease cases.

**GREEN.** Use frozen record/time selectors and TASK-004 lifecycle transitions.

**REFACTOR.** Keep `AwaitingTrace` internal to Eval while the generic run is
queued/running through normal durable states.

### Scenario 6 — Completed Eval persists canonical items and summary

**Behavior.** Sampled-in execution writes every outcome item before the common
`EvalWorkflowSummary`, requires all non-empty ACKs, and dispatches only a
completed failed binding gate. Passed, ungated, sampled-out, all-skipped,
timed-out, and errored executions create no dispatch; the latter two create no
result rows.

**RED.** Add deterministic and local LLM-judge real-server journeys covering
the complete terminal matrix, partial ACK, restart, authorization, and tenant
isolation.

**GREEN.** Implement one Eval adapter using TASK-004 result and settlement
services.

**REFACTOR.** Remove any legacy Eval result table or kind-specific dispatch.

## Acceptance Criteria

- The complete `architecture/verifier/eval.md` terminal matrix passes.
- Existing Eval engine and judge path are the only execution implementation.
- Post-ACK enqueue remains explicitly best-effort and cannot delay/rollback
  ingest.
- `AC-014`, `AC-016`, and `AC-027` pass through real server/provider seams.

## Expected Write Set and Consumer Closure

Likely owners: `vala-eval` executor/results/judge/media code and tests, Skald
media input boundary, Scribe/server post-ACK composition, `wyrd-sql` run insert,
server Eval adapter/Oracle reads/object storage, canonical table projection,
and real SDK/server/provider journeys.

## Verification and Evidence

Confirmed existing engine regressions:

```bash
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=executor::end_to_end::full_spec_against_in_memory_record_and_spans_aggregates_correctly)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=tasks::trace::trace_executor::trace_assertion_unavailable_errors)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=tasks::judge::llm_judge_executor::retry_budget_exhausted_errors)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=results::results_aggregation::context_capture_redact_drops_actual)'
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(=results::results_aggregation::pass_gate_all_pass_on_all_skipped_subject_set_fails)'
mise run test:vala
mise run test:sql
mise run test:wyrd
mise run test:bifrost
mise run test:wyrdstate:journey
mise run fmt
mise run lints
git diff --check
```

The final common-verdict tests must not weaken the last engine regression; they
map its zero-attestation result at the consumer boundary.

## Material Stop Conditions

Stop for atomic observation-to-run delivery, a new Eval engine/status/verdict,
offline dataset execution, provider file lifecycle, changed terminal mapping,
or any path that treats an execution error as subject failure.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verifier/eval.md`
- `changes/active/verified-change-contract/architecture/logic/table_schema.md`
- `architecture/references/domain/evaluation.md`
- `AGENTS.md`
