---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-github-verifier-pipelines
spec_revision: 3
parent_task: TASK-001
remediates: [FIND-001-2, FIND-001-6, FIND-001-7]
depends_on: []
---

# Finish the deadline and startup configuration boundaries

Route to `$wyrd-implement`. Approved spec: `changes/active/github-verifier-pipelines/spec.md` revision 3, REQ-015/016, AC-010, INV-001/005/007. Original task: `changes/active/github-verifier-pipelines/tasks/001-isolate-queued-capacity.md`. Original base `395274d58bf76a2075f245ca3a5c1bd044925027`; first candidate `34fb45bb387582d2aedfee77fb4e09fa2753b80d`; this reviewed candidate `ea4460c3c97adb55395acd50f062658bc8d0d4cd`. Retain R1 (`review/001-capacity-r1/001-R1-close-capacity-review.md`, relative to the active packet) including both human-approved revisions, both verdicts and validated ledgers as next-round context.

FIND-001-1/-3/-4/-5 are closed. All nine exact closure tests passed on this candidate. Correct the remaining untested paths in the three findings below. The human revisions are approved scope, not new reviewer findings or authority to add other settings.

## FIND-001-2 — Carry the original call deadline through gateway admission

`components/gateway/caller.rs:133–186` now bounds the lane wait and subtracts elapsed time before handing a relative remainder to the gateway. But `components/gateway/invocation.rs:565–568,601` awaits snapshot/authorization/routing before calculating `now + remainder`. A normal ten-minute Agent call with a long enclosing execution ceiling can wait 599 seconds for its lane, then two seconds for a cold snapshot's tenant checkout (within its existing five-second bound). It can dispatch at about 601 seconds and receive another second of provider time. The existing gateway test has fast admission; it proves the wait subtraction and provider expiry, but cannot falsify late dispatch on this path. This is incomplete closure of the original call bound, not a gateway-wide timeout change.

Keep one absolute monotonic deadline at the lane-bound caller from before the permit wait and carry it privately into the existing GatewayInvocation admission and execution path. Use that instant for lane acquisition, reversible snapshot/authorization/routing preparation, capture waiting and the admitted call's existing engine deadline. Reuse the existing lane and governed timeout outcomes; preserve completed permission decisions/audits and capture staging refusal. Refuse expiry before starting mutating ledger admission, including expiry after a capture slot becomes available. Keep unbound callers and their relative gateway timeout contract unchanged.

Preserve durable admission and settlement at their current owners. If a ledger write is already in progress when the deadline passes, finish its existing handoff to the tracked CallTask. The engine's existing expired-deadline check then prevents provider dispatch and lets normal timeout accounting release the actual holds/leases. Preserve settlement before the buffered answer so a follow-on call does not meet its own outstanding reservation. Retain genuine caller-drop cancellation and provider `timed_out` capture. Do not wrap the whole invocation in a blanket timeout: dropping a pending ledger write can lose its settlement handoff, and dropping the guard during provider execution can turn timeout into cancellation. Do not introduce a hard reply-time SLA during a pending durable write, move admission/settlement to another task owner, change engine cancellation priority, or loosen the exact timeout/capture assertions. The bounded preparation phase and existing admitted deadline close the witnessed path without those changes. See `followup-review.md` and the independently validated trace in `findings-validation.md`.

Acceptance: in the existing real caller fixture, spend part of a short original budget waiting for the lane, hold actual cold-snapshot admission beyond the original deadline, and assert the governed deadline error before releasing that dependency, zero provider dispatch, released permit and a successful call after release. Preserve the existing held-lane expiry/cancellation, hung-provider exact deadline/capture, accounting and subsequent-call assertions. Keep the unbound adapter proof and the existing admission/settlement regression green. Do not add a synthetic load test or replacement fixture harness.

## FIND-001-6 — Resolve and validate the effective verification settings once

All three diagnosed cases belong to the existing `VerificationConfig` resolution and validation boundary; H3/H4/H8 require normal source precedence and refusal of unrepresentable effective values before startup composition.

