---
id: TASK-005
kind: implementation
status: review
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-072, REQ-073, REQ-074, REQ-080, REQ-082, REQ-085, REQ-110, REQ-113, REQ-134, REQ-152, INV-004, INV-010, INV-012, INV-015, AC-012, AC-013, AC-020, AC-024, AC-028, AC-033]
depends_on: [TASK-004, TASK-010]
---

## Outcome and Value

PSI, SPC, and Custom Drift Verifiers fit or validate their approved baselines,
analyze immutable subject/time windows with server-built fixed SQL read
through the ordinary query service,
reuse the existing Vala scorers, and persist explainable canonical results.
Users see readiness and can run the same analysis manually or by schedule;
insufficient input is inconclusive, never a pass.

## Owners, Scope, Consumers, and Prohibited Changes

`vala-drift` owns fitting/scoring algorithms and reports. Oracle owns
table-authorized, audited SQL reads reached through the query service and Gate
(local or peer-forwarded); the Drift runner authenticates with the revision 36
SYSTEM read token scoped to `vala.drift.observations`. `wyrd-sql` owns baseline work/status;
`wyrd-server` owns the fitter and Drift adapter inside the generic runtime.
The registry/Data Card storage path supplies exact Parquet artifacts. The
generic runtime owns claims, result transport, settlement, and dispatch.

Revision 32 explicitly preserves the existing SPC public fields and scorer;
the conflicting replacement X-bar/S algorithm in earlier linked prose is not
implementation authority. Do not add client aggregation, raw-value downloads
to the scorer, user SQL, profile MemTables, a Drift scheduler, an Alert table,
or fabricated reports for pre-scoring inconclusive outcomes.

## Approach

1. Validate approved method/signal/profile combinations and create baseline
   status in the registration transaction.
2. Add bounded fitter claims that resolve exact Data Cards and Parquet,
   perform method-appropriate server fitting, and persist existing fitted types.
3. Mint and verify the SYSTEM Drift read token, then run fixed SQL for
   PSI/SPC/Custom over subject, series, and managed event-time windows through
   the query service; return aggregates only.
4. Feed aggregates through narrow entry points sharing existing formulas and
   `DriftReport` construction.
5. Map produced reports and pre-scoring inconclusive outcomes through the
   generic result writer/status/manual/scheduled paths.

## Ordered Implementation Scenarios

### Scenario 1 — Registration creates correct readiness work

**Behavior.** PSI/SPC registration atomically stores a pending baseline tied to
exact Verifier/Data identities and returns without fitting. Custom becomes
ready without a fit row. Invalid pairs, non-Parquet artifacts, incompatible
profiles, and invalid Custom configuration fail before persistence.

**RED.** Add contract/registration/Postgres cases for all valid and invalid
combinations. Current Drift Cards and validation admit incompatible shapes.

**GREEN.** Reuse existing spec validation and add only the approved Verifier
and baseline projection behavior.

**REFACTOR.** Keep pure validation synchronous and remove duplicate server
checks already expressed by typed contracts.

### Scenario 2 — Baseline fitting is durable, exact, and visible

**Behavior.** A bounded fitter claims pending/failed-due work, resolves the
exact Data version and registered Parquet artifact, reads Arrow, calls existing
fitters, and exposes pending/building/ready/failed plus structured errors.
Interrupted/failed work retries through the same row; all Pandas/Polars/Arrow
Data authoring paths produce usable Parquet. PostgreSQL assigns and evaluates
every fitter claim, lease, due time, and retry deadline using its own clock.

**RED.** Add real storage/Postgres/server cases for readiness, each authoring
path, wrong artifact, fit failure, lease expiry, restart, tenant isolation, and
status Card GET.

**GREEN.** Compose current Data artifact resolution, storage, fitters, and
control-row claim pattern.

**REFACTOR.** Keep the fitter the sole implementation-specific background
helper and reuse the generic runtime's permits/supervision.

### Scenario 3 — PSI uses fitted bins and server counts

**Behavior.** Fixed SQL filters exact tenant/subject/series/window,
count numeric or categorical fitted bins including unknown categories and zero
bins, and pass counts to existing PSI formula/report construction. Minimum
samples and invalid input are inconclusive.

**RED.** Add known-fixture numeric/categorical aggregate/scoring cases for
boundaries, unknowns, zero bins, subject/tenant/window exclusion, pass/fail,
and insufficient input.

**GREEN.** Render fitted bins as escaped SQL literals and add a narrow
aggregate-count scoring entry point sharing existing formula code.

**REFACTOR.** Return aggregate rows only and delete any raw-batch or double-
binning path.

