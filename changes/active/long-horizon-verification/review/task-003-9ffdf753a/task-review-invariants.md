# TASK-003 invariant review

Overall result: **FAIL** (two proposed findings, subject to independent validation).

## Immutable subject and authority

- Base: `ef3a57b0776dd7bc451273b9176498fcca700550`.
- Candidate: `9ffdf753ab11ce9b638c906388d3690a6aede249`.
- Reviewed the complete cumulative range, including all changed runtime owners, tests, configuration, SDK testing projections, error projections, and documentation. No other discovery report was read.
- Authority: original TASK-003, the unchanged REQ-006 / AC-005 / AC-006 / INV-001–005 obligations, repository AGENTS.md and agent rules, spec-driven development and maintainer guidance, relevant Rust/testing/error/architecture/reliability references, and the verification/outbox portions of Wyrd design, doctrine and Bifrost design.
- The supplied spec file says revision 14 / draft while the task names approved revision 13. The revision history records revision 13's approval and says revision 14 adds a pre-release table-schema rule. This review does **not** approve revision 14. Findings below concern unchanged TASK-003 obligations and do not rely on that added schema rule; final authority reconciliation remains with the orchestrator.
- HEAD was checked at completion and remains the candidate. Only this report was written by this reviewer.

## Acceptance matrix

| Requirement / constraint / non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Finite queued, waiting, retrying and in-flight staging; no dequeue-only release | `outbox.rs`: `Admission`, `reserve`, `send`, `OutboxWriter::{dispatch,finish,release}` retain one charge through retries and terminal consumption. Both reserved and unreserved shares are bounded. | Independently ran the two new bounded-outbox tests; both passed. | PASS |
| Bound Scribe-ACK → submission handoff | `VerificationControl::enqueue` reserves a submission slot and Scribe slot before staging; `ScribeWrite::ExecutionInputs` retains the submission reservation; `ScribeSink::write` transfers it only after successful input delivery. `SubmissionSink::outbox` supplies finite capacity. | Source tracing; implementation records SDK journeys and runtime suites as passing. This reviewer did not rerun those journeys. | PASS |
| Bound ordinary active execution tasks at claims | `RuntimeLimits::execution_slots`, `ClaimLoop::{run,claim_round}`, `VerifierRunner::build` enforce the cap before spawning; Operator consumer retains its permits and receives its existing global bound. | New `the_runner_claims_no_more_than_its_execution_slots`; source inspected, recorded run only. | PASS |
| Typed retryable overload before source staging, including two-row direct calls | Direct execution reserves two Scribe slots before execution; enqueue reserves both owners before its single source stage. `VerificationStagingFull` is catalog-backed with retry detail; HTTP maps Retry-After and MCP uses the existing error projection. | Three real SDK journeys plus MCP rejection test and HTTP mapper assertion are present; recorded passing, not rerun here. | PASS |
| Keep successful source work; preserve retry identity | Reserved source/result rows remain charged until terminal write/loss; retry backlog remains a distinct identical slice. Runner reserves a result slot before engine execution and defers when unavailable. | Independently reran identical-slice retry regression and bounded retry/in-flight test: passed. | PASS |
| Audit remains non-blocking and countable, one write path | Existing `AuditStage for Outbox` still calls synchronous `stage`; unreserved overflow counts/logs loss without an error result. Span producer retains its narrower permit cap. No alternate audit owner or persistence barrier added. | Independently reran `bounded_audit_staging_remains_nonblocking`: passed. | PASS |
| Preserve shutdown/drain accounting and existing lifecycle contract | `shutdown` adds a second independently swapped counter; concurrent shutdown callers can take inconsistent portions of the same abandoned work. | Existing deadline test passes but uses only unreserved items and one shutdown caller. See INV-REV-001. | FAIL |
| Tenant B completes while A continues offering burst traffic | New journeys hold A's first direct call, synchronously wait for B to complete, then issue A's second call and overload attempts. No A traffic continues while B is waiting. | Existing outbox tenant test and SQL claim-round flood test support the owners, but do not establish this required SDK pressure/progress journey. See INV-REV-002. | FAIL |
| Drain then retry the refused enqueue without duplicate source rows | New SDK journeys retry the refused run's submit after drain and count three accepted input rows in A. Existing enqueue retains idempotency semantics. | Journeys inspected; recorded passing, not rerun here. | PASS |
| Portable Rust owners, no producer Postgres access, no distributed quota or new durable queue | Existing generic outbox, Scribe, submission and claim owners extended. Configuration/test harness bindings project the server-owned bound. | Cumulative diff inspected; no prohibited mechanism found. | PASS |
| Standalone proof independent of TASK-001; grouped integration recorded later | Standalone calls use current verify/submit; no dependency introduced. Task evidence explicitly leaves grouped J-005 open until integration. | Grouped proof is explicitly unrun/open rather than claimed passed. | PASS for this standalone task boundary |

## Producer-to-sink and reuse audit

1. Direct source: authorize → reserve two slots → existing target/engine flow → build both rows → stage both on reservation. Failure before staging drops unused slots. No second admission owner is required.
2. Submitted source: authorize and build bounded input → reserve SubmissionOutbox slot → reserve Scribe slot → source row retains the submission reservation → identical Scribe slice retries → successful ACK stages the submission → existing tenant transaction accepts/replays it. Terminal Scribe consumption drops the unspent reservation rather than creating a queue row without inputs.
3. Queued result: finite claim set → existing verifier/SYSTEM lookup → reserve one result slot → engine → lease-fenced settlement → reserved result stage. Retry, defer, terminal and stale-lease branches drop unused result capacity; deferral preserves the existing queue/attempt semantics.
4. Sibling producers: audit, gateway capture and server spans retain the shared Scribe outbox's unreserved staging path; reservations do not turn audit into a refusal. SubmissionSink is the other real generic Outbox implementation. Its SQL transaction and replay owner remain unchanged.
5. Generic writer: queued tuple carries reserved identity; per-tenant waiting/failed backlogs and in-flight metadata retain reserved counts; terminal writes release both pending and admission counters. Shutdown is the one changed release path that fails to retain a single owner for those paired counters.