1. **Precedence.** `config.rs:1854–1901,2079–2083` validates file positivity and converts pool units before applying environment overrides. For example, file `queued_executions = 0` refuses even with `WYRD_VERIFICATION_QUEUED_EXECUTIONS=9`; a parseable file pool MiB value of `9223372036854775807` also refuses before a valid pool override. This prevents operators from repairing a stale shared file through the approved environment layer. The current test proves precedence only when both inputs are already valid.
2. **Admission counts.** `config.rs:1752–1801,2018–2065` accepts positive file counts fitting `usize` without checking the runtime limit. File `direct_executions = 9223372036854775807` reaches `HostedBlocks::start` and panics at `hosted/blocks.rs:150` because it exceeds Tokio's `Semaphore::MAX_PERMITS`. The same boundary feeds both execution semaphores, both model/workspace semaphores and each Python workers-plus-waiters admission aggregate (`hosted/sandbox.rs:99–102`). The checked worker-memory sum alone cannot bound a huge waiter count. The explicit obligation is to refuse an unrepresentable setting, not to serve an enormous workload.
3. **Queued deadline.** `config.rs:1758–1761,1885–1889,1935–1938` accepts file `execution_timeout_ms = 9223372036854775807`. Boot projects it to `runner.rs:1189–1191`, where this duration fits Chrono's signed millisecond range. `verifier_executions.rs:444–445,2940–2957` binds it for the first claim's `statement_timestamp() + ($3::bigint * INTERVAL '1 millisecond')`. The exact expression failed in managed Postgres with `22008 interval out of range`, and the first-claim transaction would roll back (`claims.rs:345–355,407–425`). A `18446744073709551615` millisecond env value instead fails Chrono conversion and silently falls back to five minutes. Both are invalid effective settings; grouped/drift/fitter consumers also read this same configured duration. See `followup-config-timeout.md` for the independent source trace and limits of the SQL reproduction.

Use the existing startup override convention to select the winning raw value before positivity and unit conversion, then validate the resolved value at the same configuration owner. Preserve defaults → parsed file → existing env overrides, named ConfigError refusals, memory/prewarm/cross-field predicates, and the single limits/runtime projection. Malformed TOML remains a parse error; semantic validity of an overridden parseable value must not block a valid winner. Validate the newly configurable admission counts using the existing semaphore maximum and checked Python aggregate, before any semaphore is constructed. Keep the shared checked `HostedLimits::lanes_fit` predicate for worker memory. Validate the selected queued duration against the existing checked signed millisecond, interval microsecond and returned Chrono `DateTime<Utc>` timestamp representation. Reuse existing checked conversion/arithmetic; derive the check from representation rather than introducing a product timeout ceiling. PostgreSQL statement time still writes each actual deadline. Do not introduce a second parser/loader, new public setting, count type, per-consumer guards or broader semaphore/timer sweep. Deleting file checks alone would accept invalid effective values; checking downstream cannot restore precedence after early refusal.

Acceptance: extend the existing focused config proof to show valid env overrides win over a zero file count, overflowing file pool and out-of-range parseable file deadline. Without overrides the active values are refused by name, as are invalid env winners. Confirm that selected execution/model/workspace counts over the actual semaphore bound and overflowing or excessive Python worker/waiter aggregates refuse without creating runtime resources. The two queued-deadline reproductions must refuse at startup by selected input name, with coverage at the representation boundary used by the implementation and retained ordinary/two-hour cases. Keep the approved defaults and prior checked worker-overflow, memory and prewarm refusal assertions.

## FIND-001-7 — Document the changed test dispatch panic contract

`components/gateway/pg_invocation_tests.rs:115–137`, `Scripted`'s `ProviderDispatch::dispatch`, now records deadlines via a mutex lock with `expect` and also expects the seen/script locks. Its rustdoc does not explain the poisoned-lock panic condition. Root AGENTS and `agent_docs/rust.md` make that documentation mandatory for changed private/test helpers. Add the meaningful `# Panics` contract to this method and retain its behavior/assertions. Bundle with the substantive corrections above; no new test or neighboring rustdoc cleanup.

## Constraints and exact closure proof

Keep both approved default revisions, typed file configuration/env names, isolated direct/queued lanes, engine, claims and original persisted deadlines, tenant/subject authority, provider routing/settlement, and relocated real-client saturation proof. No new public API, dependency, service, durable format, platform behavior, performance claim, benchmark or broad cleanup. Do not reopen the four closed findings absent a fix impact. Update the implementation evidence with actual assertions, commands/results and any diagnoses; do not rewrite historical reports.