### Scenario 4 — SPC preserves the approved existing contract

**Behavior.** SPC+Distribution loads its exact fitted baseline, aggregates the
configured fixed windows/subgroups server-side, and feeds the current approved
SPC scorer/fields. Boundary, ordering, pass/fail, and insufficient-input
behavior remain compatible; SPC+Metric stays rejected.

**RED.** Add regression fixtures for every existing SPC field and scorer
decision plus server filtering/aggregation and empty/partial windows.

**GREEN.** Adapt aggregate output to the existing scorer without changing the
public SPC profile or algorithm.

**REFACTOR.** Remove superseded experimental X-bar/S and fixed-WECO prose/code
if present; preserve only one scorer.

### Scenario 5 — Custom scores the raw-value window mean

**Behavior.** Oracle returns observed/numeric counts and one weighted mean for
the exact metric/window. Equality is no drift; above threshold is drift. Empty,
non-numeric, or nonfinite stored input completes inconclusive with null details
and no feature rows; engine failure retries and writes no result.

**RED.** Add unequal-batch, boundary equality, invalid/empty, exact time-edge,
subject isolation, direct, and binding-created cases.

**GREEN.** Add a narrow aggregate-input entry point sharing existing Custom
formula/report construction.

**REFACTOR.** Do not manufacture Arrow raw batches or evidence wrappers.

### Scenario 6 — Drift results and activation share the generic runtime

**Behavior.** Scored reports create feature rows then summary; pre-scoring
inconclusive sends summary only. Manual direct and binding runs use the same
adapter; direct runs never dispatch. Scheduled runs preserve due windows,
skip inactive/unready/missed occurrences, and dispatch only completed failed
binding results after all ACKs.

**RED.** Add real server/Bifrost journeys for PSI, SPC, Custom, manual/direct,
cron, partial ACK, restart, authorization, and tenant isolation.

**GREEN.** Implement one Drift adapter consumed by TASK-004's closed runtime.

**REFACTOR.** Delete kind-specific scheduling/result/notification mechanics.

## Acceptance Criteria

- All `AC-012` method journeys and `AC-013` Drift activation cases pass.
- Baseline Card status is exact and non-blocking; Custom needs no fit job.
- Fixed SQL uses managed `wyrd_event_time`, exact subject, and aggregates only.
- Produced `DriftReport` semantics remain current; no-report inconclusive rows
  have null details and zero features.

## Expected Write Set and Consumer Closure

Likely owners: Drift contracts/tests in `wyrd-spec`, `vala-drift` fit/score
entry points, `wyrd-sql` baseline queries, Data artifact resolution/storage,
SYSTEM read-token issuance and verification, the query service caller path,
server fitter/runtime adapter/status, and
real SDK/server/Bifrost journey fixtures.

## Verification and Evidence

```bash
mise run test:vala
mise run test:sql
mise run test:wyrd
mise run test:bifrost
mise run test:bifrost:journey:sdk
mise run test:wyrdstate:journey
mise run test:storage:matrix
mise run codegen:check
mise run check:tenant-isolation
mise run fmt
mise run lints
git diff --check
```

Run every new named statistical, SQL, and journey test with its exact focused
command after the implementer fixes its final target and selector.

## Material Stop Conditions

