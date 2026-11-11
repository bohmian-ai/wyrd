---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 7
requirements: [REQ-003, REQ-005, REQ-007, REQ-008, REQ-011, REQ-018, REQ-020, REQ-021, REQ-022, REQ-024, REQ-028, REQ-029, REQ-030, REQ-031, REQ-032, REQ-033, AC-023, AC-024, AC-025, AC-026, INV-001, INV-002, INV-003, INV-004]
depends_on: [TASK-005]
---

# Local Gateway governance and lower Agent check latency

## Outcome and Value

Keep PostgreSQL off Gateway quota admission and answer delivery by extending
its existing per-server cache with usage and local reservations. Check/reserve
locally, dispatch, return the answer, and batch usage writes/shared-state
refresh in the background. Quotas, including budgets, are approximate across
servers. Unflushed usage may be lost on crash. Missing/unhealthy/full state
returns the existing retryable 503 until background recovery succeeds.

Also remove repeated JSON Schema preparation from Agent checks and correct
supported remaining latency costs. Keep schema validation, repair, custom-data
bounds and citations. The existing verification benchmark has no quotas, so
its latency improvement must come from the Agent path, not SQL removal.

Revision 7 records the maintainer's 2026-11-11 approval and failure-policy
choices. TDD is explicitly waived for this task; focused correctness and
performance proof remain required. Keep the implementation small: existing
cache, workers, task lifecycle and SQL owners; no Redis, durable local ledger,
replica leases, strict global quotas or separate coordination service.

## What Changes and What Must Stay True

- Extend the existing Gateway process owner, tenant snapshot/listener lifecycle
  and tracked completion work. Cache changing usage/reservations in addition
  to policy. Do not add another Gateway, provider transport, scheduler or store.
- Admission is atomic within a server and isolated by tenant/policy. Account
  for request/token windows, concurrency, spending periods, fallback/retry
  exposure and unknown usage/cost according to revision 7. A
  background SQL refusal cannot undo an already-dispatched provider call.
- Background persistence combines servers' contributions rather than replacing
  the total with one server's view. Refresh neither erases local pending
  reservations nor double-counts acknowledged contributions. Bound retained
  tenants, pending work and refresh concurrency; stop admitting quota-covered work on missing/unhealthy/full state.
- Buffered answers do not await ledger IO. Streams settle after completion;
  cancellation, provider error and shutdown retain/release ownership exactly
  once. Local concurrency ends when execution ends, not merely at headers.
- Keep authentication, authorization, tenant isolation, SSRF/secret handling,
  capture-slot reservation before dispatch, attribution, evidence and durable
  acknowledgment rules unchanged except for explicitly approved governance
  consistency/persistence changes. The capture outbox is not automatically a
  durable quota ledger. Preserve existing response/error dialects unless the
  spec explicitly changes a refusal contract.
- Reuse preparation for the exact effective output schema and validation
  settings, including rendering, reachable-slot narrowing and schema-changing
  callbacks. Keep declared Verdict validation where it differs from the sent
  schema, code-check validation, safe repair feedback and custom-data limits.
  Reuse immutable preparation only; never share media, inputs, tools, caller
  authority, mutable conversations or workspaces between executions.
- Keep compact decisions/descriptive details from TASK-005, real provider
  request/response checks, retry/fallback/deadline behavior and ordinary-mode
  telemetry. Do not remove the tracked Gateway handoff merely to lower a timer.

Costs are approximate global enforcement and possible loss of unflushed usage
after crash or an expired shutdown deadline. No new performance SLO, SDK API, authentication cache,
host-load generator, dependency or Cargo feature is selected.

## Approach

1. Surface the Gateway phases/waits already recorded during Verification through
   the canonical report; recover the saved reference before another experiment.
2. Reuse immutable preparation for the exact effective schema at an existing
   execution owner whose lifetime spans requests. Keep validation in every caller.
3. Extend the existing Gateway cache with local reservations/usage and use its
   background lifecycle to batch persistence and refresh shared totals.
