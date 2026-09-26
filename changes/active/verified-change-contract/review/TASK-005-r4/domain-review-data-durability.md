# TASK-005 r4 — data, durability, and concurrency domain review

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; immutable cumulative range `f8811ac5035c3aa165d34c38992f9889b3c9081f..6d93b75825926ac27c9fd7de90879b19d67e39b3`.
- Approved `changes/active/verified-change-contract/spec.md` revision 36, especially REQ-073, REQ-078, REQ-146, and REQ-152; original `tasks/TASK-005-production-drift-verifier.md`; previous r1/r2/r3 verdicts and `review/TASK-005-r3-retry2/TASK-005-R3-baseline-shutdown-drain.md`.
- Governing rules: `AGENTS.md` §§3, 5–6, 9, 11–12; `architecture/agent-rules.md` (TenantConn ownership, RLS, tests and gates); `architecture/references/languages/spec-driven-development.md`; `architecture/wyrd-design.md` and `architecture/bifrost-design.md` for durable evidence and control-plane boundaries. `.codegraph/` is absent.

## Boundary and source coverage

Reviewed the cumulative baseline queue, `wyrd.drift_baselines` migration and Postgres tests, fitter, runtime supervisor and limits, shared permits, runner claim/drain comparison, cooperative Vala fit cancellation, server shutdown budget, and the new Postgres fitter test. Also inspected the cumulative changed-file list for the scheduler, run/result publication, Forge verification namespace, and SDK journeys; their prior closure evidence is present in the task and review packet. The r4 source change is confined to the fitter lifecycle, test-only gate, and its integration test; the other r4 commit records evidence.

| Obligation | Source and evidence | Result |
|---|---|---|
| Exact tenant-pinned baseline and durable lease | `drift_baselines.rs` claims by tenant under `TenantConn`; migration enforces RLS and composite tenant FKs; fenced `complete`, `fail`, `release`; `pg_drift_baselines::expired_leases_are_reclaimed_and_stale_tokens_fenced` | PASS |
| Shared bounded execution | `verification/mod.rs` passes one `VerifierPermits` to fitter and runner; `fitter.rs::pass` holds permit from before claim through settlement; `baseline_fits_share_the_verifier_permits` | PASS |
| Shutdown closes new claims, including a racing claim | `fitter.rs::fit_next` uses biased stop/claim select and checks stop after commit; no fit starts from a post-stop committed claim. The new test only invokes `pass` with stop **already cancelled**; it never holds a claim transaction or commit across stop. | **FAIL: required focused proof absent** |
| Fit admitted before shutdown drains within grace | `fit_within_drain` starts grace on stop and allows completion; new test holds a fit, cancels stop, releases gate, and observes `ready` | PASS |
| Fit past grace stops before release; retries retain identity | `fit_within_drain` cancels and awaits `fit`; `fit_next` calls fenced queue `release`, which refunds the attempt; new test checks `pending, 0`; Vala fitting and decoded Parquet polling check cancellation | PASS, subject to the known noninterruptible quantile sort |
| Timeout, restart, and publication durability | Execution timeout still returns structured fit failure after awaiting cancellation; SQL lease expiry and stale-token tests cover crash recovery; shared runtime tests cover run replay and Scribe acknowledgment. No r4 change to publication. | PASS |

## Proposed finding

### DATA-R4-001 — claim/shutdown race is not exercised by the required fitter test

- **Classification:** MISSING (required verification).
- **Violated obligation:** The r3 remediation acceptance criterion for `FIND-TASK-005-11` expressly requires a controlled Postgres-backed fitter lifecycle test covering “new claims stop at shutdown, including a claim already racing the stop signal.” REQ-146 makes that behavior material.
- **Exact evidence:** `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1875-1937` drives a fit already admitted before stop, a fit held past grace, and then calls `releasing.pass(&stop)` **after** `stop.cancel()` and after the releasing task has exited. There is no fitter claim transaction blocked or commit held across cancellation. The same file's `cancelled_runner_rolls_back_its_blocked_claim` and `claim_committed_after_cancellation_is_released_unexecuted` demonstrate the existing controlled Postgres pattern for the runner, but they do not execute `BaselineFitter::fit_next`.
- **Observable consequence:** The stated focused proof would remain green if a regression admitted a claim whose transaction or commit straddled shutdown; only the simpler already-stopped case is protected. Source inspection supports the new guards, so this is a proof gap, not a claim that the current fitter demonstrably executes a post-shutdown fit.
- **Testable correction:** Extend the existing `pg_verification_runtime` fitter coverage using its existing controlled Postgres claim-race pattern so stop occurs while the fitter claim is in progress, then assert no fit starts and the baseline remains or returns `pending` with no attempt charged. Keep the currently passing in-grace and post-grace cases. Run the exact named test through the Postgres wrapper and the owning server integration lane. No production mechanism or new harness is needed.

## Verification limits and result

The remediation records exit-zero `fmt`, `lints`, focused Postgres test, `test:bifrost:integration:server` (83 tests), `test:wyrd` (2135 tests), Drift journey, and cumulative `git diff --check`; I did not rerun them. They verify the two in-flight fit outcomes and post-cancel refusal, but not a racing fit claim. One quantile sort remains noninterruptible within the 256 MiB decoded budget; the approved lease-expiry recovery covers work that outlives process drain. No new persistent schema or public contract was added in r4.

**Overall: FAIL** — `DATA-R4-001` is a bounded, explicit proof gap in the r3 remediation acceptance criterion; no production data or concurrency defect was established.
