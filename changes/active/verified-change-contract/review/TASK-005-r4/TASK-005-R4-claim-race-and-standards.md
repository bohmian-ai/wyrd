---
id: TASK-005-R4
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-073, REQ-146]
depends_on: [TASK-005]
parent_task: TASK-005
remediates: [FIND-TASK-005-11, FIND-TASK-005-12, FIND-TASK-005-13]
---

# Prove the fitter shutdown claim race and close repository-rule gaps

## Authority and subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36.
- Original task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`.
- Prior remediation: `changes/active/verified-change-contract/review/TASK-005-r3-retry2/TASK-005-R3-baseline-shutdown-drain.md`.
- This review: `changes/active/verified-change-contract/review/TASK-005-r4/findings-validation.md` and `verdict.md`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; reviewed candidate: `6d93b75825926ac27c9fd7de90879b19d67e39b3`; latest remediation range `93a2e3c9..6d93b758`.

## Diagnoses and required outcomes

### FIND-TASK-005-11 — required claim-race proof is still missing

REQ-146 requires shutdown to stop new claims immediately. The r3 remediation specifically requires a controlled Postgres-backed fitter test in which a claim races the stop signal, in addition to fit completion within grace and cancellation after grace. The new `baseline_fit_drains_within_grace_then_releases` test at `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1875-1938` covers the latter two and calls `pass` after cancellation, but it never holds the fitter's claim transaction or commit across shutdown. `BaselineFitter::fit_next` at `verification/fitter.rs:232-255` now has separate guards for an uncommitted claim and a claim whose commit completes after stop; neither branch is proven under that race. A future regression could start a fit after shutdown while all current assertions still pass. Source inspection does not establish a present production defect, but the required proof remains incomplete.

Extend the existing Postgres fitter lifecycle coverage with a controlled claim across the stop signal. Reuse the same test file's established transaction/commit race pattern for the generic runner, but drive the fitter's own queue path. Prove that an uncommitted claim does not become executable and that a claim committed after stop is released without fitting and without consuming an attempt. Keep the existing in-grace and past-grace proof. Do not change production code unless this focused proof exposes a real defect.

### FIND-TASK-005-12 — changed functions hide imports

`architecture/agent-rules.md` requires `use` declarations at module top; its narrow `Trait as _` exception applies only to a single generic function where module scope is unsuitable. The cumulative diff adds a `DriftMethod` import inside `score_psi_counts` at `crates/vala/vala-drift/src/psi/mod.rs:171` and ordinary plus trait imports inside `baseline_artifact` at `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1762-1766,1784-1785`. The test helper is not generic. These changed modules therefore violate the mandatory source structure despite green tests.

Move those imports into the existing module import blocks, combining with existing imports and removing duplicates. This changes no runtime behavior or test fixture semantics.

### FIND-TASK-005-13 — changed public PSI fit entry point lacks rustdoc

The cumulative diff materially changes public, fallible `fit_psi_baseline` at `crates/vala/vala-drift/src/psi/mod.rs:67-73` into an uncancelled wrapper over the cancellable fitter. It has no item-level rustdoc or `# Errors`, although `AGENTS.md` §16 and `architecture/agent-rules.md` make both mandatory for materially modified fallible Rust. The adjacent cancellable entry point's documentation does not document this public item.

Add rustdoc stating this entry point performs the ordinary uncancelled fit and documenting its real fitting errors under `# Errors`. Keep its output and implementation path unchanged.

## Constraints and non-goals

- Preserve the r3 shutdown correction: immediate stop of new fit claims, 30-second in-flight grace, cooperative cancellation after grace or timeout, await before fenced release, and attempt refund.
- Preserve shared Verifier permits, PostgreSQL-clock claims/retries, exact baseline identity, 256 MiB artifact and decoded budgets, and the three first-class SDK journeys.
- Do not add a new fitter worker, scheduler, public configuration, test harness, or production branch solely to satisfy a test.
- Do not alter Drift scoring, query authorization/audit, result publication, Operator dispatch, or prior closed findings.

