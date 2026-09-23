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
