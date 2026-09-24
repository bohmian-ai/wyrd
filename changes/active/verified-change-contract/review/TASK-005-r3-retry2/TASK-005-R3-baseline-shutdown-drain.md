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
