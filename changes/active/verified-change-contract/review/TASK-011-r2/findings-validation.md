# TASK-011 r2 independent finding validation

**Subject:** `338f33235f81c30dfe3a570dc26934fe7bb77048..3c6fc1880692cc29c4c1ff7fc5fcb72a850bb9d6`, approved specification revision 38, original TASK-011, prior r1 verdict and TASK-011-R1. `.codegraph/` is absent. This is a source review; I did not rerun the recorded tests. The candidate stayed at `3c6fc188` during this validation.

## Proposal decisions

| Wave 1 proposal | Decision | Source validation and smallest safe boundary |
|---|---|---|
| STAT-R2-1 | **REVISED** | `score_psi_counts` scores each fitted feature independently and then calls `DriftReport::aggregate_verdict`, whose `Drift` precedence can fail a run with another feature below 100. Direct `score_psi` calls the same aggregate scorer after checking its selected rows. Server `DistributionFold::finish` calls it after `ObservationWindow::incomplete`; that SQL counts distinct series but permits duplicate rows for one `(record_id, series)`, so a 99-record window with an extra `x` row can reach 100 `x` and 99 `y` counts. The observations table has no uniqueness fence. REQ-156 requires insufficient complete samples to be inconclusive, and the original task requires incomplete targets to have no scored details or feature rows. In the shared PSI aggregate scorer, validate all feature counts before calculating any PSI; if any is short, return `DriftReport::unscored(Psi)`. In the existing server completeness SQL, require exactly one row per configured series for each selected `record_id` by checking row count as well as series presence, so duplicate rows cannot inflate a complete population. Keep the common verdict aggregator and Custom untouched. This revises the proposal's top-level-only inconclusive correction because that would leave scored feature details and duplicate-inflated samples. |
| S1 | **CONFIRMED** | `ColumnRef::collect_f64` and materially changed `collect_string` in `feature.rs` return `Result` but have no `# Errors` heading; newly added `psi_score::feature` has no rustdoc or `# Panics` for its `expect`. `AGENTS.md` §16 and `architecture/agent-rules.md` require these even for private test helpers. Add the missing intent, error, and panic documentation at those items; inspect other new or materially modified items in the cumulative diff for the same rule. No wrapper or lint suppression is needed. |
| S2 | **REVISED** | The rule at `architecture/agent-rules.md:9` applies to signatures of materially modified items. `fold_spc` still uses the fully qualified `FeatureName` despite an existing import; new `WyrdTestServer::verification_fixture` uses two qualified return types; new TypeScript test helpers `verification_fixture` and `reason` use qualified return/trait types. Import the types at module scope and use bare names in those signatures. Existing unrelated signatures need no churn. |
| S3 | **CONFIRMED** | The cumulative diff changes `wyrd-spec/src/card/mod.rs` tests `rejects_spc_sample_size_below_two` and `rejects_retired_and_unknown_spc_profile_fields`. TASK-011 says the `wyrd-spec` suite passed, but its evidence records no `test:wyrd` lane or exact focused `wyrd-spec` executions. The Bifrost, lint, codegen, and served OpenAPI gates do not execute these two tests. Run the owning `mise run test:wyrd` lane or both exact `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::tests::<name>)'` commands after confirming their real module paths, and record actual results. Repair any failure. |

The task implementation review's `PASS` is contradicted by the reachable PSI count path and by mandatory repository rules. The query/persistence and security reports propose no findings; their boundary checks stand. No Wave 1 proposal was rejected.

## Final deduplicated ledger

### FIND-TASK-011-8 — REVISED; INCORRECT

