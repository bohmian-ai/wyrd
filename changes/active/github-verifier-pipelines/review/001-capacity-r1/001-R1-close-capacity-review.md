---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-github-verifier-pipelines
spec_revision: 3
parent_task: TASK-001
remediates: [FIND-001-1, FIND-001-2, FIND-001-3, FIND-001-4, FIND-001-5]
depends_on: []
---

# Close the capacity-isolation review

Route directly to `$wyrd-implement` under the existing approved behavior.

Approved spec: `changes/active/github-verifier-pipelines/spec.md`, revision 3, REQ-015/016, AC-010, INV-001/005/007. Original task: `changes/active/github-verifier-pipelines/tasks/001-isolate-queued-capacity.md`. Reviewed base: `395274d58bf76a2075f245ca3a5c1bd044925027`; original candidate: `34fb45bb387582d2aedfee77fb4e09fa2753b80d`. Retain that cumulative range, this remediation, `verdict.md` and `findings-validation.md` as the next review's context.

The independently validated ledger is `changes/active/github-verifier-pipelines/review/001-capacity-r1/findings-validation.md`. The real isolation journey and four focused server units pass on the original candidate; the recovery test fails. The reservation design and single-engine ownership are accepted. Correct the five bounded gaps below without changing those decisions.

## Diagnosis and selected correction

### FIND-001-1: reject overflowing combined worker counts at the existing owner

`verification/hosted/mod.rs:140–144`, `HostedLimits::lanes_fit`, adds direct and queued worker counts before its checked conversion/multiplication. Config's existing `set_scaled` accepts the positive 64-bit maximum as a worker count on every supported platform. With the direct default of 2, the queued value `18446744073709551615` therefore panics during validation in checked builds, or wraps the aggregate to 1 and incorrectly passes the shared memory ceiling in wrapping builds. Both config validation and Sandbox startup consume this same predicate. This violates typed refusal of inconsistent operator limits and the pool memory invariant; it does not require supporting an enormous worker pool.

Make that aggregate addition checked and return the predicate's existing refusal result when it cannot be represented. Reuse the existing named ConfigError and PoolError paths. The correction belongs at the shared producer of the false memory-fit result, before either lane starts. A downstream guard, new count/config abstraction or a sweep of unrelated semaphore limits is unnecessary; specifically, do not add the extra sandbox guard proposed during discovery merely for this already-refused worker value. Keep existing memory and per-worker checks.

### FIND-001-2: spend one original call budget across model admission and invocation

`components/gateway/caller.rs:128–170` inserts a lane semaphore wait before forwarding the unchanged full `call.timeout`. The supplied Skald call contract says the caller enforces its remaining duration; its actual provider adapter supplies ten minutes and a never-cancelled token. Ordinary Agents may have no shorter authored timeout. This task also exposes a supported two-hour queued execution ceiling. With one queued model permit and multiple queued executions, nine minutes waiting followed by a two-minute provider call can now succeed after eleven minutes for a ten-minute operation. Contention can also postpone dispatch beyond the call budget. The original whole-execution ceiling still works; the passing ten-second cancellation journey does not establish this smaller contract. Overdue work holds a runner slot and may start chargeable provider work after its original bound.

At the existing ServerWyrdGatewayCaller owner, enforce the original per-call deadline across both lane acquisition and the existing governed invocation. Expiry while waiting returns the existing ProviderError timeout before dispatch; acquiring a permit cannot restart the budget. Reuse existing Tokio timeout/cancellation and owned-permit cleanup. Keep GatewayInvocation authoritative for authorization, routing, provider cancellation, tracked accounting and settlement; retain its drop/cancellation behavior if the enclosing call expires. Preserve unbound callers, request projection/refusal and response behavior. No new scheduler, timeout option, retry rule, execution deadline or provider client is needed. Gateway-only enforcement cannot account for the wait it never sees.

### FIND-001-3: establish the recovery test's intended race before comparison