Stop if implementation requires changing the approved SPC public contract or
algorithm, a new baseline artifact format, client-side aggregates, arbitrary
SQL, new result fields, backfill, or different insufficient-input semantics.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/logic/drift.md`
- `changes/active/verified-change-contract/architecture/logic/table_schema.md`
- `architecture/bifrost-design.md`
- `architecture/references/domain/evaluation.md`
- `AGENTS.md`

## Implementation Evidence

Commits `ef2e163f`..`a96bfe25` on `vcc/task-005`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| All `AC-012` method journeys pass | PSI/SPC/Custom engine `wyrd-server/src/verification/drift.rs`; fitter `verification/fitter.rs`; Rust journey `sdks/wyrd-sdk-rust/tests/drift_verification.rs` (Parquet baseline, PSI+SPC ready, SPC-over-string `baseline_fit_failed`, direct runs, result+feature rows joined by `result_id`, unready 409, non-Parquet `WYRD_DRIFT_400_VALIDATION`, reader 403, cross-tenant `INVALID_TARGET`/`TABLE_NOT_FOUND`); Python journey `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py` (Pandas, Polars, Arrow-Parquet baselines; Arrow IPC refused) | `mise run test:bifrost:journey:drift`; focused `pytest -m integration tests/integration/test_drift_journey.py`; `mise run test:bifrost` | PASS |
| `AC-013` Drift activation cases pass | Drift adapter in the generic runtime; scheduled failed result dispatches once per Operator (`assert_scheduled_failure_dispatches`); direct runs never dispatch | `mise run test:bifrost:journey:drift`; `mise run test:wyrd` (`pg_verification_runtime`) | PASS |
| Baseline Card status exact and non-blocking; Custom needs no fit job | Registration creates pending status (`ef2e163f`, `1e5f97e7`); `DriftBaselineStatus` on Card status, TS projection `VerificationStatus.baseline`; journey asserts Custom has no baseline status | Rust and Python journeys; `mise run codegen:check`; `mise run ts:typecheck` | PASS |
| Fixed SQL uses managed `wyrd_event_time`, exact subject, aggregates only | Superseded by remediation r1 (below): fixed server-built SQL in `ObservationWindow` (`drift.rs`), read through the query service; the typed-plan Oracle seam change was reverted (`d6a67597`) | See remediation r1 F6 | PASS |
| `DriftReport` semantics unchanged; no-report inconclusive has null details and zero features | Aggregate-input entry points share existing formulas (`1e5f97e7`); empty-window Custom run asserted inconclusive/`details: None`/no features | Rust Drift journey; `unscored_drift_publishes_only_the_summary` | PASS |

Verification commands, each run in this session and exited 0: `mise run test:vala`,
`mise run test:sql`, `mise run test:wyrd`, `mise run test:bifrost`,
`mise run test:bifrost:journey:sdk`, `mise run test:wyrdstate:journey`,
`mise run test:storage:matrix`, `mise run codegen:check`,
`mise run check:tenant-isolation`, `mise run fmt`, `mise run lints`,
`git diff --check`, `mise run ts:typecheck`, and the focused commands above.

Non-goals remain excluded: no client aggregation, raw-value download, user SQL,
profile MemTable, Drift scheduler, Alert table, fabricated pre-scoring report,
or SPC algorithm change.

CLI fixture baselines: `typed_state/training.yaml` and
`end_to_end_prerequisites/churn-classifier-data.yaml` carry genuine Parquet
artifacts at `data/data.parquet` (100 rows; digests, sizes, and schemas match), and
`churn-classifier-drift` declares `contract_type` categorical. Both CLI journeys
run with the verification runtime and wait for their Drift baselines to fit
`ready` (`wait_baseline_ready` in `card_lifecycle.rs`); the canonical journey now
uses a bound server, because an in-process server that is never bound runs no
background capability. Material limit: the fitter's missing- or invalid-artifact
path (`BASELINE_ARTIFACT_INVALID`) is covered by code but not by a journey.

## Remediation r1 Evidence

Review `review/TASK-005-r1`; commits `5318de41`..`b7b0f3e8` on `vcc/task-005`.
F6 follows spec revision 36 (`02767a4d`). Postgres-backed commands run inside
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && <command>"`.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| F1 never-written Custom table is terminal | `Reader::caller` in `verification/drift.rs`: no `vala.drift.observations` table → no token → inconclusive with no details, features, or dispatch; malformed batches stay terminal `DRIFT_INVALID` | First case of `drift_method_edges_score_through_oracle` (Rust), `test_drift_method_edges_score_through_oracle` (Python), `drift-verification.test.ts` (TS) | PASS |
| F2 journey matrix | Method edges in all three SDK journeys: PSI pass and min-sample inconclusive, categorical, SPC pass with trailing chunk and short-window inconclusive, Custom threshold equality, row-weighted mean, `[start,end)` exclusion on both sides, text metric, never-written table. TS test server gains `verificationRuntime` (`efe0c82b`); TS Drift test joins `test:bifrost:journey:typescript` (`b722a22d`). Oracle-less pod journey `drift_runner_without_local_oracle_reads_through_a_peer`. Activation, delivery, partial ACK, restart, and scheduling skips are generic-runtime behaviour proven by `pg_verification_runtime` (`unacknowledged_summary_retries_with_a_fresh_result`, `crashed_runner_restarts_and_reclaims_without_duplicates`) and `wyrd-sql` `pg_verifier_runs::scheduler_skips_inactive_unready_and_missed_occurrences`; scheduled Drift dispatch in `drift_methods_fit_score_persist_and_dispatch` | Focused commands below; `mise run test:bifrost:journey:drift`, `mise run test:bifrost:journey:typescript`, `mise run test:bifrost` | PASS |
| F3 unrelated skill edits | Reverted (`01e13a86`); `git diff f8811ac5..HEAD -- .agents .claude` is empty | `mise run check:skills-sync` | PASS |
| F4 focused commands | Every named new or changed test is listed below with its exact command | Each ran alone and passed in this session | PASS |
| F5 SPC retains every chunk | `SpcScorer{new,push,finish}` in `vala-drift/src/spc/spc_scorer.rs`; incremental WECO scan capped at the largest rule lookback (`spc/weco.rs`); authored thresholds including 16 preserved | `retained_history_is_bounded_over_many_chunks`, `scan_history_is_capped_at_lookback`, `incremental_scan_matches_reference_on_random_sequences`, `alternating_threshold_sixteen_fires_once` | PASS |
| F6 manufactured SYSTEM authority | `TenantTokenIssuer::issue_system_drift_read_token` mints only `bifrost_query:read` on the observation table UID (`Permission::drift_table_read`); `wyrd-auth-verify` accepts exactly one SYSTEM purpose; Drift verifies the token into a `Caller` and reads fixed SQL with escaped literals through `query::service::stream_query` (Gate → local or forwarded Oracle, audited) via `ScheduledQueryCaller::authenticated`/`run_with`. The hand-built principal, `Option<Oracle>`, the local plan path, and the typed-plan Oracle seam change (`d6a67597`) are deleted | `system_drift_reader_reads_only_the_observation_table` (other tenant refused, results write denied, results read `QueryForbidden`, read and denial audited); `drift_runner_without_local_oracle_reads_through_a_peer` (+1 `bifrost.query.read_decision`); `token_verifier_accepts_system_drift_read_token`; collector `scheduled_sink_folds_batches_and_refusal_ends_the_stream`, `scheduled_terminal_requires_clean_eof`; SQL semantics `psi_numeric_sql_bins_on_fitted_edges_inside_the_window`, `psi_categorical_sql_escapes_labels_and_counts_unknowns`, `spc_sql_orders_chunks_by_creation_then_record`, `custom_sql_is_one_row_and_requires_a_complete_window` | PASS |
| F7 test-module rustdoc | `verification::drift::tests` and every helper documented | `mise run lints` | PASS |
| F8 fits bypass permits | One `Arc<VerifierPermits>` shared by `BaselineFitter` and the runner; permit acquired before claim and held through settlement | `baseline_fits_share_the_verifier_permits` | PASS |
| F9 decoded work unbounded | `decode_bounded` caps decoded Arrow memory at 256 MiB (`BASELINE_ARTIFACT_INVALID`) and checks cancellation; `fit_next` honours timeout and shutdown before lease release | `decode_stops_at_the_decoded_budget`, `cancelled_decode_returns_before_fitting` | PASS |