Run the existing exact configuration/default selections, including checked worker overflow and both human revisions:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tests::lanes_split_the_memory_ceilings) | test(=config::tests::verification_limits_env_applies_and_refuses) | test(=config::tests::verification_capacity_file_values_resolve_under_env) | test(=verification::tests::production_defaults_are_the_specified_ceilings)'
```

Extend the existing gateway caller proof or add only the focused delayed-snapshot case at the same test owner. Record its actual exact selector before RED if it needs a new test. Run the affected exact caller and admission/settlement regressions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::lane_wait_spends_the_agent_call_budget) | test(=components::gateway::pg_invocation_tests::gateway_agent_calls_start_only_on_a_compatible_adapter) | test(=components::gateway::pg_invocation_tests::gateway_admission_holds_limits_and_budgets_across_replicas)'"
```

Use the narrow applicable format/lint/boundary checks and `git diff --check` for the write set. If operator wording changes, keep its generated projections in sync and run the existing docs lane. Do not rerun full journey suites, `gate`, a platform matrix or benchmarks for this bounded task. The already green recovery and relocated proof carry forward unless the correction affects their behavior or evidence.

## Obligations

| ID | Source line | Observation | Planned assertion |
|---|---|---|---|
| G1 | 22, 26 | A lane-bound call that spends part of a short budget waiting for its lane, then meets a cold snapshot held past the original deadline, returns `WYRD_GATEWAY_504_DEADLINE_EXCEEDED` while the snapshot dependency is still held, near the original deadline | `lane_wait_spends_the_agent_call_budget`: lock waiter observed, then the call's error code and end time asserted before the barrier commits |
| G2 | 26 | That call dispatches nothing | same test: `dispatch.seen()` empty |
| G3 | 26 | That call keeps no lane permit | same test: `available_permits() == 1` after the refusal |
| G4 | 26 | After the dependency is released, a call succeeds | same test: after the barrier commits, the hung-provider call is dispatched and the following call is answered |
| G5 | 22 | The lane wait and the admitted call's engine deadline use one absolute instant fixed before the wait | same test: the hung-provider attempt's deadline is at most the original deadline plus margin, and the call ends by it |
| G6 | 22, 24 | Expiry before the mutating ledger write is refused, including expiry after a capture slot arrives; once the ledger write starts, it finishes its existing handoff | same test: a passed bound with an immediate capture slot is refused unsettled and uncaptured; an admission write held past the deadline answers 504, starts no attempt and is accounted `timed_out` |
| G7 | 26 | Held-lane expiry `[300ms,1s)`, cancellation, the hung provider's exact problem, `timed_out` capture, accounting and the next-call assertions are kept | same test, existing assertions unchanged |
| G9 | 22 | Completed permission decisions stay audited when a bound refuses the call | same test: the passed-bound phase's requested and fallback invoke decisions are staged |
| G10 | 22 | The capture-slot wait reuses the deadline and its staging refusal | `capturing_call_waits_for_a_capture_slot_and_refuses_at_its_deadline`; the bound path passes the same `deadline` |
| G11 | 24 | Settlement precedes the buffered answer, so a follow-on call does not meet its own hold | same test: under a concurrency limit of one, the call right after an expired admission is admitted |
| G12 | 24 | Caller cancellation stays distinct from deadline expiry | same test: a lane-bound call its caller cancels mid-attempt is accounted `cancelled` while the expired calls stay `timed_out`; a call dropped during a held snapshot dispatches nothing and keeps no permit |
| G8 | 22, 26 | Unbound callers keep the relative timeout; the adapter and admission/settlement regressions stay green | `gateway_agent_calls_start_only_on_a_compatible_adapter`, `gateway_admission_holds_limits_and_budgets_across_replicas` |
| C1 | 32, 38 | A valid env override wins over a zero file count (`queued_executions = 0`, `WYRD_VERIFICATION_QUEUED_EXECUTIONS=9`) | `verification_capacity_file_values_resolve_under_env` |
| C2 | 32, 38 | A valid env pool wins over an overflowing file pool MiB `9223372036854775807` | same |
| C3 | 34, 38 | A valid env deadline wins over a parseable out-of-range file deadline `9223372036854775807` | same |
| C4 | 38 | Without overrides, the zero count, the overflowing pool and the out-of-range file deadline are refused by their TOML names | same |
| C5 | 38 | Invalid env winners are refused by name, including `WYRD_VERIFICATION_EXECUTION_TIMEOUT_MS=18446744073709551615`, even over a valid file value | same, and `verification_limits_env_applies_and_refuses` |
| C6 | 33, 38 | Selected direct/queued execution, model-call and workspace counts above `Semaphore::MAX_PERMITS` are refused by the selected name from `resolve_limits`, before any semaphore exists | both config tests |
| C7 | 33, 38 | An overflowing or excessive Python workers-plus-waiters aggregate is refused | both config tests |
| C8 | 34, 36, 38 | The queued deadline is validated as signed milliseconds, interval microseconds and a `DateTime<Utc>` from now; values just inside are accepted and just outside refused; the ordinary two-hour value is kept | both config tests |
| C9 | 38 | Approved defaults, checked worker overflow, memory and prewarm refusals are kept | the four U selectors, existing assertions unchanged |
| D1 | 42 | `Scripted::dispatch` documents its poisoned-lock panics | rustdoc at `pg_invocation_tests.rs` |
| K1 | 46, 60 | fmt, lints, `git diff --check`; docs lane if operator wording changes; no new API, dependency or setting | checks |

