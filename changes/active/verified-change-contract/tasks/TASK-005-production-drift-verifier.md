---
id: TASK-005
kind: implementation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-072, REQ-073, REQ-074, REQ-080, REQ-082, REQ-085, REQ-110, REQ-113, REQ-134, REQ-152, INV-004, INV-010, INV-012, INV-015, AC-012, AC-013, AC-020, AC-024, AC-028, AC-033]
depends_on: [TASK-004, TASK-010]
---

## Outcome and Value

PSI, SPC, and Custom Drift Verifiers fit or validate their approved baselines,
analyze immutable subject/time windows with server-owned DataFusion plans,
reuse the existing Vala scorers, and persist explainable canonical results.
Users see readiness and can run the same analysis manually or by schedule;
insufficient input is inconclusive, never a pass.

## Owners, Scope, Consumers, and Prohibited Changes

`vala-drift` owns fitting/scoring algorithms and reports. Oracle/DataFusion owns
tenant-authorized aggregate plans. `wyrd-sql` owns baseline work/status;
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
3. Build fixed typed Oracle plans for PSI/SPC/Custom over subject, series, and
   managed event-time windows; return aggregates only.
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

**Behavior.** Fixed Oracle plans filter exact tenant/subject/series/window,
count numeric or categorical fitted bins including unknown categories and zero
bins, and pass counts to existing PSI formula/report construction. Minimum
samples and invalid input are inconclusive.

**RED.** Add known-fixture numeric/categorical aggregate/scoring cases for
boundaries, unknowns, zero bins, subject/tenant/window exclusion, pass/fail,
and insufficient input.

**GREEN.** Build typed DataFusion expressions and a narrow aggregate-count
scoring entry point sharing existing formula code.

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
- Plans use managed `wyrd_event_time`, exact subject, and aggregates only.
- Produced `DriftReport` semantics remain current; no-report inconclusive rows
  have null details and zero features.

## Expected Write Set and Consumer Closure

Likely owners: Drift contracts/tests in `wyrd-spec`, `vala-drift` fit/score
entry points, `wyrd-sql` baseline queries, Data artifact resolution/storage,
Oracle internal logical-plan seam, server fitter/runtime adapter/status, and
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
| Plans use managed `wyrd_event_time`, exact subject, aggregates only | Typed Oracle plans in `drift.rs`; Oracle seam `replace_typed_sources` rebuilds scans against the physical provider schema | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::planner::tests::typed_source_replacement_reconciles_the_physical_timezone_spelling)'`; `mise run test:vala` | PASS |
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
