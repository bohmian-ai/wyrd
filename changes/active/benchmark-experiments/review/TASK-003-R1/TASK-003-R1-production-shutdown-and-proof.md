---
id: TASK-003-R1
kind: remediation
status: ready
spec: SPEC-benchmark-experiments
spec_revision: 8
requirements: [REQ-017, REQ-018, REQ-019, INV-010, AC-011]
depends_on: [TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2, FIND-TASK-003-3]
---

# Complete production shutdown and deliver its regression proof

## Subject and approved authority

Route directly to `$wyrd-implement`; this is a bounded correction under approved behavior, not a new design exercise.

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Approved spec: `changes/active/benchmark-experiments/spec.md`, revision 8, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`. The maintainer explicitly approved this text in the review session; stale draft metadata does not revoke that instruction. Candidate Git contains revision 6; use the explicitly approved revision-8 text recorded by this review.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-003-production-tracing.md` at candidate.
- Original base: `7f422c53ad54e370c24cb971a343ec5e06651109`.
- Reviewed candidate / remediation base: `45641bd5eadf8c86111d82fda6afd9605e218f84`.
- Independent diagnoses and ledger: this directory's `findings-validation.md`, `task-review-behavior.md`, `system-review.md`, and `verdict.md`.

Preserve the original task and this review as acceptance context. Do not alter historical evidence to imply earlier approval or successful closure. Record the remediation candidate and exact new regression selectors in completion evidence. Commit or push only if separately asked.

## Diagnosis and selected correction

### FIND-TASK-003-3 — Ordinary production shutdown omits the SDK total and final drain

REQ-018/AC-011 require orderly SDK shutdown reporting and bounded lifecycle behavior. Production installs a provider tracer in the global subscriber (`wyrd-telemetry/src/lib.rs:431–468`) and returns a `TelemetryGuard`. The server's shared process owner (`wyrd-server/src/app/mod.rs:43–105`, `app/metrics.rs:234–272`) drains its roles and then drops guard/runtime ownership. The guard has no disposal implementation; its consuming `shutdown` method (`lib.rs:300–310`) is not called there. The global subscriber retains a tracer with a strong provider reference, so SDK final-reference cleanup does not run when application ownership is dropped. Consequently an ordinary diagnostic restart or rolling replacement after saturation never invokes the SDK path that logs its exact lifetime total, and queued operator diagnostics lack an intentional final drain. V3 passes because it explicitly shuts down a test-local provider, bypassing this deployed owner.

Make disposal of the existing shared `TelemetryGuard` invoke the retained stock provider's bounded shutdown even while tracer/subscriber references remain alive. Preserve the explicit consuming shutdown API, exactly-once invocation, no-provider no-op, and existing shutdown error logging. Retain process ordering after serving/role/outbox drain. Reuse the installed provider and processor timeout behavior. The existing guard already promises drop cleanup, so fulfilling that shared contract is smaller and more complete than adding cleanup to individual serving branches. Dropping another Arc or weakening the operations promise does not satisfy the obligation. No new public setting, runtime, timeout policy, queue, worker, retry or termination behavior is warranted.

### FIND-TASK-003-2 — The delivered proof selection skips the new quiet-policy journey

Task V5 explicitly says to add no new ignore. The new quiet-policy story has a new ignore at `sdks/wyrd-sdk-rust/tests/integration/action_timeline.rs:191`, and V5's recorded selector includes only the original two stories. Thus default shared-family execution and the recorded focused command can pass without exercising the central production-filter regression. The shared-family lane already provisions repository Postgres/migrations, and nextest process isolation supports this test's global subscriber. The expanded review command passed all three stories but does not repair delivered selection.

Remove only the new story's ignore and include it in the maintained V5 exact selector. Keep the existing `act_and_read` assertions, original stories, existing ignores and Postgres lifecycle. No new binary, fixture, opt-in lane or runtime option is needed.

### FIND-TASK-003-1 — Required changed-item rustdoc is incomplete

Root rules and `agent_docs/rust.md` make changed-item rustdoc a mandatory merge gate, including private fields, fixtures and tests. The candidate omits field documentation on `FailedBatches`, `Staged`, `Logs`, and `FailedBatchCount`; required error sections on `FailedBatches::export`, `Logs::write`/`flush`, and `Gated::export`; and panic sections on `serve` and the three new telemetry tests. Exact locations and contracts are in the validated ledger. Runtime success cannot waive this gate.

Complete truthful documentation for this changed item set and the runtime correction's new/changed items. Document actual error and panic conditions, including intentional fixture/test assertions, without changing runtime behavior or sweeping unrelated old code. Batch these local corrections with this remediation; no new documentation checker or wording tests are justified.

## Preserved behavior and boundaries

The telemetry guard remains the lifecycle owner; existing subscriber/provider, sampler, tenant route/catalog/scrubbing and Scribe outbox remain their current owners. Preserve independent log/operator filtering, 1% operator default and explicit ratios, W3C identity/linkage, correlated unsampled-error logs, nonblocking requests/tenant copies and independent tenant loss accounting. Preserve stock exporter result and lifecycle delegation. Operator shutdown failures remain logged rather than failing serving requests or terminating unrelated capabilities.

