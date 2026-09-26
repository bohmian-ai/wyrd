---
id: TASK-005-R3
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-073, REQ-146]
depends_on: [TASK-005]
parent_task: TASK-005
remediates: [FIND-TASK-005-11]
---

# Close the baseline fitter shutdown drain

## Authority and subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36.
- Original task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`.
- Review: `changes/active/verified-change-contract/review/TASK-005-r3-retry2/findings-validation.md` and `verdict.md`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; reviewed candidate: `0ef7208565c69d96c44dfe5316394764678bd4e6`. The remediation commits in that candidate are `45ebc1dd..0ef72085`; later `d638cc90` is a blocked-review record outside the candidate.

## Diagnosis and required outcome

### FIND-TASK-005-11 — baseline fits skip REQ-146's shutdown grace

REQ-146 requires shutdown to stop new claims immediately, allow 30 seconds for already admitted work, then cancel remaining work and release or expire its fenced lease for retry. Baseline fitting shares the Verifier execution ceiling and is in this runtime. `VerificationRuntimeBuilder` gives `BaselineFitter::run` the runtime shutdown token (`crates/wyrd/wyrd-server/src/verification/mod.rs:180-245,365-373`). The generic runner uses the existing `RuntimeLimits::drain_grace` before it cancels in-flight runs (`verification/runner.rs:165-215`). The fitter has no drain-grace input: after claiming a baseline, `fit_next` selects directly on the shutdown token (`verification/fitter.rs:143-255`), cancels its blocking fit, awaits it, and releases the lease. Its stop check before `fit_next` also leaves a claim already in progress able to commit after shutdown and start fitting briefly.

A healthy fit close to completion is discarded at normal shutdown even when it would finish inside the specified grace, so status stays pending and work repeats after restart. The r2 cancellation test proves that Vala fit computation stops once cancellation is requested; it does not test when the server requests cancellation. The recorded green lanes do not cover this fitter lifecycle. The candidate therefore falls short of REQ-146 despite preserving the lower-level cooperative stop.

Keep the correction in the existing `BaselineFitter` lifecycle. Use the already configured runtime drain grace and shutdown signal: close durable claim admission when shutdown begins; let fits admitted before it complete and settle during grace; after grace, request the existing cooperative cancellation, await the blocking work's stop, and settle through its original fenced lease. Reuse the generic runner's admission/drain ordering as a pattern and the existing fitter/SQL/permit mechanisms as owners. This resolves the lifecycle gap without changing the approved contract.

## Constraints and non-goals

- Preserve the existing execution timeout, shared global/per-tenant Verifier permits, SQL claim/retry/refund behavior, PostgreSQL coordination clock, and stale-token fencing.
- A blocking fit must stop before its lease is released or failed; retain cooperative polling and the 256 MiB decoded-data bound.
- Do not add another worker, scheduler, public setting, process-local work registry, or client-owned fitting behavior.
- Do not change Drift scoring, scoped observation reads, result publication, Operator delivery, or the prior FIND-2/9/10 corrections.

## Acceptance and proof

| Finding | Acceptance criterion | Focused proof |
|---|---|---|
| `FIND-TASK-005-11` | New claims stop at shutdown, including a claim already racing the stop signal. A baseline admitted before shutdown may finish and settle `ready` inside the configured grace. One still active after grace is cancelled, awaited, and released or expired under the same fence. | Add the smallest controlled Postgres-backed server fitter lifecycle test covering all three cases; record and run its exact named `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command under the repository Postgres wrapper. |

Run `mise run fmt`, `mise run lints`, and the narrowest owning Wyrd/Drift runtime lane after the final code change. Record the focused and lane results in this task. Keep the cumulative candidate committed and submit it for a fresh `$wyrd-task-review` against `f8811ac5`.

## Remediation r3 Evidence

Candidate commit `2d92672f` on `vcc/task-005` (remediation range
`93a2e3c9..2d92672f`), same base `f8811ac5`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-005-11` new claims stop at shutdown, including a racing claim | `BaselineFitter::fit_next` (`verification/fitter.rs`) runs the tenant transaction and claim in a `biased` race with `stop`, as `VerifierRunner::claim` does, so an uncommitted claim rolls back. A claim whose commit completes after `stop` is released unfitted, with its attempt refunded. `pass` no longer pre-checks `stop`; it ends when `fit_next` refuses a claim after shutdown | Third case of `baseline_fit_drains_within_grace_then_releases`: `pass` with the stopped token settles 0, the gate sees no new fit, and the released row stays `pending` with 0 attempts | PASS |
| `FIND-TASK-005-11` a fit admitted before shutdown finishes within the grace | `fit_within_drain` races the fit against the execution timeout and against `stop` followed by `RuntimeLimits::drain_grace`, so shutdown alone no longer cancels an admitted fit. `BaselineFitter::new` takes the runtime's `&RuntimeLimits`, the same lease, timeout, grace, and poll values the runner uses | First case: a fit held in flight across `stop` keeps the fitter running (`building`, 1 attempt); once released it settles `ready` and the fitter returns | PASS |
| `FIND-TASK-005-11` a fit still running after the grace is cancelled, awaited, and released | When the grace elapses, the fit's cancellation token is cancelled, the blocking work is awaited, and the claimed lease is released through the existing fenced `DriftBaselineQueue::release` (attempt refunded). The timeout path, permits, decoded budget, and cooperative Vala polling are unchanged | Second case: with a 100 ms grace the held fit is cancelled, the fitter returns, and the row is `pending` with 0 attempts | PASS |

`FitGate` (behind `test-support`, following the `EngineScript` pattern) holds a
fit after its claim commits, so the test can keep a fit in flight across
shutdown without timing guesses. It adds no production worker, setting, or
registry.

Focused command, run alone in this session (1 passed):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=baseline_fit_drains_within_grace_then_releases)'"
```

Lanes run after the final code change, each exit 0: `mise run fmt`,
`mise run lints`, `mise run test:bifrost:integration:server` (83, including
`pg_verification_runtime`), `mise run test:wyrd` (2135),
`mise run test:bifrost:journey:drift`, and `git diff --check f8811ac5..HEAD`.

Non-goals held: no new worker, scheduler, public setting, or process-local
registry. Drift scoring, observation reads, result publication, Operator
delivery, and the FIND-2/9/10 corrections are untouched.