The existing Outbox and ClaimLoop are the appropriate extension points. No alternate transport, queue, durable receipt, quota scheduler or new dependency is needed. Neither proposed correction requires replacing these owners.

## Proposed findings

### INV-REV-001 — Concurrent shutdown can split paired abandoned-work counters

- Classification: **REGRESSION / INCORRECT**.
- Obligation: TASK-003 preserves shutdown/loss semantics and correct whole-pending accounting. `Outbox::shutdown` expressly guarantees concurrent calls are supported and only one reports the loss (`outbox.rs:331–333`).
- Changed location: `crates/shared/wyrd-runtime/src/outbox.rs:348–350`; consumer arithmetic is `Admission::release` at lines 438–441.
- Source proof: after the writer stops, caller A can execute `pending.swap(0)` and obtain a positive abandoned count, then be preempted. Caller B executes the same swap and obtains zero, then takes the positive `staged_reserved` count. B calls `release(0, reserved)`; `count - reserved` underflows. This panics with development/test overflow checks. Without those checks it temporarily wraps the unreserved counter instead of preserving the invariant. A subsequently obtains zero reserved items and releases all its abandoned work as unreserved. Independent atomics do not pair these ownership transfers.
- Reachability: two calls on clones of the public `Arc<Outbox<_>>`, with at least one reserved item still hanging/retrying at the shutdown deadline. This is an explicitly supported lifecycle operation, not an internal impossible state. Current server shutdown call sites generally have a single coordinator; I do not claim that a normal single-coordinator server shutdown currently invokes this race.
- Siblings/alternatives checked: ordinary writer completion owns count and reserved count together in its in-flight metadata; `send` failure also releases its known pair. Only shutdown reconstructs that pair through independent competing swaps. The existing `pending.swap` already elects the one loss-reporting caller.
- Observable consequence: supported concurrent shutdown can panic rather than returning its promised `(loss, 0)` results, with inconsistent admission accounting during the interleaving. Existing shutdown tests cannot catch it: they stage only unreserved items and call shutdown once.
- Smallest testable correction: keep shutdown cleanup at Outbox, and let only the existing nonzero `pending.swap` winner consume/release `staged_reserved` and perform loss accounting. Preserve the current close/drain fence and avoid another lifecycle service or counter layer. Add focused reserved-item deadline/concurrent-shutdown coverage proving no panic, one loss report, and released pending/share counts.

### INV-REV-002 — Tenant-progress journeys stop A's offering before waiting for B

- Classification: **MISSING** acceptance proof.
- Obligation: TASK-003 Scenario 3 explicitly requires accepted B work to complete “while A continues offering more calls,” mapped to REQ-006/AC-005; tests must prove progress under pressure, not only coexistence with an idle held call.
- Locations: Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1228–1261`; Python `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:489–503`; TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:449–467`.
- Source proof: all three use the same serial shape: start A1 → wait for its slow judge → submit B → wait for B to finish → flush → start A2 → perform refused attempts. While B is awaiting completion, no further A request is offered. A1 holds only half the reservable capacity; B finishes before the staging-full phase starts. The tests therefore demonstrate free-capacity coexistence and later refusal/recovery, but never continuing-burst tenant progress.
- Existing alternatives checked: `outbox::tests::a_failing_tenant_does_not_delay_another_tenant` proves separate sink tenants; `pg_verification_runtime::a_flooding_tenant_does_not_delay_another_tenants_run` seeds a finite SQL queue before runtime startup and proves claim order. Neither drives continuing producer traffic through the real SDK/server staging and claim path. They are useful supporting regressions and should remain.
- Observable consequence: the explicitly requested cross-boundary pressure/progress behavior has no credible journey proof; a producer-pressure regression affecting B can escape these new tests. This is a coverage finding, not a source-proven claim that the current production claim loop starves B.
- Smallest testable correction: extend the existing three staging-pressure journeys with controlled A requests that continue during the lifetime of already-admitted B work, including the pressure phase, then stop A and prove accepted work drains. Keep finite ordinary request traffic, current server fixtures and existing owner counters; no benchmark, synthetic host stress, new harness or strict fairness guarantee. Continue reconciling accepted/refused responses with source rows and preserve the existing refused-enqueue recovery assertions.

## Verification actually performed

The following command was run by this reviewer against the candidate and selected exactly six tests (62 skipped), all passed:

```sh
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::bounded_audit_staging_remains_nonblocking) | test(=outbox::tests::a_retry_writes_the_identical_failed_slice_without_later_items) | test(=outbox::tests::a_failing_tenant_does_not_delay_another_tenant) | test(=outbox::tests::shutdown_flushes_items_that_recover_before_the_deadline) | test(=outbox::tests::shutdown_counts_items_unwritten_at_the_deadline_as_lost)'
```

`git diff --check ef3a57b07...9ffdf753a` passed. Final `git rev-parse HEAD` matched the candidate. The implementation's appended check/journey results are recorded claims, not independently rerun verification. No raw Red/Green traces or diagnostician reports were supplied, so their existence is not established by this review.

Skipped: SDK/MCP/Postgres reruns, broad lint/codegen suites and live infrastructure checks; source/tests/configuration remained immutable.
Risk: reserved shutdown concurrency and continuing-burst tenant progress remain unproven by the passing focused tests.