`sdks/wyrd-sdk-rust/tests/integration/verifier_orchestration.rs:2065–2194`, `speculative_recovery_cannot_resurrect_content`, claims a late comparison after admitted extra work completes, or while excluded extra work is in its second call. Its setup holds the consolidator and some EXTRA responses but never holds COMPARE. The shared reply producer answers BOOTSTRAP and COMPARE immediately. Compare can therefore decide before extra starts or before its publication/invocation milestone, while later assertions require speculative disposition and an extra attempt. The missing setup ordering explains the two recorded failure branches. The exact managed-Postgres rerun fails again on the immutable candidate; its bounded output did not reveal which assertion failed. This is a confirmed red gate and source race, not a new production recovery defect. AGENTS explicitly requires repairing a red affected gate regardless of ancestry.

Use the existing HeldUpstream and `reached` milestone mechanism demonstrated by `speculative_disposition_controls_consumer_admission` to establish the intended ordering in this recovery test. Keep comparison held until the admitted extra action is accepted as completed; for discarded/blocked cases establish its publication and intended second-call/running state before allowing comparison to decide. Preserve the consolidator stall, lease takeover, invocation counts/no replay, mounted-content, original-attempt and retained-publication assertions. Keep production engine/admission semantics and mock response semantics unchanged. Do not restore timing delays, loosen assertions, enlarge timeouts or add retries. Preserve an evidence-backed diagnosis and the exact failure/closure results; a lucky green repetition cannot replace the ordering precondition.

### FIND-001-4: put runtime composition proof in its existing server-test owner

The new SDK test at `isolated_capacity.rs:153–160,240–249,405–412` steers internal hosted lane/direct execution semaphores and polls formatted server logs with sleeps. It is registered in the SDK integration binary. `TESTING.md:331–340` and the narrower `WyrdTestServer` control contract at `wyrd-testing/src/server.rs:186–204` explicitly exclude these runtime-only controls and polling from client-facing SDK stories. The mandatory boundary is violated and the intended server capability lane lacks AC-010's proof, despite the current journey passing.

Move this one real resource-composition story to the existing `wyrd-testing` `server` target under verification-runtime ownership, as the original task prescribed. Adapt its local fixture provisioning to that target's existing bound-server/bootstrap/native-client and Card declaration/pull/hydration patterns. The feasibility evidence is concrete: `verification_runtime.rs:5381–5452,7851–7910` already declares YAML, pulls a graph, hydrates WyrdState and verifies/submits through the client. The SDK directly reexports these same wyrd_client types. Existing target dependencies include the client, server test support, spec, wiremock and tempfile; public gateway administration and CredentialWriter provide mock-route setup. No SDK dependency, reverse dependency, extra reusable harness or new target/lane is required. The supporting trace is in `followup-review.md`.

Retain the current shared YAML fixtures and all genuine client declare/pull/hydrate/verify/submit/status/wait/Bifrost round trips. Preserve all four distinct saturation cases, direct success, queued waiting/defer/reclaim, reverse no-borrowing, cancellation/release, execution ceiling and tenant/subject accounting assertions. Runtime holds and live LogCapture remain valid at their runtime owner. Remove the SDK story registration/source once the proof is registered in its server owner. Wrapping internal accesses in another SDK helper, weakening the control contract, duplicating the story, using EngineScript/seeded outcomes or replacing client calls with direct engine calls does not close this finding. Existing ordinary SDK stories stay in place. The move preserves literal client→WyrdTestServer/Postgres→client proof and requires no spec/task correction.

### FIND-001-5: complete the changed defaults test's panic contract

`verification/mod.rs:626–665`, `production_defaults_are_the_specified_ceilings`, materially adds assertions for queued slots and timeout but has no `# Panics` section. The root changed-item rustdoc gate and `agent_docs/rust.md` explicitly apply to private/test items. Add the missing contract for failure of the function's asserted default ceilings/invariants, retaining all assertions. This is a mandatory gate bundled with substantive remediation, not a separate style round. Do not expand documentation work to untouched neighboring items.

## Preserved behavior and scope

