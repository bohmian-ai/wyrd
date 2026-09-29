# TASK-005 round 3 finding validation

## Subject and method

Base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate `0ef7208565c69d96c44dfe5316394764678bd4e6`. The later `d638cc90` records a blocked review and is outside this candidate. Authority is the approved specification revision 36, original TASK-005, repository rules, and the previous review and remediation packets. I read all five Wave 1 reports and checked the cumulative source and the proposed finding against the actual ownership and call paths. This is a static review; I did not rerun the recorded green lanes. `.codegraph/` is absent.

## Wave 1 proposal validation

| Source proposal | Decision | Independent evidence |
|---|---|---|
| `DATA-R3-001` | **REVISED** | `REQ-146` explicitly gives in-flight work a 30-second shutdown drain. `VerificationRuntime::Capability::spawn` gives the same shutdown token to `BaselineFitter::run` and `VerifierRunner::run` (`verification/mod.rs:150-156`). The runner stops admission and waits `limits.drain_grace` before cancelling its separate `abandon` token (`runner.rs:165-215`). The fitter passes shutdown through `run` → `pass` → `fit_next`; after a claim, `fit_next` selects on `stop.cancelled()` and cancels its work immediately (`fitter.rs:143-160,165-191,206-255`). Its constructor has no drain-grace input (`fitter.rs:75-123`; `verification/mod.rs:365-373`). The fitter also checks `stop` only before entering `fit_next`, so an in-progress claim can commit after shutdown and then start fitting briefly; the runner's `claim`/`claim_round` close that same admission race (`runner.rs:225-306`). These are two facets of the one fitter shutdown lifecycle boundary. |

No other Wave 1 reviewer proposed a finding. The task review marks REQ-146 PASS, but its cited cancellation test proves cooperative computation stops *after cancellation is requested*; it does not prove the required drain *before* requesting cancellation. The data review's conflict is therefore resolved in favor of the source-reachable shutdown gap.

## Caller and simplification check

`BaselineFitter::new` has one production caller, `VerificationRuntimeBuilder::build`; `run` is spawned only by `Capability::spawn`; `run` calls `pass`, `pass` calls `fit_next`, and `fit_next` alone calls `fit`. I read these complete bodies and the corresponding runner `run`, `claim_round`, and `claim` bodies. The proposed correction stays in this owner and runtime limit wiring. `fit` and `vala_drift::fit_baseline_until` already support cooperative cancellation and need no new abstraction or algorithm. The SQL queue already supplies fenced `complete`, `fail`, and attempt-refunding `release`; preserve those transitions, the shared permit through settlement, and the execution timeout. A new worker, scheduler, clock, or public setting would add no value.

The path is production-reachable: a shutdown token can fire while a claimed baseline is being resolved, read, decoded, or fitted. The `tokio::select!` then chooses cancellation, awaits the fit's cooperative stop, and releases the lease rather than allowing a healthy fit to settle during the configured grace. Current tests cover the lower-level cancellation and shared permits, but none drives this fitter lifecycle through shutdown. This is within the approved task's baseline fitter and shared runtime, not an adjacent lifecycle redesign.

## Final deduplicated ledger

### `FIND-TASK-005-11` — REVISED from `DATA-R3-001`

- **Classification and obligation:** `VIOLATION` of spec `REQ-146`: shutdown stops new claims immediately, lets already admitted work finish for the configured drain grace, then cancels remaining work and releases or expires its fenced lease.
- **Location:** `crates/wyrd/wyrd-server/src/verification/fitter.rs:143-255`; limit wiring at `crates/wyrd/wyrd-server/src/verification/mod.rs:365-373`.
- **Evidence and consequence:** The fitter cancels an in-flight fit as soon as `stop` fires, although the runner uses `drain_grace`; a healthy fit that would finish during the grace is discarded and retried after restart. A claim already underway can also commit after shutdown because `fit_next` does not recheck `stop` at the claim boundary.
- **Decision-complete correction:** In `BaselineFitter`, close admission at the durable claim boundary and give fits admitted before shutdown the existing `RuntimeLimits::drain_grace` to finish and settle. After grace, use its current cancellation token, await the blocking fit's stop, and settle through the same fenced lease. Wire the existing clipped runtime limit into this owner. Preserve execution timeout, SQL attempt/refund behavior, the shared permit, and the guarantee that blocking work ends before release. Reuse the runner's admission/drain pattern without moving runner or Operator lifecycle into the fitter.
- **Focused closure proof:** A controlled Postgres-backed fitter lifecycle check shows a fit already claimed at shutdown completing and settling `ready` within grace; a fit still running after grace is cancelled, awaited, and released or expired under its original fence; a claim racing shutdown is not executed. Run the exact named test command and the owning Wyrd/Drift lane.

## Prior findings and limits

`FIND-TASK-005-2` is closed by the Rust journey's two-Service shared Trigger checks and manual binding dispatch assertions (`sdks/wyrd-sdk-rust/tests/drift_verification.rs:579-590,1012-1192`); direct runs still assert no dispatch (`:351-352`). `FIND-TASK-005-9` is closed for cooperative cancellation inside PSI/SPC fitting: the server passes the token to `fit_baseline_until` and awaits the blocking task before settlement (`fitter.rs:216-289`); the new shutdown-drain finding concerns *when* cancellation starts. `FIND-TASK-005-10` is closed by the task's revision-36 status and query-service wording. No evidence reopens the earlier findings. The noninterruptible single quantile sort remains bounded by the decoded-data budget, with no demonstrated separate failure. Recorded format, lint, Vala, Wyrd, Bifrost and focused tests passed but do not exercise the fitter's shutdown grace.