No second provider, custom processor, queue, worker, retry, tail sampler, live-drop metric, dynamic capture service, new public timeout, live restart harness, broad refactor or full language-suite repetition is in scope. Do not change tenant consent, authorization, durability or process drain ordering. No specification revision is required for these corrections.

## Ordered proof and acceptance

1. For FIND-TASK-003-3, add a focused regression at the existing telemetry owner that retains tracer/subscriber ownership, drives controlled queued/dropped spans, and releases the guard through its actual disposal action. Before correction it must fail because shutdown is not invoked. After correction it must observe final exporter shutdown/drain and the exact SDK lifetime total despite the retained tracer. Verify explicit guard shutdown does not invoke shutdown twice and no-provider disposal remains harmless. Reuse existing controlled-exporter/stock-processor behavior. Do not add a public test timeout API or server restart harness. Existing V3 plus installed SDK timeout source establishes the stall bound once guard invocation is proven.
2. For FIND-TASK-003-2, verify the quiet story is no longer ignored, update V5, and run the three-story selector below through the existing lifecycle. It must preserve connected-tree/IDs/links and tenant-negative assertions.
3. For FIND-TASK-003-1, source-audit the changed item set for required field/error/panic rustdoc, including the new regression. Runtime wording tests are unnecessary.

Record the newly added/changed regression's actual name and exact nextest selector, RED cause, GREEN and regression results. Never claim a newly planned test already exists or use a selector that chooses no tests. Run that exact selector plus existing V1/V3 focused checks and the three-story command below; then only the affected-owner format/lint checks. No full journey suite or broad aggregate is required here.

```bash
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::production_filters_preserve_tenant_traces)'
mise exec -- cargo nextest run --locked -p wyrd-telemetry --features test-support --lib -E 'test(=tests::operator_export_loss_is_observable)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=action_timeline::service_action_timeline_is_one_connected_tree) | test(=action_timeline::service_action_timeline_survives_a_quiet_operator_policy) | test(=action_timeline::another_tenant_cannot_read_the_timeline)'"
mise exec -- cargo fmt -p wyrd-telemetry -p wyrd-ai -- --check
mise exec -- cargo clippy --locked -p wyrd-telemetry --features test-support --all-targets -- -D warnings
mise exec -- cargo clippy --locked -p wyrd-ai --test integration -- -D warnings
mise exec -- git diff --check
```

Review next round against the prior candidate and these three findings, carrying forward unchanged acceptance evidence. Reopen an unchanged area only if the guard correction or new source evidence materially affects it.

## Evidence

Remediation candidate: `71d2c240a` (base `45641bd5e`). Commits: `0027b896c` guard shutdown, `998bf6e61` rustdoc, `7680a2890` journey ignore removed, `71d2c240a` V5 selector amended in the original task.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-3: guard disposal shuts down the retained provider once; explicit shutdown once; no-provider drop harmless | `impl Drop for TelemetryGuard` (`wyrd-telemetry/src/lib.rs`) takes the provider and calls stock `shutdown`; `shutdown(self)` is `drop(self)`; error still logged | New `tests::guard_drop_shuts_down_a_retained_provider`: RED with the drop body removed (`the queued span is drained: left Err(Empty), right Ok(1)`); GREEN after: queued span drained, exporter shut down once, `dropped_span_count=1` logged while the tracer stays alive; explicit shutdown shuts the exporter down once; no-global guard drops harmlessly | PASS |
| FIND-TASK-003-2: quiet story runs without a new ignore and is in V5 | Ignore removed from `service_action_timeline_survives_a_quiet_operator_policy`; V5 command in TASK-003 now selects all three stories | `cargo nextest list` without `--run-ignored` selects the quiet story; V5 three-story command: 3/3 passed | PASS |
| FIND-TASK-003-1: changed-item rustdoc complete | Field docs on `FailedBatches`, `Staged`, `Logs`, `FailedBatchCount`; `# Errors` on `FailedBatches::export`, `Logs::write`/`flush`, `Gated::export`; `# Panics` on `serve` and the four telemetry tests including the new one; `Drop::drop` documented | Source audit of the changed item set; fmt and clippy clean | PASS |

Commands (all passed): `tests::guard_drop_shuts_down_a_retained_provider`, `tests::production_filters_preserve_tenant_traces`, `tests::operator_export_loss_is_observable` exact selectors; full `wyrd-telemetry` lib (28/28); the V5 three-story journey command above; `cargo fmt -p wyrd-telemetry -p wyrd-ai -- --check`; `cargo clippy --locked -p wyrd-telemetry --features test-support --all-targets -- -D warnings`; `cargo clippy --locked -p wyrd-ai --test integration -- -D warnings`; `git diff --check`.

Non-goals held: no new provider, processor, queue, worker, retry, timeout setting or restart harness; process drain ordering in `wyrd-server/src/app/mod.rs` is unchanged, so the guard's final drop still follows serving, role and outbox drain. The SDK's 5 s default shutdown timeout and V3's stalled-shutdown proof bound the drop. Pre-existing uncommitted spec/TASK-001/TASK-002 edits and untracked review files were not committed.