Found while verifying: Forge's namespace allowlist omitted `vala.verification`,
so every planning hint for the result tables was refused and they were never
compacted (`Forge planning hint persistence failed … invalid Forge table
identity` in the TS lane). Fixed in the Rust allowlist and both Postgres CHECKs
(`20260910000030_forge_verification_namespace.sql`, `d56f1db9`), guarded by
`forge_accepts_every_bifrost_namespace`; the error no longer appears.

Focused commands, each run alone in this session (1 passed):

```bash
# Unit tests: mise exec -- cargo nextest run --locked -p <crate> --lib -E 'test(=<path>)'
vala-bifrost-redux namespaces::tests::forge_accepts_every_bifrost_namespace
vala-drift baseline::aggregate_inputs::custom_mean_equality_is_no_drift
vala-drift baseline::aggregate_inputs::fitted_baselines_round_trip_through_json
vala-drift baseline::aggregate_inputs::psi_categorical_unknowns_join_the_total_only
vala-drift baseline::aggregate_inputs::psi_counts_match_raw_scoring_and_small_windows_are_inconclusive
vala-drift baseline::aggregate_inputs::spc_chunks_match_raw_scoring_and_short_windows_are_inconclusive
vala-drift spc::spc_scorer::alert_threshold_filters_lower_zones
vala-drift spc::spc_scorer::alternating_threshold_sixteen_fires_once
vala-drift spc::spc_scorer::consecutive_run_counts_every_full_window
vala-drift spc::spc_scorer::invalid_inputs_are_rejected
vala-drift spc::spc_scorer::retained_history_is_bounded_over_many_chunks
vala-drift spc::spc_scorer::short_and_missing_features_are_inconclusive
vala-drift spc::spc_scorer::trailing_short_chunk_is_scored
vala-drift spc::spc_scorer::trend_window_fires_once
vala-drift spc::weco::tests::incremental_scan_matches_reference_on_random_sequences
vala-drift spc::weco::tests::scan_history_is_capped_at_lookback
wyrd-auth-verify tests::token_verifier_accepts_system_drift_read_token
wyrd-spec card::drift_validation_tests::rejects_custom_metric_name_outside_the_feature_grammar
wyrd-spec card::drift_validation_tests::rejects_custom_profile_naming_another_metric
wyrd-spec card::drift_validation_tests::rejects_psi_categorical_feature_outside_the_signal
wyrd-server query::scheduled::tests::scheduled_sink_folds_batches_and_refusal_ends_the_stream
wyrd-server query::scheduled::tests::scheduled_terminal_requires_clean_eof
wyrd-server verification::drift::tests::custom_sql_is_one_row_and_requires_a_complete_window
wyrd-server verification::drift::tests::psi_categorical_sql_escapes_labels_and_counts_unknowns
wyrd-server verification::drift::tests::psi_numeric_sql_bins_on_fitted_edges_inside_the_window
wyrd-server verification::drift::tests::spc_sql_orders_chunks_by_creation_then_record
wyrd-server verification::fitter::tests::cancelled_decode_returns_before_fitting
wyrd-server verification::fitter::tests::decode_stops_at_the_decoded_budget

# Postgres-backed, inside the wrapper above
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_grpc_ingest_smoke -E 'test(=system_drift_reader_reads_only_the_observation_table)'
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=baseline_fits_share_the_verifier_permits)'
mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_verification_runtime -E 'test(=unscorable_verifier_errors_without_publishing)'
mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::drift_runner_without_local_oracle_reads_through_a_peer)'
mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test drift_verification -P journey --run-ignored=all -E 'test(=drift_method_edges_score_through_oracle)'
(cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_drift_journey.py -k test_drift_method_edges_score_through_oracle)
(cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/drift-verification.test.ts tests/integration/verification-run.test.ts)
```

