# TASK-005 round 4 task implementation review

## Subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`; cumulative base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate `6d93b75825926ac27c9fd7de90879b19d67e39b3` (checked-out HEAD).
- Authority: approved `spec.md` revision 36, original TASK-005, r1/r2/r3 validated findings and remediation, `AGENTS.md`, `architecture/agent-rules.md`, the spec-driven-development reference, Wyrd design/doctrine, Bifrost design, security posture, and Drift logic. `.codegraph/` is absent.
- Inspected the full base-to-candidate file range, then traced the r3 fitter change through `VerificationRuntime`, `VerifierRunner`, `DriftBaselineQueue`, Vala's cancellable fit, and the Postgres test. `git diff --check` passes. Recorded focused and broader commands in the r3 remediation task report exit zero; this reviewer did not rerun them.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-110/072/073/074/134: valid PSI/SPC/Custom contracts; exact version-pinned Parquet fitting, readiness and status; no Custom fit | `wyrd-spec/src/card/{drift,verifier}.rs`, server Card registration/status, `DriftBaselineQueue`, `BaselineFitter::resolve` | Rust/Python/TypeScript journeys; SQL baseline tests; recorded `test:vala`, `test:sql`, `test:wyrd` | PASS |
| REQ-080/082/085, AC-012: exact bounded subject/window read, PSI/SPC/Custom scoring and canonical result shape; inconclusive is never pass | `verification/drift.rs`, `vala-drift` aggregate entry points, result publisher | Three language method journeys and edge cases, report/result tests; recorded Drift journey lane | PASS |
| Revision 36 security: table-scoped SYSTEM read token, query-service Gate/Oracle read and audit; no manufactured principal/local plan | `wyrd-auth` issuance, `wyrd-auth-verify`, `verification/drift.rs`, `query::{scheduled,service}` | `system_drift_reader_reads_only_the_observation_table`, `drift_runner_without_local_oracle_reads_through_a_peer` | PASS |
| AC-013: due windows, two Services sharing Trigger, separate binding/results/Operators, manual binding dispatch, direct-run isolation | Generic scheduler/runner and Rust SDK Drift journey | `drift_methods_fit_score_persist_and_dispatch`; shared runtime scheduling/ACK/restart tests | PASS |
| REQ-146: shared global/tenant capacity and shutdown admission/drain semantics | `VerificationRuntime` passes one `VerifierPermits` and its `RuntimeLimits` to fitter; `fitter.rs:216-313` rolls back uncommitted claims on stop, releases late commits, allows admitted fit through grace, then cancels/awaits/releases via lease | `baseline_fits_share_the_verifier_permits`; `baseline_fit_drains_within_grace_then_releases` proves completion within grace, grace expiry, and already-stopped pass. It does **not** hold a claim in progress across shutdown as the r3 remediation's focused proof requires. | FAIL (proof gap, TASKREV4-001) |
| REQ-152/INV-015: PostgreSQL owns durable coordination time | `drift_baselines.rs` uses `statement_timestamp()` for due/lease/retry; local timeout/grace uses Tokio timer | SQL integration and server runtime tests | PASS |
| INV-004/010/012: no fabricated pass, Postgres control versus Bifrost evidence, existing formulas/report | `vala-drift` scoring; `verification/drift.rs`; SQL and Bifrost result owners | Method edge tests, canonical result tests, recorded Vala/Bifrost lanes | PASS |
| REQ-146/r2 cancellation and limits: bounded artifact/decoded work, cooperative fit cancellation, no detached work before settlement | `fitter.rs:61-63,335-355,528-553`; `vala_drift::fit_baseline_until`; `fit_within_drain` awaits fit after cancellation | decoded-budget and cancellation focused tests; r3 grace test | PASS |
| AC-020/024/028/033 and regression boundary: real SQL/server seam, namespace publication, client journeys, generated contracts, security checks | migration/Forge namespace, generated schemas, test harness and SDK changes in cumulative diff | recorded `test:bifrost` nine lanes after r2, r3 server integration/Wyrd/Drift lanes, codegen and boundary checks | PASS |
| Task non-goals: no Drift scheduler, alert table, client aggregation, raw-value download, public SPC rewrite, arbitrary SQL, extra worker | Current diff and runtime/Drift ownership | Source inspection; focused and journey tests | PASS |

## Proposed finding

### TASKREV4-001 — MISSING: focused proof for a claim already in progress when shutdown begins

- **Obligation:** REQ-146 requires immediate cessation of new claims; the r3 remediation acceptance/proof explicitly requires a controlled Postgres test of a claim racing shutdown, alongside in-grace completion and post-grace cancellation.
- **Location:** `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1875-1938` (`baseline_fit_drains_within_grace_then_releases`), especially its final `releasing.pass(&stop)` assertion. `crates/wyrd/wyrd-server/src/verification/fitter.rs:235-255` contains the new claim/commit race logic.
- **Evidence:** The test holds work only *after* a claim commits (`FitGate::pass`); its final case calls `pass` with a token that is already cancelled. It never holds an uncommitted claim, cancels while a claim is underway, or exercises a commit completed after shutdown starts. The implementation's `biased` select and post-commit stop check look correct on source inspection, but the stated race proof is absent.
- **Consequence:** A regression at the commit boundary could admit a fit after shutdown while this test and recorded lanes remain green; it is a durable SQL state transition, so a post-stop no-op pass does not establish the racing case.
- **Testable correction:** Extend the existing Postgres fitter lifecycle test with a controlled in-progress claim across shutdown and assert no fit starts and the durable row is left retryable with its attempt refunded, using the existing `BaselineFitter` and SQL queue. Keep the production code unchanged unless that proof exposes a defect; retain the grace and timeout assertions.

## Prior findings and verification limits

Prior `FIND-TASK-005-1` through `-10` remain closed on source and recorded proof: never-written Custom table, complete language edge journeys, unrelated skill edits removed, exact named commands, bounded SPC history, approved scoped token/query path, rustdoc, shared permits, cooperative fit cancellation, and revision-36 task text. `FIND-TASK-005-11` is implemented for the inspected lifecycle paths; the focused claim-race proof remains incomplete. A single quantile sort remains noninterruptible within the decoded-data budget; no separate failure was demonstrated. The r3 record reports passing fmt, lints, server integration (83), Wyrd (2135), Drift journey, focused test, and diff check; these are recorded results, not independent reruns.

## Result

**FAIL** — one bounded proof gap (`TASKREV4-001`); no additional task-behavior defect demonstrated.