## Implementation evidence

Commits after the reviewed candidate `ea4460c3c`: 726536f82 (ledger), d050a6702 (deadline carried through admission), 509f56eb2 (config selection and representation), a8a6a0666, e5cd84932, 5b863220e, 6deeb6589 (gateway proofs), 3fc98df76 (evidence), 1e6abb69a (bound capture wait, dropped call, G1 lower bound), 844ba2e2b (cancelled bound call). Production changes are `components/gateway/caller.rs`, which fixes one `Instant` before the lane wait and passes it to `GatewayInvocation::bounded`, and `components/gateway/invocation.rs`:
- the `bounded` handle (`:305`);
- `timeout_at` around reversible preparation only (`:585–628`);
- the admitted and capture deadline reused (`:632–634`);
- the check before `admit` (`:663`).

The other production change is `config.rs`: `Selected` and `set_count` (`:2089–2185`), plus the Python admission total check (`:2062–2078`). Operator wording is in `reference/verification-runtime-limits.md` and the generated `llms-full.txt`. Gateway test lines refer to `pg_invocation_tests.rs` and config lines to `config.rs`.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| G1: refuse a held cold snapshot past the original deadline | `invocation.rs:585–628` | 504 code `:6195` and end within `[budget, budget + 25%)` `:6199–6202`, both asserted after the lock waiter is observed (`:6186`) and before the barrier commits (`:6212`) | G 5/5 | PASS |
| G2: no dispatch | same | `:6203–6205` | G | PASS |
| G3: no permit kept | `caller.rs` permit drops with the call | `:6207–6210` | G | PASS |
| G4: success after release | same | the next call after release is admitted and answered `:6303–6307` | G | PASS |
| G5: one instant for the lane wait and the engine deadline | `caller.rs:139–151`, `invocation.rs:632–634` | the dispatched deadline lies in `[started+budget, started+budget+budget/8]` after a delayed snapshot `:6269–6273` | G | PASS |
| G6: expiry refused before the ledger write; a write already in progress hands off | `invocation.rs:663` | passed bound with the capture slot in hand: 504, unsettled, undispatched `:6414–6419`; uncaptured `:6422–6425`; write held past the deadline: 504 `:6344`, no attempt `:6348–6351`, `timed_out` capture `:6376–6379` | G | PASS |
| G7: existing assertions kept | unchanged | held lane `[300ms,1s)` `:6135–6144`; cancellation `:6155–6169`; hung provider's problem `:6263`; `timed_out` capture `:6285–6288`; next call `:6305–6307` | G | PASS |
| G8: unbound path and regressions | `caller.rs:168` passes no bound; `invocation.rs:634` keeps the relative timeout | `gateway_agent_calls_start_only_on_a_compatible_adapter`, `gateway_admission_holds_limits_and_budgets_across_replicas` | G | PASS |
| G9: completed decisions stay audited | audits are staged synchronously in `decide` | `:6428–6434` | G | PASS |
| G10: capture wait and staging refusal | the bound path passes the same `deadline` to `capture_slot` | bound call with a 10 s relative timeout refused `GatewayCaptureStagingFull`, undispatched `:3493–3497`, at its 200 ms bound `:3498–3500` | G | PASS |
| G11: settlement before the answer | existing `settle_buffered` order unchanged | the next call under `concurrent_calls = 1` is answered `:6368–6369` | G | PASS |
| G12: cancellation distinct | drop guard `invocation.rs:490` unchanged | cancelled mid-attempt: no permit kept `:6456–6460`, outcome `cancelled` `:6467–6471`, the two expiries still `timed_out` `:6472–6479`; dropped call undispatched `:6225–6228`, no permit `:6229–6233` | G | PASS |
| C1–C3: valid env wins over stale file values | `Selected::of` checks a file value only when no variable is set | `(9, 8192 MiB, 2h)` over zero, overflowing and i64::MAX file values, `config.rs:4677–4690` | U 4/4 | PASS |
| C4: file refusals by TOML name without overrides | `Selected::of`, `set_count`, `Selected::deadline` | `:4714`, `:4734`, `:4805–4816` | U | PASS |
| C5: invalid env winners refused | `env_positive` and `Selected::refuse` | `:4692–4711`; env list `:4912–4928` | U | PASS |
| C6: counts above `Semaphore::MAX_PERMITS` refused; the maximum itself applies | `set_count` | file `:4767–4768`, `:4785–4816`; env `:4932–4945` | U | PASS |
| C7: Python admission total overflowing or excessive | `config.rs:2062–2078` | file excess `:4801`; env overflow `:4921` | U | PASS |
| C8: deadline representation | `Selected::deadline`: `TimeDelta::from_std`, then `Utc::now().checked_add_signed` | just inside is accepted `:4770–4782`; just outside, i64::MAX and u64::MAX are refused `:4805–4810`, `:4918–4928`; two hours `:4688` | U | PASS |
| C9: defaults, worker overflow, memory and prewarm refusals kept | existing assertions unchanged | `lanes_split_the_memory_ceilings`, `production_defaults_are_the_specified_ceilings`, `:4740–4756`, `:4823` | U | PASS |
| D1: `Scripted::dispatch` panics documented | rustdoc | `pg_invocation_tests.rs:117–119` | lints 0 | PASS |
| K1: checks and scope | `bounded` is `pub(crate)`; no new setting, API or dependency | n/a | fmt 0, lints 0, `git diff --check` 0, check:deps 0, docs:check 0 | PASS |