Lanes run in this session after the final code change, each exit 0:
`mise run fmt`, `mise run lints`, `mise run test:vala`, `mise run test:sql`,
`mise run test:wyrd`, `mise run test:bifrost` (9/9 lanes, including the drift,
server, SDK, Python, and TypeScript journeys), `mise run test:bifrost:journey:sdk`,
`mise run test:wyrdstate:journey`, `mise run test:storage:matrix`,
`mise run codegen:check`, `mise run check:tenant-isolation`,
`mise run check:client-tier`, `mise run check:skills-sync`, and
`git diff --check f8811ac5..HEAD`. Python format and lints, and
`mise run ts:typecheck`, ran with the journey commits.

Material limits: the read token authorizes the whole tenant observation table;
subject, series, and window confinement come from the server-built SQL, as spec
revision 36 states. Oracle's `query_plan` seam predates this task and is left
untouched.

## Failure Diagnoses

**`test:wyrd` (3 failures).** Symptom: two CLI applies returned
`WYRD_DRIFT_400_VALIDATION` ("baseline Data Card interface Custom is not stored
as Parquet"); `unavailable_engine_errors_without_publishing` expected
`implementation_unavailable`, got `drift_invalid`. Evidence: `card_lifecycle.rs:1263`,
`:1467`; `pg_verification_runtime.rs:669`. Cause: the new registration check
(`cards/resolve.rs` `validate_baselines`) correctly refuses Custom-interface
baselines; the real Drift engine now ships, so an empty script reaches it and the
fixture Verifier has no profile (`drift.rs` "no profile"). Fix site: fixtures only
(`585fdbbd`): Parquet interface plus feature columns; the runtime test is re-pinned
as `unscorable_verifier_errors_without_publishing`. Independent read-only
diagnostician: cause and fix site confirmed; runtime fix CORRECT. It rated the
fixtures INCOMPLETE (no genuine Parquet bytes); remediated with real Parquet
artifacts and fit-to-`ready` assertions in both CLI journeys.

**`test:bifrost` integration:redux, `multi_plan_success_counts_all_committed_volume_once`.**
Symptom: final assertion 25 != 30; passes in isolation. Evidence: the extra-pass
worker settled at `elapsed_ms=503` (the 500 ms sleep, then `shutdown()`);
`support.rs` `shutdown` cancels the worker; `worker.rs` records input volume
only after settlement or recovery. Cause: the test cancelled an attempt whose
catalog commit had removed inputs before its volume was recorded, then compared
consumed files with the counter. Fix site: the test's wait (`8a3070a7`,
`a96bfe25`): wait until no small-files task is ready, retryable, claimed,
running, or prepared and no operation is `prepared`, after clearing backoff;
the equality assertion is unchanged. The shared `shutdown` is not changed, because
other callers rely on cancelling in-flight work. Independent read-only
diagnostician: CORRECT.