## Acceptance and proof

| Finding | Acceptance criterion | Focused proof |
|---|---|---|
| `FIND-TASK-005-11` | The fitter does not execute a claim whose transaction overlaps shutdown or whose commit finishes after stop; durable state remains retryable and its attempt is not charged. Previously proved within-grace and past-grace behavior remains. | Run the exact named Postgres-wrapped fitter lifecycle test(s) with controlled transaction/commit races, asserting no fit entry and `pending, attempts=0`; then run the owning server integration lane. |
| `FIND-TASK-005-12` | No ordinary `use` remains inside the changed PSI scorer or server test helper. | Read both module import blocks and affected functions; run `mise run fmt`, `mise run lints`, and their owning Vala/server tests. |
| `FIND-TASK-005-13` | `fit_psi_baseline` has item-level rustdoc and a truthful `# Errors` section; fitting output is unchanged. | Read the item documentation and run the owning Vala lane. |

Record exact focused commands for every named test, and record all post-change lane results in this task. Keep the cumulative candidate committed and submit it for a fresh `$wyrd-task-review` against `f8811ac5`.

## Remediation r4 Evidence

Commit `63ddbdf1` on `vcc/task-005` (range `72d205dd..63ddbdf1`). No production code changed.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-005-11` uncommitted claim across stop | `stopped_fitter_rolls_back_its_blocked_claim`: `SHARE` lock on `wyrd.drift_baselines` parks the fitter's claim transaction; `stop` fires; fitter returns; blocker rolls back → `("pending", 0)`, `FitGate::entered() == 0` | Focused command below | PASS |
| `FIND-TASK-005-11` claim committed after stop | `fit_claim_committed_after_shutdown_is_released_unfitted`: deferred constraint trigger holds the claim's `COMMIT` on an advisory lock; `stop` fires, lock released, commit wins → `("pending", 0)`, fenced lease token remains, `FitGate::entered() == 0`. Mutation check: disabling the post-commit guard in `fit_next` fails it with `("failed", 1)` | Focused command below | PASS |
| `FIND-TASK-005-11` in-grace and past-grace | `baseline_fit_drains_within_grace_then_releases` unchanged | Focused command below | PASS |
| `FIND-TASK-005-12` hidden imports | `DriftMethod` joins the `psi/mod.rs` import block; `baseline_artifact` imports moved to the test module's top block. Shared helpers `block_writes`/`wait_blocked_writer` now take the table, reused by the runner and fitter races | `mise run fmt`, `mise run lints`, `mise run test:vala`, `mise run test:bifrost:integration:server` | PASS |
| `FIND-TASK-005-13` `fit_psi_baseline` rustdoc | Item rustdoc plus `# Errors` naming `FeatureMissing`, `FeatureNotNumeric`, `FeatureNotCategorical`, `FeatureEmpty`, `NonFiniteValuesInColumn`, `PsiInternal`; body unchanged | `mise run lints`, `mise run test:vala` | PASS |

Focused commands, each run alone inside
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && <command>"` (1 passed each):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=stopped_fitter_rolls_back_its_blocked_claim)'
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=fit_claim_committed_after_shutdown_is_released_unfitted)'
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=baseline_fit_drains_within_grace_then_releases)'
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=cancelled_runner_rolls_back_its_blocked_claim)'
```

Lanes after the final change, each exit 0: `mise run fmt`, `mise run lints`,
`mise run test:vala` (1308 passed), `mise run test:bifrost:integration:server`
(85 passed), `git diff --check f8811ac5..HEAD`.

Non-goals held: no fitter worker, scheduler, configuration, harness, or
production branch added; r3 shutdown behavior, permits, claims, budgets,
scoring, authorization, publication, and dispatch untouched.