- **Source:** STAT-R2-1. **Obligation:** REQ-153 minimum PSI target sample, REQ-156 inconclusive insufficient target, and TASK-011 incomplete-target result without scored details or feature rows.
- **Location/evidence:** `crates/vala/vala-drift/src/psi/mod.rs::score_psi_counts` assigns one feature `Inconclusive` below 100 but scores siblings; `report.rs::aggregate_verdict` chooses any `Drift`; `wyrd-server/src/verification/drift.rs::ObservationWindow::incomplete` checks `COUNT(DISTINCT series) < configured`, while per-feature SQL counts physical rows. `vala.drift.observations` is a tall table without a `(record_id, series)` uniqueness fence.
- **Consequence:** One drifting feature with 100 counts and another with 99 can publish a failed result and dispatch Operators; a duplicate configured-series row can create that mismatch from only 99 complete logical observations.
- **Correction:** Use the existing PSI scorer to refuse the whole aggregate before PSI math when any fitted feature has fewer than 100 values, producing the existing empty unscored report. Use the existing completeness query to flag repeated configured-series rows per selected record as incomplete, preserving one statement, one Oracle cut, tenant/window filters, and audit. Do not change the shared verdict aggregator or Custom.
- **Focused proof:** A direct aggregate fixture with one drifting 100-count feature and one 99-count feature returns an empty inconclusive report; a server SQL/fold fixture with 99 complete records and one repeated series row returns `None`, persists inconclusive with null details and zero feature rows, and dispatches no Operator. Keep existing complete PSI and one-feature short-window tests, updating the latter only if its scored-detail expectation conflicts with REQ-156.

### FIND-TASK-011-9 — CONFIRMED; VIOLATION

- **Source:** S1. **Obligation:** `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for changed items, `# Errors` for fallible functions, and `# Panics` when relevant.
- **Location/evidence:** `crates/vala/vala-drift/src/feature.rs::ColumnRef::{collect_f64,collect_string}` lack `# Errors`; `psi/mod.rs::psi_score::feature` lacks rustdoc and contains `expect`.
- **Consequence:** The changed code misses a hard repository documentation gate at an Arrow conversion boundary and test helper.
- **Correction/proof:** Add precise rustdoc to these items and check the remaining new/materially modified Rust items in the cumulative diff; `mise run fmt` and `mise run lints` stay green. No new abstraction or test is needed.

### FIND-TASK-011-10 — REVISED; VIOLATION

- **Source:** S2. **Obligation:** `architecture/agent-rules.md:9` requires module-level imports and bare type names in signatures.
- **Location/evidence:** `wyrd-server/src/verification/drift.rs::fold_spc` uses `wyrd_spec::ids::FeatureName`; `wyrd-testing/src/server.rs::WyrdTestServer::verification_fixture` uses qualified fixture/result types; `wyrd-sdk-ts/native-testing/src/lib.rs::{verification_fixture,reason}` uses qualified fixture, `Display`, and `napi::Error` types.
- **Consequence:** Materially changed signatures hide dependencies in path noise and violate the repository's required import style.
- **Correction/proof:** Reuse the existing `FeatureName` import and add top-level imports for the remaining types, then use bare names in these signatures; run `mise run fmt` and `mise run lints`. No behavior or test harness changes.

### FIND-TASK-011-11 — CONFIRMED; VIOLATION

- **Source:** S3. **Obligation:** `AGENTS.md` §11 requires owning tests and reproducible exact commands for specifically named tests.
- **Location/evidence:** Changed `wyrd-spec/src/card/mod.rs` SPC tests; TASK-011 evidence claims a `wyrd-spec` suite but records no owning execution.
- **Consequence:** The contract validation proof is unsupported even though compilation and generated-schema checks pass.
- **Correction/proof:** Run the owning `mise run test:wyrd` or each affected exact `wyrd-spec --lib` nextest test, record commands and exit results in TASK-011, and fix any failing test without weakening a gate.

## Prior findings and recommendation

FIND-TASK-011-1 through FIND-TASK-011-7 remain closed by the cumulative source and recorded focused proof: selected direct rows, whole-run SPC incompleteness, one Oracle statement, served OpenAPI, exact original named tests, task status, and Python/TypeScript scheduled/legacy journeys. No new product, public API, security, concurrency, or persistent-data decision is needed for findings 8–11.

**Recommended verdict: FIX_REQUIRED.** Four bounded findings remain. The simplest closure is one PSI scorer guard, one existing SQL completeness condition, documentation/import edits, and the missing contract-test execution record. The candidate did not satisfy every approved obligation despite the task reviewer’s `PASS`.
