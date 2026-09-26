# TASK-005 round 3 data, durability, and concurrency review

## Subject and boundary

- Base `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate `0ef7208565c69d96c44dfe5316394764678bd4e6`. The checked-out later commit changes only a blocked-review record. `.codegraph/` is absent.
- Reviewed the cumulative Drift baseline registration, exact Card/artifact resolution, PostgreSQL work queue and RLS, shared run/fit permits, shutdown and lease settlement, Bifrost observation reads, result publication, Forge namespace change, and the generic scheduling/dispatch seams the Drift adapter uses. This is a source review; no lane was rerun.
- Authorities: approved `spec.md` revision 36, especially REQ-073, REQ-079–082, REQ-086–087, REQ-097, REQ-115, REQ-146, REQ-152 and AC-012–013; original TASK-005; `AGENTS.md` §§6, 9, 11–12; `architecture/agent-rules.md` (TenantConn, RLS, transaction ownership); `architecture/wyrd-design.md`; `architecture/bifrost-design.md`; and `architecture/wyrd-security-posture.md`. Reviewed the prior r1/r2 data reports and retained FIND-TASK-005-9 closure obligation.

## Source and proof coverage

| Boundary | Source traced | Available proof and limit |
|---|---|---|
| Baseline registration, status, retry and lease | `wyrd-sql/migrations/20260601000032_drift_baselines.sql`; `wyrd-sql/src/queries/drift_baselines.rs`; `wyrd-server/src/components/cards/service.rs`; `wyrd-server/src/verification/fitter.rs` | `pg_drift_baselines.rs` tests atomic registration, PostgreSQL-clock due/retry/expiry, stale-token fencing, ready status and forced-RLS isolation. |
| Fit resource ownership and cancellation | `verification/mod.rs` runtime builder/supervisor; `verification/permits.rs`; `verification/fitter.rs`; `vala-drift/src/baseline/mod.rs`, PSI/SPC fit paths | `pg_verification_runtime.rs::baseline_fits_share_the_verifier_permits` covers shared capacity; the new focused `baseline::cancellation::cancellation_after_fit_starts_stops_inside_the_feature` covers cooperative fit cancellation. It does not exercise the server's shutdown grace. |
| Drift execution, query and results | `verification/drift.rs`, `verification/runner.rs`, `verification/publisher.rs`, `query/scheduled.rs`, `query/service.rs`; Forge namespace and migration | Rust/Python/TypeScript journeys and Bifrost runtime tests cover method results, peer query, table auth, result acknowledgements, and dispatch. Accepted separate-table partial visibility is described in REQ-086–087. |
| Generic schedule, restart and dispatch | `verification/scheduler.rs`, `runner.rs`; `wyrd-sql` run/binding/dispatch queries; `pg_verification_runtime.rs` | Existing shared runtime tests cover claim rollback, late claim refund, expired lease reclaim, restart, scheduler cursor, partial acknowledgements, and dispatch. The new Rust Drift journey covers two Services on one Trigger and manual binding dispatch. |

## Proposed finding

### DATA-R3-001 — VIOLATION: a baseline fit loses its shutdown drain

- **Obligation:** REQ-146 says shutdown stops new claims, allows 30 seconds for work already in flight, then cancels remaining work and releases or expires its fenced lease. It explicitly places baseline fits under the shared execution ceiling. The r2 remediation also describes shutdown drain followed by cancellation.
- **Location and evidence:** `crates/wyrd/wyrd-server/src/verification/mod.rs:180-245,365-373` passes the runtime shutdown token directly to `BaselineFitter::run`, whereas the generic runner owns a separate `abandon` token and waits `limits.drain_grace` at `verification/runner.rs:177-205`. In `verification/fitter.rs:144-155,206-239`, an already claimed fit races its work directly against `stop.cancelled()`; when shutdown fires, it cancels the cooperative fit immediately, awaits its return, and releases the row to `pending`. `BaselineFitter` is constructed without `drain_grace`. The fit cancellation test exercises the lower-level fit loop, not this server lifecycle. No existing code delays cancellation of an in-flight baseline fit for the configured grace.
- **Reachable consequence:** A normal server shutdown during a healthy, nearly finished baseline fit discards that fit immediately, refunds its attempt, and makes it refit after restart. A run admitted through the same runtime receives the configured grace, but baseline work does not. A completion during the 30-second window cannot settle ready or become available to scheduled work.
- **Testable correction:** In the existing `BaselineFitter` owner, stop admission on shutdown and let a claimed fit finish within the existing runtime `drain_grace`; after that grace, cancel the blocking fit cooperatively, await its stop, and settle by its fenced lease. Reuse the runtime's existing limit and token/permit/lease mechanisms. Keep execution timeout, PostgreSQL-clock claim/retry, and the guarantee that blocking work ends before lease release. Do not add a worker or a second scheduler.
- **Focused closure proof:** A controlled server fitter test with a claimed fit that finishes after shutdown starts but within the configured grace must settle `ready`; a fit still active after grace must be cancelled and released/expired under the same fenced identity. Run its exact focused command through the repository Postgres wrapper and the owning runtime/Drift lane.

## Verification limits and result

The recorded r2 `fmt`, `lints`, Vala/Wyrd/Bifrost lanes, focused cancellation and Drift journey tests, and clean diff are credible for their exercised paths. None proves the baseline fitter's shutdown grace; the generic runner's grace test does not cover this separate owner. The 256 MiB decoded budget bounds a noninterruptible single quantile sort, but does not change the drain obligation. Earlier FIND-TASK-005-9's cooperative cancellation is present, while this lifecycle gap remains.

**Overall: FAIL**