Keep separate, nonborrowing execution/workspace/Python/model admissions, mode selection for every queued claim/reclaim, direct pre-step eligibility, the one HostedBlocks engine, existing accepted authority/evidence and tenant/subject accounting, original persisted budgets/deadline, shared downstream CPU/Postgres/gateway/providers, and portable behavior. Keep existing config names/defaults and owner/operator docs accurate. No new API, dependency, queue, service, persistent format, future queued-only action, availability SLA or platform enforcement. Historical production-before-RED remains accurately recorded; do not manufacture retrospective compliance.

Update the implementation evidence with actual correction results and the relocated journey selector. Do not change historical review reports. Any material decision beyond these approved owner corrections returns for specification review rather than becoming remediation scope.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| FIND-001-1 | An unrepresentable combined worker count is refused without panic by the shared predicate; the maximum-width env case returns the normal named configuration refusal. Existing valid/default/memory cases still pass. |
| FIND-001-2 | A held model lane times out within a short original call budget without gateway/provider dispatch. Admission after part of that budget leaves only the original remaining duration; provider completion beyond it times out. Cancellation/expiry drops permit/wait state and the following call can proceed; existing gateway accounting/settlement remains owned by GatewayInvocation. |
| FIND-001-3 | Source-visible milestone ordering establishes all three late-decision scenarios before comparison. The exact recovery test and its directly related disposition test pass with every takeover/content/attempt assertion retained. |
| FIND-001-4 | The existing server target contains and runs the full real-client capacity journey, with all current assertions and YAML retained; SDK registration/internal-control story is removed. No alternate engine or unsupported SDK control is introduced. |
| FIND-001-5 | The materially changed defaults test has meaningful panic documentation and its exact test passes. |

Use the owning pure memory/config/defaults tests for arithmetic and rustdoc closure. Their exact selector (all three selected tests must run):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tests::lanes_split_the_memory_ceilings) | test(=config::tests::verification_limits_env_applies_and_refuses) | test(=verification::tests::production_defaults_are_the_specified_ceilings)'
```

For model timeout closure, extend existing caller/gateway proof in `components/gateway/pg_invocation_tests.rs`, using its actual caller and recorded dispatch, held lane admission and short call durations. No ten-minute test or load generation. For relocation use the existing server integration target and repository-managed Postgres. Confirm actual test names/targets and record the new tests' exact managed selectors before RED; do not invent selectors in this task. Run the exact new/changed tests, including the relocated capacity journey, rather than the full server lane.

For the two existing orchestration tests:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai -P journey --run-ignored=all -E 'test(=verifier_orchestration::speculative_recovery_cannot_resurrect_content) | test(=verifier_orchestration::speculative_disposition_controls_consumer_admission)'"
```

Keep Cargo work sequential in the shared checkout. Apply required format/lint checks for the write set through existing mise tasks, plus `git diff --check`. No full journey suites, gate, broad aggregate, platform matrix or unrelated tests are required for this bounded remediation; integrated change review runs the required full capability lanes once. If a focused check fails, diagnose/fix at its owner without weakening or bypassing it.

## Obligations

