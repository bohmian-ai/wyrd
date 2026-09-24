# TASK-005 task review, round 3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; branch `vcc/task-005`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`.
- Reviewed candidate: `0ef7208565c69d96c44dfe5316394764678bd4e6`.
- Remediation range: `45ebc1dd..0ef72085`. Later commit `d638cc90` contains only the prior blocked review record and is outside this subject. The previously supplied `ff3bc3fc` was unavailable; the user's final range fixes the candidate at `0ef72085`.
- Authority: `changes/active/verified-change-contract/spec.md` revision 36, `tasks/TASK-005-production-drift-verifier.md`, repository rules and architecture, prior r1/r2 reviews, and `review/TASK-005-r2/TASK-005-R2-production-drift-closure.md`.
- The candidate commit remained fixed during both review waves. `.codegraph/` is absent. `git diff --check f8811ac5..0ef72085` passes.

## Acceptance matrix

The full obligation-by-obligation matrix and evidence are in `task-review.md`. The independently validated exception below overrides that review's REQ-146 PASS.

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Drift methods, scorer parity, managed observation window, audited tenant read through query service | Vala scorer, fixed SQL, scoped SYSTEM token, Gate/Oracle path; Rust, Python, TypeScript method journeys and peer/read denial tests | PASS |
| Baseline registration, exact Data artifact, status, retry, and SQL fencing | Card/SQL/fitter path; recorded SQL, server and SDK journeys | PASS |
| Scheduled and manual binding behavior, isolated two-Service results and Operators, direct-run no dispatch | Shared runtime and extended Rust Drift journey; exact focused journey recorded green | PASS |
| Shared fit/run permits and cooperative cancellation after a cancellation request | `VerifierPermits`, `fit_baseline_until`, blocking fit awaited before lease settlement; focused cancellation test recorded green | PASS |
| REQ-146 shutdown: stop claims immediately, drain in-flight fits for 30 seconds, then cancel and settle by fence | `BaselineFitter::fit_next` races active work directly against the shutdown token; the fitter has no drain-grace limit and lacks a claim-boundary stop check. No fitter shutdown lifecycle test covers it. | FAIL — `FIND-TASK-005-11` |
| PostgreSQL coordination clock, durable result/report publication, Forge verification namespace, first-class SDK journeys, non-goals | Cumulative source and recorded SQL, Vala, Wyrd, Bifrost, codegen, boundary and focused checks; no private Drift scheduler or client-owned scorer | PASS |
| Revision 36 task authority and prior FIND-2/9/10 corrections | Active task text aligns with scoped token/query service; extended Rust journey and cooperative fit test | PASS |

## Review waves

| Report | Result | Material proposal |
|---|---|---|
| `task-review.md` | PASS | None |
| `standards-review.md` | PASS | None |
| `domain-review-security-tenancy.md` | PASS | None |
| `domain-review-data-durability.md` | FAIL | `DATA-R3-001` |
| `domain-review-statistics.md` | PASS | None |
| `findings-validation.md` | Complete | `DATA-R3-001` independently REVISED to `FIND-TASK-005-11` |

## Validated finding ledger

### FIND-TASK-005-11 — shutdown skips the baseline fit drain

`VIOLATION` of REQ-146 at `crates/wyrd/wyrd-server/src/verification/fitter.rs:143-255` and runtime limit wiring at `verification/mod.rs:365-373`. Shutdown cancels a claimed fit immediately and releases its fenced lease, so a healthy fit that would finish within the required 30 seconds is discarded. A claim already underway can commit after shutdown and begin work because the fitter does not recheck the stop signal at the claim boundary. Keep this one lifecycle correction in `BaselineFitter`: close claim admission immediately, allow already admitted fits the existing runtime drain grace, then cancel, await, and settle remaining work through the current fenced lease. Preserve timeout, SQL retry/refund, permit ownership, and cooperative fit cancellation. The focused proof must exercise completion within grace, cancellation after grace, and a claim racing shutdown through the server fitter and repository Postgres.

## Prior findings and verification limits

`FIND-TASK-005-2`, `-9`, and `-10` are closed for their original obligations. `-9` established cooperative fit cancellation *after it is requested*; it does not cover when shutdown requests cancellation. Earlier r1 findings remain closed. The recorded post-change `fmt`, `lints`, `test:vala`, `test:wyrd`, nine Bifrost lanes, and named focused tests passed, but none drives the fitter shutdown grace. This review did not rerun those lanes. One quantile sort is not internally interruptible and remains bounded by the decoded-data limit; no separate violation was demonstrated. The Oracle capacity test now waits within its prior five-second bound for permitted temporary-directory cleanup and changes no production behavior.

## Verdict

**FIX_REQUIRED** — one bounded implementation finding remains. Implement `TASK-005-R3-baseline-shutdown-drain.md`, then review the complete cumulative candidate again against the same base.