**RED.** Each obligation failed before its fix took effect:
- G1: the new phase failed at `:6143`, timing out after 10 s, because the call was still blocked on the held snapshot.
- G5: restoring `Instant::now() + request.timeout` failed at `:6209`.
- G6: disabling the check before `admit` failed at `:6360`; the call was admitted and settled by the engine.
- C1–C3: the zero file count refused before the env winner, at `:4578`.
- C5 and C8: the env `u64::MAX` deadline was accepted, at `:4813`.
- C6: with the count bound disabled, the tests failed at `:4933` and `:4810`.
- C7, excess: with the excess branch disabled, the test failed at `:4810`.
- C7, overflow: with waiters dropped from the sum, the test failed at `:4919`.
- C8, timestamp: with the timestamp check disabled, the tests failed at `:4919` and `:4810`.
- G10: giving the capture slot `Instant::now() + request.timeout` instead of the bound failed at `:3498` after 10.0065 s.
- G12: making the engine report `TimedOut` on caller cancellation (`wyrd-gateway/src/engine.rs:642`) failed at `:6466` with outcome `timed_out`.
- Every mutation was restored, and its diff is empty.

**Characterization, not new behavior.** G7, G9, G11 and C9 hold behavior that already existed. They have no RED. RED cites the line numbers at the time of the run.

**G6 reachable form.** A capture slot that frees only after the bound cannot reach the pre-ledger check, because `reserve_until` already refuses at the same deadline (G10). The check before `admit` is therefore proved by a bound that has passed when the slot is granted at once.

**C7 naming.** A file-sourced workers-plus-waiters refusal names the env variables, following the existing cross-field refusal convention in `resolve_limits`.

**Diagnosis (new audit assertion).**
- **Symptom:** the first version of the G9 assertion got 19 decisions where it expected 18 (`:6375`).
- **Evidence:** `configure` installs a global `dep-b/b` fallback, and `route` audits each fallback candidate's invoke decision.
- **Cause:** the expectation counted only the requested model.
- **Fix site:** the test only. An independent diagnostician confirmed this and found no other callers affected. The assertion now pins both allowed `gateway.invoke` decisions.

**Commands.**
- U: the four exact selectors given in the task.
- G: the task's three managed-Postgres selectors, plus `capturing_call_waits_for_a_capture_slot_and_refuses_at_its_deadline` and `gateway_terminal_spans_classify_every_call_and_attempt`; 5/5 pass.
- `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run docs:check` and `git diff --check ea4460c3c`.

**Limits and non-goals.**
- The deadline check uses the clock at startup. A value within uptime of the year-262143 limit can still overflow at claim time; that is theoretical.
- A ledger write held past the deadline answers only once the lock is released. That is the accounting order the task requires, not a reply-time SLA.
- No full journey suite, gate, benchmark or platform matrix was run.
- No historical report was edited.