Contract: this remediation plus the original task 001, under spec revision 3. The ledger rows come from the acceptance table and the preserved-scope paragraph above.

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| R1 | FIND-001-1 | `lanes_fit` refuses a combined worker count it cannot represent, with no panic or wrap | `lanes_split_the_memory_ceilings`: queued `python_workers = usize::MAX` gives `!lanes_fit()` |
| R2 | FIND-001-1 | A maximum-width env worker count gets the normal named configuration refusal | `verification_limits_env_applies_and_refuses`: `QUEUED_PYTHON_WORKERS=18446744073709551615` is refused naming `WYRD_VERIFICATION_DIRECT_PYTHON_WORKERS plus` |
| R3 | FIND-001-1 | The valid, default and memory cases still pass | the existing assertions in both tests, retained |
| R4 | FIND-001-2 | A held model lane times out within a short original call budget, with no gateway or provider dispatch | new caller pg test: a held lane plus a short timeout gives `ProviderError::Timeout`, and the dispatch log is empty |
| R5 | FIND-001-2 | Admission after part of the budget leaves only the remaining duration, and provider completion beyond it times out | same test: the dispatched attempt's deadline falls at the call's start plus its budget, not at admission plus the budget; a hanging provider ends by the original deadline |
| R6 | FIND-001-2 | Cancellation or expiry drops the permit and wait state, and the next call proceeds; accounting stays with GatewayInvocation | same test: all permits are available after each timeout, and a following call is answered |
| R7 | FIND-001-2 | Unbound callers, request projection and refusal, and response behavior are unchanged | `gateway_agent_calls_start_only_on_a_compatible_adapter` still passes |
| R8 | FIND-001-3 | Source-visible milestones establish all three late-decision orderings before comparison | the recovery test holds COMPARE at a `HeldUpstream`; admit releases it once `extra` is accepted as Succeeded with no disposition; discard and block release it once `extra`'s second call has reached the upstream |
| R9 | FIND-001-3 | The recovery and disposition tests pass with every takeover, content and attempt assertion retained | the exact pair selector |
| R10 | FIND-001-4 | The server target holds and runs the full real-client capacity journey with all current assertions and the YAML | `server` target `verification_runtime::isolated_capacity::direct_verification_runs_while_queued_capacity_is_held` |
| R11 | FIND-001-4 | The SDK registration and source of that story are removed, with no alternate engine or SDK control | `isolated_capacity.rs` deleted and its `mod` line dropped; the diff audit |
| R12 | FIND-001-5 | The changed defaults test has meaningful `# Panics` documentation and its exact test passes | rustdoc at `verification/mod.rs`, plus the exact selector |
| R13 | Preserved scope | Format, lints, `check:deps` and `git diff --check` pass; no new API, dependency or config | the lanes and the diff audit |


## Human-approved revision — capacity defaults and server configuration (2026-11-10)

The user approved this addition after remediation had started. It is new human-directed scope, separate from the independently validated FIND-001-1 through FIND-001-5; do not assign another finding ID or rewrite the historical review reports. Continue the existing remediation and include this revision in its delivery and closure review.

This revision supersedes the earlier instruction to keep existing capacity defaults and the “no new … config” restriction in R13, only for the settings specified here. All five findings, their corrections and other preserved behavior remain applicable.

### Revised defaults

| Per-process setting | Previous default | Approved default |
|---|---:|---:|
| Direct executions | 16 | 256 |
| Queued executions | 64 | 64 |
| Direct concurrent model calls | 16 | 256 |
| Queued concurrent model calls | 48 | 256 |

Retain the existing Python worker/waiter/prewarm, workspace, memory and execution-timeout defaults. Keep direct and queued ownership independent, with no borrowing and the same engine and gateway governance. These values provide initial concurrency headroom; they are not benchmark-backed throughput or latency guarantees. Resource-heavy workflows remain constrained by their Python/workspace allowances, and downstream gateway/provider capacity remains shared.

### Use the normal startup configuration pattern

Make the operator-facing verification capacities and execution timeout ordinary typed server configuration settings, following the existing server/Bifrost startup configuration conventions exactly: owner defaults, configuration-file values, environment overrides, then startup validation and composition of the existing runtime owners. Preserve the standard precedence of environment over configuration file over defaults. All existing per-lane execution/workspace/Python/model-call capacity overrides and the original queued execution timeout must work through this path; keep existing environment variable names.

The current `VerificationConfig.limits` and `.runtime` fields are skipped during deserialization, leaving these settings environment-only. Close that gap through the existing configuration owner and loader. Nearby `LimitsConfig` and Bifrost runtime configuration demonstrate the established pattern; reuse the normal loader, override and validation stages. Do not introduce a parallel loader, configuration service, new initialization path or dependency. Do not expose internal lease, rewrap or maintenance timings merely because they share a runtime struct with an operator-facing setting. Express duration units using the existing configuration conventions.

