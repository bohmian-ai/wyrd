# TASK-005 round 3 task implementation review

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; candidate: `0ef7208565c69d96c44dfe5316394764678bd4e6`.
- Authority: approved `changes/active/verified-change-contract/spec.md` revision 36, the original `tasks/TASK-005-production-drift-verifier.md`, its round 1 and 2 reviews, and `review/TASK-005-r2/TASK-005-R2-production-drift-closure.md`.
- Read `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/languages/spec-driven-development.md`, the relevant spec sections and prior finding ledger. `.codegraph/` is absent. Inspected the complete cumulative changed-file list and the remediation diff, then traced the task-sensitive source and journey paths. This review did not rerun lanes; command results below are recorded implementation evidence.
- The later `d638cc90` commit contains only the previous blocked review verdict and is outside this immutable candidate. `git diff --check f8811ac5..0ef72085` is clean.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-110; valid method/signal/profile pairs, Statistical condition, existing SPC fields and scoring | `wyrd-spec/src/card/drift.rs`, `card/verifier.rs`; `vala-drift/src/psi`, `spc`, and `custom` | Validation and scorer parity tests recorded in the task; `test:vala` | PASS |
| REQ-072, REQ-073; exact registered Parquet baseline, pending work, bounded fit, retry | `verification/fitter.rs`, `wyrd-sql/src/queries/drift_baselines.rs`, Card registration | `pg_drift_baselines`, Rust/Python/TypeScript baseline journeys; `test:sql`, `test:bifrost` | PASS |
| REQ-074, REQ-134; exact nonblocking Card status and structured fit failure | `components/cards/resolve.rs` baseline status projection | SDK status and failed-fit journeys; `test:wyrd`, `test:bifrost` | PASS |
| REQ-080; tenant/subject/series/window aggregate scoring, insufficient input inconclusive | `verification/drift.rs` fixed SQL via `query::service::stream_query`; Vala aggregate-input scorers | Fixed-SQL unit tests, never-written table and method-edge journeys in three SDKs | PASS |
| REQ-082; direct and binding manual runs share scorer, direct dispatches none | `verification/runner.rs`; `SharedTrigger::assert_manual_binding_run_dispatches`; direct-run `complete` assertion | Focused Rust `drift_methods_fit_score_persist_and_dispatch` journey | PASS |
| REQ-085; report and features, summary-only pre-scoring inconclusive, no result on error | Drift adapter and generic result publisher | Three SDK method-edge journeys; `unscored_drift_publishes_only_the_summary`; runtime retry tests | PASS |
| REQ-113, INV-010, INV-012; existing Vala reports and Bifrost authority, no Drift scheduler | Aggregate entry points reuse existing scorer/report constructors; generic runtime owns claims and dispatch | Vala parity tests, Rust/Python/TypeScript and server journeys | PASS |
| REQ-146; shared execution permits, cancellation and fenced lease settlement | Shared `VerifierPermits`; `BaselineFitter::fit_next` cancels and awaits blocking fit before settlement; `fit_baseline_until` polls during PSI loops and between SPC fit phases | `baseline_fits_share_the_verifier_permits`; focused `baseline::cancellation::cancellation_after_fit_starts_stops_inside_the_feature`; `test:vala`, `test:wyrd` | PASS |
| REQ-152, INV-015; PostgreSQL coordination clock | SQL baseline claim/retry and verifier-run queue | `pg_drift_baselines`, `pg_verifier_runs` | PASS |
| INV-004; unready, missing, invalid and insufficient evidence cannot pass | Readiness gate, inconclusive `Drift(None)`, retry/terminal mapping | Three SDK negative journeys; runtime terminal tests | PASS |
| AC-012; real Rust, Python and TypeScript fit, score, status and result journeys | Three SDK Drift journey files and testing runtime activation | Recorded `test:bifrost` all nine lanes and focused method-edge commands | PASS |
| AC-013; two Services share Trigger, isolated binding runs, scheduled/manual Operator path, direct no dispatch | Rust journey registers one referenced Trigger and two Services with 2 and 1 Operators; `SharedTrigger` makes both due, requires exactly two new runs, distinct binding/result IDs, own subject and Operator sets; manual binding reuses Operator identities with new dispatch IDs; direct helper requires empty dispatches. Generic runtime owns skip, ACK and recovery behavior. | Focused Rust journey; `pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences`; `pg_verification_runtime` delivery/restart tests | PASS |
| AC-020, AC-024, AC-028, AC-033; SQL state, analytical schema, Card status, shared runtime | Baseline SQL/migration, result tables and Forge namespace allowlist, Card status, runtime/permits | `test:sql`, `test:bifrost`, `test:wyrd`, storage matrix and codegen results recorded in task evidence | PASS |
| Revision 36 SYSTEM reader and audited peer-capable query path | Tenant-issued table-scoped read token; verified `Caller`; fixed SQL via query service/Gate to local or peer Oracle | `system_drift_reader_reads_only_the_observation_table`; `drift_runner_without_local_oracle_reads_through_a_peer` | PASS |
| Task Scenario 1–6 and non-goals: no client aggregates, raw scorer reads, user SQL, alternate scheduler, Alert table, fabricated report or replaced SPC contract | Cumulative diff and owning implementation; original task now points at revision 36 mechanics | Static diff review and above journeys | PASS |
| Required focused commands and broader verification | Exact commands listed in r1 and r2 evidence; last code change precedes recorded green lanes | Recorded fmt, lints, `test:vala`, `test:wyrd`, `test:bifrost` 9/9, two new focused tests, and `git diff --check`, all exit 0 | PASS |

## Prior finding closure

- `FIND-TASK-005-2`: The Rust journey now exercises two Services with a shared referenced Trigger and the manual binding Operator path, while retaining the direct-run non-dispatch check. Generic skip, ACK and restart mechanics remain covered by their owning runtime tests.
- `FIND-TASK-005-9`: The server forwards its cancellation token into `vala_drift::fit_baseline_until`, whose PSI loops poll within a feature; SPC polls between collection and limit fitting. The server still awaits blocking completion before fenced lease settlement. The focused test forces cancellation after fitting starts and compares uncancelled output with `fit_baseline`. A single quantile sort is not interruptible; the decoded Arrow budget caps its input at 256 MiB.
- `FIND-TASK-005-10`: The original task now says revision 36 and `status: review`; its operative outcome, ownership, approach, scenario, acceptance and write-set text describes the token and query service. The earlier route remains only as historical r1 evidence.
- Round 1 `FIND-TASK-005-1`, `-3` through `-8` remain closed by the cumulative source and earlier focused evidence. The pre-existing, uncalled `Oracle::query_plan` is outside this task.

## Proposed findings and result

No material task finding. The Oracle capacity test change polls a resource-ownership assertion within its existing five-second bound; it restores the reported green gate without changing product behavior. **PASS.**
