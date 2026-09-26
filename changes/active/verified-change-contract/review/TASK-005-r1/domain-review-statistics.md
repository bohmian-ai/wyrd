# Statistical Drift Domain Review

## Review Findings

### Important

- **STAT-001 — VIOLATION** — [`crates/wyrd/wyrd-server/src/verification/drift.rs:636`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs) collects every Oracle aggregate batch into a `Vec<RecordBatch>`, and [`crates/wyrd/wyrd-server/src/verification/drift.rs:348`](../../../../../crates/wyrd/wyrd-server/src/verification/drift.rs) then appends every SPC subgroup mean to `SpcTargetChunks.means`. This is the reachable production SPC path from `DriftEngine::try_verify` at lines 514–532. It violates `architecture/logic/drift.md:153-163`, which requires SPC aggregate rows to stream through a bounded rule window rather than accumulate an unbounded vector. A large but valid fixed window can therefore make Rust memory and Oracle response volume grow with the number of subgroups and can fail as an engine/query error instead of producing the required SPC judgment. **Required correction:** keep the DataFusion subgroup aggregation and ordering, but consume SPC aggregate batches incrementally through the existing WECO/zone semantics while retaining only the bounded state needed by the authored rule and trend evaluation; do not collect the complete batch stream or complete mean vector. Preserve the current violation count, alert-threshold filtering, trailing-chunk behavior, and `DriftReport` shape. **Closure proof:** a focused test must feed the same ordered subgroup sequence through raw/current and streaming paths and assert identical report fields for zone, alternating, trend, and trailing-chunk cases, plus a production-path test with many subgroup batches that proves completion without retaining all means.

- **STAT-002 — MISSING** — The real server journeys do not prove the required statistical and window boundaries. The Rust journey emits one uniformly drifting window (`sdks/wyrd-sdk-rust/tests/drift_verification.rs:398-409`) and asserts only failed/inconclusive verdicts (`:544-588`); the Python journey repeats the same high-value drift case (`sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py:203-234`). Neither journey executes fitted PSI edge equality, absent/zero bins, minimum-sample pass/inconclusive, SPC ordering/trailing/pass/no-data cases, Custom threshold equality or unequal publication batches, nor exact `[start,end)` exclusion. The plan-only unit test at `crates/wyrd/wyrd-server/src/verification/drift.rs:717-763` checks rendered predicates and column names, not DataFusion results, while aggregate-scorer unit tests construct counts/means directly and bypass Oracle. This falls short of TASK-005 scenarios 3–5 (`TASK-005-production-drift-verifier.md:83-129`) and the mandatory production journeys in `architecture/logic/drift.md:421-440`. Errors in CASE boundary assignment, row ordering/chunk formation, time-edge filtering, or weighted aggregation can remain green. **Required correction:** add focused real SDK→server→Bifrost journeys through the production Oracle path for the listed PSI, SPC, Custom, and exact-window cases. Assert persisted scores, thresholds, feature verdicts, summary verdicts, and null/no-feature behavior—not only terminal status. Reuse the existing journey server and fixtures; no new harness is needed.

### Critical

None.

### Suggestions

None.

## Reviewed Boundary

Immutable subject: base `f8811ac5035c3aa165d34c38992f9889b3c9081f`, candidate `9cfe9b69c5990603e07458aa4402f6750131bfbc` (candidate remained `HEAD` at review completion).

The review traced:

- PSI/SPC baseline artifact resolution, Parquet reading, Arrow fitting, serialization, and exact Verifier/Data identity through `BaselineFitter`, `vala-drift`, and `drift_baselines`.
- PSI numeric `(lower, upper]` and categorical/unknown bin plans, zero-fill/total reconstruction, minimum-sample behavior, and reuse of the existing PSI formula/report constructor.
- SPC baseline row order, frozen adaptive/authored chunk size, runtime `created_at` then `record_id` ordering, subgroup count/mean plans, existing zone/WECO/trend evaluation, trailing chunks, and insufficient-input mapping.
- Custom observed/numeric counts, raw-value `AVG`, finite/all-numeric gating, strict threshold equality, and unscored inconclusive mapping.
- Exact subject/series and managed `wyrd_event_time >= start AND < end` predicates, Oracle execution, fitted-baseline loading, common verdict mapping, canonical report/detail persistence, and Rust/Python journey assertions.

## Authority and Source Coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Exact Parquet baseline fitting | `spec.md` REQ-072/073; TASK-005 scenario 2; `drift.md` baseline and SPC sections | `verification/fitter.rs`; `vala-drift/src/baseline`, `psi`, `spc`; Rust/Python baseline journeys | PASS |
| PSI aggregate semantics | REQ-110, INV-012; TASK-005 scenario 3; `drift.md:183-206` | `ObservationWindow::psi_numeric/psi_categorical`, `psi_counts`, `score_psi_counts`, aggregate-input unit tests | PASS in source; production boundary proof incomplete under STAT-002 |
| SPC contract and scorer preservation | REQ-110, INV-012; TASK-005 scenario 4; `drift.md:208-227` | `ObservationWindow::spc`, `spc_chunks`, `score_spc_chunks`, existing WECO/control-limit code | FAIL under STAT-001; production boundary proof incomplete under STAT-002 |
| Custom weighted mean and equality | TASK-005 scenario 5; `drift.md:229-388`; INV-004 | `ObservationWindow::custom`, `custom_mean`, `score_custom_mean`, result mapping/tests | PASS in source; production boundary proof incomplete under STAT-002 |
| Exact subject/time window and aggregate-only boundary | TASK-005 acceptance; `drift.md:153-181`; AC-012 | `ObservationWindow::series`, Oracle `query_plan`, SDK/server journeys | Predicate shape PASS; edge execution proof incomplete under STAT-002 |
| Report/inconclusive preservation | REQ-085, INV-004/012; TASK-005 scenario 6 | `engines.rs`, `results.rs`, Rust empty-Custom journey, aggregate scorer tests | PASS |

## Verification Limits

- I did not rerun the broad `mise` lanes. The task records successful `test:vala`, `test:sql`, `test:wyrd`, `test:bifrost`, Drift journey, codegen, format, lint, tenancy, storage, and TypeScript checks; this review inspected the relevant source and test assertions rather than treating that summary as acceptance evidence.
- There is no real production-path evidence for the statistical boundary matrix described in STAT-002, so the correctness of those DataFusion executions remains unverified.
- No benchmark or bounded-state proof exists for production SPC aggregation; source inspection directly confirms the unbounded collection in STAT-001.
- The candidate commit did not change during this review.

## Overall Result

**FAIL**

The fitted algorithms and report mapping largely preserve existing semantics, but the production SPC path violates the required bounded streaming contract and the mandatory end-to-end statistical boundary evidence is incomplete.