Validate the final merged values consistently regardless of source: configuration-file values must receive the same positivity, representability, memory-fit and cross-field checks as environment values. Invalid file values must not bypass validation that currently occurs during environment parsing. Keep FIND-001-1's checked aggregate arithmetic at its existing owner. Startup must project the resolved settings into the same HostedBlocks and queued runtime owners; configuration support must not create another resource owner.

### Delivery and focused proof

Update affected default assertions, configuration tests, operator configuration/limits documentation and implementation evidence. Remove the obsolete environment-only/no-TOML claim for these settings. Regenerate affected documentation projections using the existing generator; do not hand-edit generated files.

Prove the approved defaults, file-only overrides reaching the resolved runtime settings, environment-over-file precedence, and invalid file/environment refusal with focused tests at the existing configuration/default owners. Include zero, unrepresentable and inconsistent memory/count cases relevant to these settings, using the existing test patterns rather than a new harness. Retain the exact default/config selectors above; record actual exact selectors for new tests before RED. Preserve the real-client isolation journey and all five findings' focused closure proof. Run only the additional format/lint/generated-doc checks and documentation lane applicable to this revision's write set; no performance claim or new benchmark gate is required.

The implementation obligations/evidence ledger must include this approved addition rather than claiming completion solely from R1–R13. Review this revision as human-directed acceptance scope alongside the original task, cumulative candidate and retained findings.

## Obligations for the approved revision

R13's "no new config" clause is superseded only for these settings.

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| H1 | Revised defaults | Direct executions 256, queued executions 64, direct and queued model calls 256 each; Python, workspace, memory and timeout defaults unchanged | `lanes_split_the_memory_ceilings` asserts `(direct_executions, direct.model_calls, queued.model_calls) == (256, 256, 256)` beside the retained worker, workspace and waiter totals; `production_defaults_are_the_specified_ceilings` keeps `(64, 5 min)` |
| H2 | Startup pattern | Per-lane workspaces, Python workers and waiters, model calls, direct and queued executions, and the execution timeout are typed `[verification]` file settings on the existing loader; lease and maintenance timings stay internal | new `config::tests::verification_capacity_file_values_resolve_under_env`: TOML values reach `limits` and `runtime` |
| H3 | Precedence | Env over file over defaults, with the existing env names | same test: the env value wins over the file value for one setting, and the other file values stay |
| H4 | Validation parity | Zero, unrepresentable and inconsistent file values get the same refusals as env values | same test: a zero file count is refused naming its key; file Python workers that overflow the pool are refused by the shared `lanes_fit` predicate; an env `WYRD_VERIFICATION_MONTY_MIN_WORKERS` that conflicts with file Python workers is refused (`monty_min_workers` has no TOML key); a negative value fails TOML parsing |
| H5 | One owner | The resolved settings reach the same HostedBlocks and runtime owners, with no new loader or owner | `VerificationConfig::limits`/`runtime` stay the only projection; the diff audit |
| H6 | Docs | Operator limits and configure pages describe the file keys and the new defaults, the env-only claim is removed, and generated projections are regenerated | `docs:check`, plus the docs diff |


## Human-approved revision 2 — Python and workspace defaults (2026-11-10)

The user approved increasing the resource defaults below after the first revision. This is additional human-directed scope, separate from FIND-001-1 through FIND-001-5. Append its obligations and evidence to the ongoing remediation; do not rewrite historical review reports or assign a new finding ID.

This revision supersedes the first revision's instruction to retain Python worker/waiter, workspace and combined pool-memory defaults, including that part of H1 and earlier assertions preserving their old totals. The first revision's execution/model-call defaults, standard startup configuration requirements and all five findings remain in force.

### Approved resource defaults

| Per-process setting | Previous default | Approved default |
|---|---:|---:|
| Direct Python workers | 2 | 4 |
| Queued Python workers | 3 | 12 |
| Direct Python waiters | 4 | 16 |
| Queued Python waiters | 12 | 48 |
| Direct workspaces | 4 | 16 |
| Queued workspaces | 12 | 48 |
| Combined Python pool accounting budget | 2048 MiB | 8192 MiB (8 GiB) |