4. Return buffered answers before SQL accounting. Preserve cancellation/stream
   ownership, bounded pending work, replay safety and approved 503 recovery.
5. Localize the remaining Agent preparation/validation/citation costs, implement
   supported owner-local reductions, and verify independently attributable gains.

## Verification Scenarios

No failing-first test or RED/GREEN/REFACTOR cycle is required. Implement the
fixes directly, then collect these focused regression/integration observations.
Private fixtures and mechanics remain implementation-owned.

| Scenario | Required observation and focused proof |
|---|---|
| Saved diagnostics | The [diagnostic regression](#focused-commands) preserves Gateway phase/wait labels, actual counts, driver/server separation and canonical saved-report regeneration. No-model steps mark these inapplicable. Do not rerun the retained baseline. |
| Exact preparation reuse | The [preparation and Verdict tests](#focused-commands) prove warm unchanged schemas avoid repeated preparation, while changed/bound/hook-replaced schemas, invalid output, custom-data bounds, repairs and citations retain behavior. Code checks still validate. Bounded eviction can prepare again; caller evidence and authority are never shared. |
| Local admission | The [real-Postgres admission proof](#focused-commands) pauses background SQL after warming state: observe dispatch and the buffered answer before releasing SQL. Concurrent calls cannot spend the same local headroom, tenants remain isolated, and multi-server enforcement is explicitly approximate. |
| Background accounting | The [reconciliation proof](#focused-commands) races refresh, acknowledged writes, uncertain commits and completion. Pending usage is neither lost nor counted twice within a live process; servers' contributions accumulate, and token deltas/releases are replay-safe. Stream/cancellation completion releases local concurrency exactly once. |
| Failure and recovery | The [lifecycle proof](#focused-commands) checks cold/unhealthy/full state returns the existing 503 before dispatch, with recovery in background and no synchronous SQL fallback. Restart uses committed totals; loss of unflushed usage is accepted. Use existing migration/drain conventions, not a new rollout protocol. |
| User behavior and latency | The [Gateway journey and SDK regressions](#focused-commands) preserve returned judgments, evidence, errors and background ownership. Implement schema preparation removal and other supported Agent reductions; publish authorized canonical comparisons with CPU/allocation/waits distinguished. SQL removal receives no credit for the no-quota Verification fixture. |

## Acceptance Criteria

| Spec obligations | Owner and proof |
|---|---|
| REQ-003/005/007/011/018/022/033; AC-025; INV-004 | Capacity regression and saved-window recovery preserve counts/units/roles and report regeneration. Supported Agent fixes have attributable evidence; async parent CPU shares are not wall-time attribution. |
| REQ-020/021/024/032; AC-025 | Skald effective-schema and Vala Verdict proofs plus focused SDK regressions establish preparation reuse without removing model/code validation, custom-data bounds, repair or citations. |
| REQ-028–030; AC-023 | Real-Postgres Gateway tests establish atomic local reservations and dispatch/answer while SQL is paused, with approximate cross-server enforcement and unchanged applicable-limit refusal codes. |
| REQ-030–031; AC-024 | Retry/refresh/completion tests preserve pending and committed ownership; cold/unhealthy/full state refuses with retryable 503 and recovers asynchronously. Crash loss of unflushed usage is accepted. |
| REQ-028–033; AC-026; INV-001–003 | Existing owners provide bounded background work and accepted-work drain, tenant/security/capture isolation and completion ownership. No new coordination service or crash-safe ledger; no TDD requirement. |

## Implementation Reference

- **Gateway owner/cache:** `components/gateway/owner.rs::Gateway`,
  `snapshots.rs::GatewaySnapshots`, `service.rs::GatewayAdministration::snapshot`.
  Already own bounded tenant policy snapshots, notifications and task drain;
  missing mutable usage/reservations and background refill. Extend their
  lifetimes rather than create another service. Bifrost `catalog/cut_cache.rs`
  provides lifecycle/generation-fence precedent, not a reusable quota policy.
- **Admission/accounting:** `ledger.rs::GatewayLedger::{admit,settle}`,
  `invocation.rs::GatewayInvocation::{admit_call,govern}` and
  `CallTask::{settle_buffered,settle_stream}`. Native `caller.rs` and public
  ingress converge here. Existing `pg_invocation_tests` prove holds, simultaneous
  replica admission, token exposure, cancellation and capture saturation. The
  missing behavior is local charging plus async usage updates, not
  another provider client or per-route workaround.
- **Durable owner:** `wyrd-sql/src/queries/gateway_admission.rs::GatewayAdmissions`
  and the immutable admission/settlement migration. Existing strict SQL cannot
  be queued unchanged as approximate admission. Token-delta replay needs
  inspection before reuse by a retrying writer; any new migration follows the
  approved persisted contract and the SQL directory rules.
- **Hosted schema lifetime:** `HostedBlocks::engine` constructs an executor per
  execution; `AgentExecutor::{resolve,bind,invoke,execute}` resolves/binds/runs
  the exact Prompt. Reusing only the per-execution executor misses the repeat-
  request cost. Retain narrowly scoped immutable preparation at an existing
  owner whose lifetime spans calls; retain no authority or evidence with it.
- **Schema validation owners:** `NativePrompt::edit_response_schema_mut` /
  `check_output_schema`, `loop_runtime`'s post-hook schema selection,
  `StructuredOutput::{new,check}`, and `verdict_from`. The latter also serves
  `CodeExecutor::execute`; preserve that consumer. Existing Skald structured-
  output integration tests cover bound/replaced schemas and repair; Vala and
  SDK tests cover custom Verdict data and citations.
- **Measurement:** `verification/run.rs::{BREAKDOWN,component_totals}` omits
  already-recorded `gateway/telemetry.rs` phase/wait families. Extend the current
  record/report machinery, not a parallel recorder or custom result format.
  `gateway/caller.rs::NativeCall::project` and typed answer decode are subsequent
  copy/conversion candidates. Provider transport is pooled and Gateway is in-process.
- **Contracts/consumers:** update architecture, policy descriptions, error
  documentation and strict-enforcement tests for the approved governance change.
  No SDK wire change is presently selected. If one becomes necessary, stop for
  specification authority and include generator/stub/all-surface closure.

Full source/artifact findings, all three saved repetitions, and limitations:
[research](../research/agent-check-and-gateway-governance.md). Governance design
context: [notes](../../gateway-governance-latency/design/notes.md).

## Verification and Evidence

### Focused commands

Commands below are implementation-time recipes, not checks executed during
planning. Every new selector is **planned**: confirm its actual module/target
and successful test discovery after adding it, before relying on its results.
Run Cargo lanes sequentially. Reuse `mise.toml` task setup and the repository
Postgres wrapper; do not run tests against an unrelated database.

Planning validation: an independent read-only comprehension pass over the current specification
and task proposal found no unexplained contradiction or material comprehension
gap. The approved scope and failure choices are recorded in revision 7. Relative
links, scenario coverage, existing test names/package targets and task
setup were checked; no implementation tests or benchmarks ran.

**Diagnostic regression (planned):**

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::run::tests::diagnostics_include_gateway_phases_and_waits)'
```

**Prepared-schema reuse (planned), and existing effective-schema/repair proof:**

```bash
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --lib -E 'test(=structured::tests::prepared_schema_reuse_tracks_effective_schema)'
mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(=structured_output::structured_output_validates_the_bound_schema) | test(=structured_output::structured_output_validates_a_replaced_schema) | test(=structured_output::structured_output_repairs_every_encoding)'
```

**Verdict validation (planned), plus existing custom-data schema regression:**

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::tests::prepared_verdict_keeps_validation_and_data_bounds) | test(=blocks::tests::verdict_data_keeps_its_schema_resource)'
```

**Real-Postgres local-admission, reconciliation and lifecycle proofs (planned):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib -E "test(=components::gateway::pg_invocation_tests::warm_governance_uses_local_reservations_without_database_waits) | test(=components::gateway::pg_invocation_tests::background_usage_reconciles_retries_and_remote_updates) | test(=components::gateway::pg_invocation_tests::usage_cache_bounds_staleness_restarts_and_policy_changes)"'
```

**Governed Gateway journey (planned):**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=resilience::gateway_cached_governance_answers_before_background_accounting)"'
```

**Focused existing SDK Agent journeys:** preserve custom data, invalid Verdict
handling, schema repair and citation behavior through the public SDK/server.
Use the owning drift-journey lane's `build:server`, `py:setup`, `ts:build` and
`ts:build:testing` prerequisites when extending its grouped language coverage.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verifier_building_blocks::invalid_judge_verdicts_are_inconclusive) | test(=verifier_building_blocks::judge_repairs_an_invalid_verdict) | test(=verifier_building_blocks::custom_verdict_data_survives_readback)"'
```

After focused proofs, run only affected package/module slices for Gateway,
Vala blocks, Skald structured output and prompt schema editing; select SQL
tests if the durable owner changes. Derive any additional exact selectors from
the final write set and record them before execution. Integrated change review
owns full journey suites and the broad gate.

**Required format/boundary checks:**

```bash
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise exec -- git diff --check
```

### Bounded measurement protocol — approval required before running

First recover the existing unprofiled and CPU/heap references listed in the
research. Persist Gateway diagnostics through the canonical report; do not
replace reports with handwritten benchmark tables. Inspection/regeneration
does not require a benchmark run.

Then present the smallest current-candidate experiment for authorization:
the Verification `direct-verify` experiment's actual declared steps
`direct-verify-c1`, `direct-verify-c64`, `direct-verify-model-c1`, and
`direct-verify-model-c64`, retaining all standard repetitions. Use unchanged
fixture/validation/recorder settings; distinguish schema-only and governance
milestones. Compare against saved references only when build/workload/method
identity is compatible; otherwise disclose the gap and request a bounded new
current reference, never rebuild historical baselines.

The no-quota fixture proves schema/Agent latency, not quota-cache latency.
Use deterministic governed journeys first; if a performance comparison with
governance enabled is needed, declare that workload explicitly and obtain
authorization rather than quietly altering a recorded benchmark step.
CPU-only and heap-only captures stay separate from unprofiled judgments.
Retain source/dirty diff, binaries/symbols, process roles, populations, response
bytes, client completion, Agent phases, provider/header/body waits, server CPU,
allocation, refusals and background drain/capture/ledger evidence. Never infer
off-CPU waiting from one async parent's CPU sample share. Missing readings
are missing, not zero. No hard latency improvement is promised by this plan.

## Material Stop Conditions

- Keep the approximate memory-owned design. Do not add strict global quota
  coordination, crash-safe local persistence, Redis or a separate service.
- Stop for changed public/wire contracts, new dependencies/features, authority
  sharing, or durable schema behavior outside revision 7. Existing SQL migrations
  may change only to support the approved async-usage contract.
- Do not remove validation/permission/audit/capture work or weaken tests to claim
  a speedup. Strict-admission tests must map to revision 7's replacement promise.
- Benchmarks remain separately authorized and cannot overlap builds/gates. Keep
  actual recorded step names and independently attributable comparison methods.

## Authority Links

- [Approved latency spec revision 7](../spec.md).
- [TASK-005](TASK-005-full-path-benchmark-parity-and-optimization.md) — real dependency;
  do not rewrite its ongoing work or evidence.
- [Historical Gateway-capacity constraint](../../gateway-capacity/spec-revision-4.md) —
  revision 7 supersedes before-answer quota settlement for this task.
- [Workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md),
  [testing](../../../../TESTING.md), [benchmark rules](../../../../benchmarks/README.md).
- [Wyrd protocol](../../../../architecture/wyrd-design.md),
  [security](../../../../architecture/wyrd-security-posture.md),
  [Bifrost cache/admission precedent](../../../../architecture/bifrost-design.md),
  [SQL directory rules](../../../../crates/wyrd/wyrd-sql/AGENTS.md).
