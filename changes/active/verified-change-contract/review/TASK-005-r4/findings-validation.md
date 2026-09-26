# TASK-005 round 4: independent finding validation

## Subject and method

Immutable cumulative range `f8811ac5035c3aa165d34c38992f9889b3c9081f..6d93b75825926ac27c9fd7de90879b19d67e39b3` in `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`. Approved authority is `spec.md` revision 36, the original TASK-005, the r1–r3 verdicts and remediations, `AGENTS.md`, `architecture/agent-rules.md`, and the applicable Wyrd/Bifrost/Drift design and testing references. `.codegraph/` is absent. I read all five Wave 1 reports and checked the proposed paths against the source and cumulative diff. This was a read-only review; recorded green lanes were not rerun.

## Proposal decisions

| Wave 1 proposal | Decision | Source-based reason |
|---|---|---|
| `TASKREV4-001` | **REVISED** | `baseline_fit_drains_within_grace_then_releases` at `pg_verification_runtime.rs:1875-1938` holds only a fit *after* claim commit, then calls `pass` with an already-cancelled token. It never drives the transaction/commit race in `BaselineFitter::fit_next` (`fitter.rs:232-255`). The r3 remediation expressly requires that focused case. Fold into still-open `FIND-TASK-005-11`, rather than minting a new behavioral defect. |
| `DATA-R4-001` | **REVISED**, duplicate | Independently confirms the same missing fitter-specific proof. Existing runner tests at `pg_verification_runtime.rs:1536-1635` exercise both an uncommitted blocked claim and a deferred commit, but their runner path cannot establish the fitter's separate SQL claim path. Deduplicate under `FIND-TASK-005-11`. |
| `REPO-R4-001` | **CONFIRMED** | New ordinary `use` items inside `score_psi_counts` (`psi/mod.rs:171`) and `baseline_artifact` (`pg_verification_runtime.rs:1762-1766,1784-1785`) violate `architecture/agent-rules.md:10`. The `as _` imports are in a non-generic helper, so the narrow exception does not apply. |
| `REPO-R4-002` | **CONFIRMED** | The cumulative diff materially changes the public, fallible `fit_psi_baseline` into a wrapper (`psi/mod.rs:67-73`), but it has no rustdoc or `# Errors`. `AGENTS.md` §16 and `architecture/agent-rules.md:35` require both. The adjacent cancellable function's docs do not document this caller-facing item. |

## Caller, reachability, and scope check

- `VerificationRuntimeBuilder::build` constructs `BaselineFitter` (`verification/mod.rs:365-373`); its `run` calls `pass`, which calls `fit_next`. `fit_next` calls `DriftBaselineQueue::claim` in a tenant transaction, commits, then either releases the late claim or calls `fit_within_drain`. `DriftBaselineQueue::release` refunds the charged attempt. The complete bodies of `run`, `pass`, `fit_next`, `fit_within_drain`, `claim`, `release`, the focused test, and its supporting baseline helpers were inspected. The race is reachable whenever shutdown arrives during a database claim or its commit; the missing proof is an explicit acceptance requirement, not an inferred production defect. The current guards appear correct by source inspection.
- `score_psi_counts` is reached by both `score_psi` and the server's Drift aggregate scorer, as well as tests. Its `DriftMethod` import belongs at module scope, with the existing spec import. `baseline_artifact` has one live Postgres test caller; moving its imports does not alter the fixture or add a helper.
- `fit_psi_baseline` remains a public entry point used by existing Vala tests. Its full body now forwards to `fit_psi_baseline_until`, whose full body returns missing-feature, type, binning, and cancellation errors; the public wrapper supplies a never-cancelled probe. Documentation should describe that wrapper's actual errors without changing either fitting path.
- The r3 fix's `FitGate` holds work after commit, so it cannot prove the claim boundary. Reuse the existing Postgres controlled-race pattern in this same test file. No new production switch, queue, dependency, or test harness is needed. Keep the existing grace, timeout, permit, SQL fence, and result behavior intact.

## Final deduplicated ledger

### FIND-TASK-005-11 — REVISED — MISSING: claim/shutdown race proof

- **Sources:** `TASKREV4-001`, `DATA-R4-001`; prior stable `FIND-TASK-005-11`.
- **Obligation:** REQ-146 and the r3 remediation's focused proof require no new claim across shutdown, including one racing the stop signal.
- **Location and evidence:** `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1875-1938` covers admitted-fit grace, grace expiry, and an already-stopped pass only. `crates/wyrd/wyrd-server/src/verification/fitter.rs:232-255` has separate uncommitted-select and post-commit stop branches, neither reached across shutdown by that test.
- **Consequence:** A regression at the claim boundary could start a fit after shutdown while all recorded focused assertions remain green. No present production misbehavior is established.
- **Smallest correction:** Extend the existing Postgres fitter lifecycle coverage to control a claim across cancellation and assert that no fit starts and the row is retryable with no charged attempt. The existing runner lock/deferred-commit tests offer an in-file pattern; exercise the fitter's own path, including the post-commit release branch, without changing production code unless the test exposes a defect. Preserve the in-grace and expired-grace assertions.
- **Focused closure proof:** Run the exact named Postgres-wrapped fitter test with the transaction/commit race held across stop, then the owning server integration lane; assert both no fit entry and durable `pending, attempts=0` after the claim resolves.

### FIND-TASK-005-12 — CONFIRMED — VIOLATION: function-scoped imports

- **Source:** `REPO-R4-001`.
- **Obligation:** `architecture/agent-rules.md:10` requires `use` items at module top, with only the stated narrow exceptions.
- **Location and evidence:** `crates/vala/vala-drift/src/psi/mod.rs:171`; `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:1762-1766,1784-1785`.
- **Consequence:** These changed modules conceal their dependencies from the prescribed import block; the ordinary test-helper imports and its non-generic `as _` imports do not qualify for the exception.
- **Smallest correction:** Move the imports into their existing module import blocks, combine the Drift method import with the existing `wyrd_spec::card::drift` import, and remove any duplicate imports. No logic change.
- **Focused closure proof:** Source check for function-scoped imports at these sites; `mise run fmt`, `mise run lints`, and affected Vala/server tests.

### FIND-TASK-005-13 — CONFIRMED — VIOLATION: changed PSI fit API lacks rustdoc

- **Source:** `REPO-R4-002`.
- **Obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md:35` require rustdoc for materially modified Rust items and `# Errors` for fallible functions.
- **Location and evidence:** `crates/vala/vala-drift/src/psi/mod.rs:67-73`, public `fit_psi_baseline`, changed by the cumulative diff into the uncancelled wrapper with no rustdoc.
- **Consequence:** Callers and maintainers cannot see its role or error contract at its public entry point; adjacent function documentation does not satisfy the rule.
- **Smallest correction:** Document the existing uncancelled wrapper and its real fitting error conditions under `# Errors`; leave the implementation as is.
- **Focused closure proof:** Read the item-level docs in source, then run `mise run fmt` and the owning Vala lane.

## Outcome

Three retained findings: one incomplete closure of prior `FIND-TASK-005-11` and two new repository-rule violations. No validated proposal requires a new product or architecture decision. Earlier findings `FIND-TASK-005-1` through `-10` are not reopened by these proposals.