Retain one prewarmed Python worker per lane, 256 MiB per-operation memory and 384 MiB per-worker accounting, plus existing operation/execution timeouts and per-workspace byte/file budgets. Retain lazy worker creation and existing cleanup, cancellation, admission, deferral and no-borrowing behavior. Waiters provide bounded waiting admission, not additional executing workers.

These are initial production defaults selected with a 32 GiB server as the sizing assumption, not benchmark-backed throughput claims or a guarantee that every combination of workloads fits that host. The 16 maximum Python workers account for 6144 MiB at 384 MiB each, within the 8192 MiB pool budget. That budget is startup accounting, not an upfront allocation or a hard whole-process RSS reservation; interpreter stacks, runtime overhead and other server work also consume memory. Workspace count similarly bounds concurrent temporary workspaces, not reserved RAM or disk. Preserve existing operator guidance to size memory and temporary storage for the workload. No host-size detection, automatic sizing, platform-specific enforcement or new aggregate resource mechanism is requested.

### Configuration and delivery

Apply these defaults at the existing owners and expose their overrides through the same normal startup configuration path required by revision 1: defaults, typed server configuration, existing environment overrides, then merged-value validation. Preserve existing environment variable names, including `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB`. All approved Python/workspace settings and the pool budget must be configurable through that path. Keep internal lifecycle timings private and avoid a parallel loader or resource owner.

Update affected defaults/configuration assertions, operator configuration and limits documentation, generated documentation projections and implementation evidence. Preserve all real-client capacity assertions and the five findings' focused proof; explicit small test overrides may remain small where they establish saturation deterministically. Do not enlarge test resource usage merely to match production defaults.

### Focused acceptance additions

- Default assertions establish direct/queued workers `(4, 12)`, waiters `(16, 48)`, workspaces `(16, 48)`, pool memory `8192 MiB`, and one prewarmed worker per lane. Update old combined-total expectations to workers/workspaces/waiters/prewarm `(16, 64, 64, 2)` while retaining meaningful overflow, invalid-memory and per-worker checks.
- Configuration proof establishes these settings, including the pool budget, resolve from file values and existing environment overrides under the standard precedence and receive the same merged-value validation. The approved defaults pass the existing shared memory-fit predicate; inconsistent overridden capacities/budgets still refuse startup.
- Documentation and evidence reflect both approved revisions without claiming measured throughput or treating the pool budget as a whole-process memory reservation.

Reuse the existing exact default/configuration selectors and affected documentation checks already required by the packet. Record exact selectors for any newly necessary focused tests before RED. No new benchmark gate, full journey suite, dependency or unrelated cleanup is part of this revision.

## Obligations for approved revision 2

These rows supersede H1's "Python, workspace, memory unchanged" clause.

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| H7 | Revision 2 defaults | Workers `(4, 12)`, waiters `(16, 48)`, workspaces `(16, 48)`, pool 8192 MiB, one prewarmed worker per lane; combined totals `(16, 64, 64, 2)`; the defaults pass `worker_fits` and `lanes_fit` | `lanes_split_the_memory_ceilings`, with its overflow, pool-below-process and per-worker refusals retained against the new pool |
| H8 | Revision 2 configuration | The pool budget resolves from `[verification] monty_pool_memory_mib` under `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB`, alongside the Python and workspace file values from H2; a zero or overflowing pool value is refused, and file workers that exceed a file pool are refused | `verification_capacity_file_values_resolve_under_env` |
| H9 | Revision 2 docs | Both pages show the new defaults and describe the pool as startup accounting, not a reservation or a measured guarantee | `docs:check`, plus the docs diff |

## Evidence

Exact selectors:

- U: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::tests::lanes_split_the_memory_ceilings) | test(=config::tests::verification_limits_env_applies_and_refuses) | test(=verification::tests::production_defaults_are_the_specified_ceilings) | test(=config::tests::verification_capacity_file_values_resolve_under_env)'`. 4/4 PASS.
- G: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::lane_wait_spends_the_agent_call_budget) | test(=components::gateway::pg_invocation_tests::gateway_agent_calls_start_only_on_a_compatible_adapter)'"`. 2/2 PASS.
- O: the remediation's orchestration pair selector. 2/2 PASS (recovery 11.9 s, disposition 14.2 s).
- J, the relocated journey: `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::isolated_capacity::direct_verification_runs_while_queued_capacity_is_held)'"`. PASS in 21.2 s.
- `mise run fmt` 0, `mise run lints` 0, `mise run check:deps` 0, `mise run docs:check` 0, `git diff --check 395274d58 HEAD` 0.

RED:

- R1/R2: before the fix, both new cases panicked with `attempt to add with overflow` at `hosted/mod.rs:141`.
- R4–R6: before the fix, the lane wait outlived a 300 ms budget, failing the test's 10 s bound on the wait (`pg_invocation_tests.rs`, the "lane wait ends within the call's budget" expect).
- H2–H4, H8: the production change landed before its test, which is a process exception. RED was then taken by mutation: with `set_file` made to ignore its value, the new config test failed with `left: (256, 64, 300s)`, `right: (7, 9, 7200s)`.
- H1/H7: the default changes landed with their assertions in the same edit, so no RED was taken. The assertions pin the approved values.
- R8: the diagnosis comes from the validated FIND-001-3. COMPARE was never held, and the reply script answers it at once, so the comparison could decide before `extra` started or before its publication. The test now establishes the ordering; production code is unchanged.
- R6 cancellation: RED by mutation. With the `cancellation.cancelled()` arm in `caller.rs` replaced by a pending future, the cancelled-wait phase failed at `pg_invocation_tests.rs:6109` with `Timeout { provider: "wyrd_gateway" } after 30.002144269s`. The original was then restored.
- R10: the code was moved, not changed. The journey's RED is still task 001's mutation RED.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| R1: unrepresentable combined workers are refused without panic | `hosted/mod.rs:143-145`, `checked_add` before the conversion | `hosted/mod.rs:248-251` | U | PASS |
| R2: the max-width env value gets the named config refusal | the shared `lanes_fit` refusal in `resolve_limits` | `config.rs:4730` case, expecting `WYRD_VERIFICATION_DIRECT_PYTHON_WORKERS plus` | U | PASS |
| R3: valid, default and memory cases still pass | unchanged checks | the retained assertions in both tests | U | PASS |
| R4: a held lane times out within the call budget, with no dispatch | `caller.rs:135-158`, a `tokio::time::timeout(call.timeout, acquire_owned)` | `pg_invocation_tests.rs:6083-6097` (the wait ends in `[300 ms, 1 s)`, Timeout, `seen()` empty) | G | PASS |
| R5: admission leaves only the remainder, and a provider past it times out | `caller.rs:155-158,180`, `timeout = call.timeout - elapsed` passed to GatewayInvocation | `pg_invocation_tests.rs:6103-6120` (`WYRD_GATEWAY_504_DEADLINE_EXCEEDED`; dispatched deadline at or before start + budget + 25%; the call ends before budget + 25%) | G | PASS |
| R6: expiry and cancellation drop permit and wait state, and the next call proceeds | an owned permit dropped on return; the gateway still owns settlement | `pg_invocation_tests.rs:6100-6122` (a pre-cancelled token ends a held-lane wait in under 5 s of a 30 s budget, with nothing dispatched and no permit taken); `:6154,6158-6165,6181-6183` (permits restored, the hung call captured `timed_out`, the next call answered) | G; J (cancelled direct wait, then a call that answers) | PASS |
| R7: unbound callers, projection, refusal and response are unchanged | the `None` arm passes `call.timeout` unchanged | `gateway_agent_calls_start_only_on_a_compatible_adapter` | G | PASS |
| R8: milestone ordering precedes the comparison | `verifier_orchestration.rs:2118-2156`: COMPARE held at a `HeldUpstream`; admit waits for `extra` to be Succeeded with no disposition; discard and block wait for `extra`'s turn-1 request, which carries its publish result, while `extra` is still Started | `verifier_orchestration.rs:2122-2133,2137-2153` | O | PASS |
| R9: both tests pass with every assertion kept | only the setup changed; the takeover, content, attempt and retained-publication assertions are untouched | the existing assertions | O | PASS |
| R10: the server target runs the full real-client journey | `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime/isolated_capacity.rs`, registered at `verification_runtime.rs:87-88`; the same YAML, client declare/pull/hydrate/verify/submit/status/wait and Bifrost reads; gateway setup through public `Gateway` plus `CredentialWriter` | every assertion of the former SDK story, carried over unchanged | J | PASS |
| R11: the SDK story is removed, with no new SDK control or engine | `isolated_capacity.rs` removed from the SDK and its `mod` line dropped from `main.rs`; the fixtures README points at the server target | `git grep isolated_capacity` finds only the server module and the README | diff audit | PASS |
| R12: the defaults test documents its panics | `verification/mod.rs:632` `# Panics` | — | U | PASS |
| R13: format, lints, deps and diff check; no new API or dependency | no Cargo or manifest changes; the new config keys are those the approved revisions require | — | the lanes above | PASS |
| H1: execution and model-call defaults are 256, queued slots stay 64 | `hosted/mod.rs` defaults | `hosted/mod.rs:237-243`; `verification/mod.rs` `(64, 5 min)` | U | PASS |
| H2: the capacities and timeout are typed `[verification]` file settings on the normal loader | `config.rs` `VerificationConfig` file fields plus `VerificationLaneConfig`; `resolve_limits` called from `apply_env_overrides` inside `WyrdServerConfig::load` | `config.rs:4498-4534`: file values reach `limits` and `runtime`, and lease and verify timeout keep their defaults | U | PASS |
| H3: env over file over defaults | file values folded first (`config.rs:1844-1904`), then the existing `set_scaled`/`set_millis` | `config.rs:4536-4556` | U | PASS |
| H4: file values get the same validation | `set_file` refuses zero; the existing merged inconsistency checks run after both sources | `config.rs:4557-4620`: zero values, overflowing pool, pool overflow, unrepresentable workers, a min-workers conflict, a negative value | U | PASS |
| H5: one owner, no new loader | `limits` and `runtime` remain the only projection into HostedBlocks and the runtime builder | — | diff audit; J | PASS |
| H6: the docs describe the file keys and defaults, and the env-only claim is gone | `reference/verification-runtime-limits.md`, `operate/configure-verification-runtime.md`; `llms*.txt` regenerated with `docs:generate` | — | docs:check | PASS |
| H7: revision 2 defaults | `hosted/mod.rs` defaults (4/12, 16/48, 16/48, 8192 MiB, prewarm 1) | `hosted/mod.rs:226-270`: exact per-lane workers, waiters, workspaces, the 8192 MiB pool and prewarm 1 (`:236-246`), totals `(16, 64, 64, 2)`, `worker_fits && lanes_fit`, and the extra-workers, pool-below-process and per-worker refusals | U | PASS |
| H8: the pool budget is file-configurable under its env name | `config.rs:1890-1903`, with overflow refused | `config.rs:4498-4556,4575-4582,4591-4596` | U | PASS |
| H9: revision 2 docs: startup accounting, no measured claim | the limits page's "sized for a 32 GiB server…" and pool paragraphs; the configure page's pool bullet | — | docs:check | PASS |

Mechanical deviations:

- `apply_limits_env` is renamed to `resolve_limits`, because it now resolves file values too. The env test keeps its name.
- The relocated journey sits in a child module of `verification_runtime`, so its imports stay local.
- R5's test sleeps for half its budget. Elapsed time is the quantity being measured, not a synchronization point.
- The `isolated_capacity` fixture row now names the server target.

Non-goals: no new API, dependency, queue, service, persistent format, scheduler, timeout option or platform enforcement. Lease and maintenance timings stay internal, production engine and admission semantics are unchanged, and no historical review report was edited.
