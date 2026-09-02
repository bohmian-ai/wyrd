---
id: BIFROST-R3-T00-AUTHORITY
title: Reconcile Bifrost authority to the approved KISS v1 query contract
kind: implementation
mode: DECOMPOSE
status: implemented
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 3
depends_on: []
requirements: [REQ-002, REQ-003, REQ-005, REQ-007, REQ-009]
invariants: [INV-001, INV-005, INV-007, INV-008]
acceptance: [AC-003, AC-004, AC-005, AC-008]
---

# Authority reconciliation

## Outcome and value

The governing Bifrost and operations documents describe the approved revision
3 behavior before any implementation task is eligible for readiness. Operators
and implementers receive one non-contradictory contract: no public distributed
`EXPLAIN` in this delivery, no automatic distributed retry, only the minimal
approved operator baseline, existing classification plus real-exchange
validation, and one aggregate query pool with practical Wyrd-owned bounds.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- Retain the uncommitted aggregate-pool and dependency-owned byte-backpressure
  reconciliations already present in `architecture/bifrost-design.md`,
  `architecture/references/domain/datafusion.md`, and
  `architecture/references/domain/analytical-operations-reliability.md`.
- Delete or revise remaining mandates for one transparent peer-loss retry,
  public typed distributed `EXPLAIN`, exact post-pruning routing facts, and the
  exhaustive windows/subqueries/deduplicating-set family.
- Retain all ingest, Iceberg, Forge, tenant, audit-WAL, peer mTLS, purpose-ticket,
  readiness, and shutdown authority not explicitly changed by revision 3.
- The old three-task revision-3 packet is superseded and preserved under
  `changes/active/bifrost-distributed-analytics-engine/scrap/`; it is evidence,
  not implementation authority.

## Owner, scope, consumers, and non-goals

Owners are `architecture/bifrost-design.md`,
`architecture/operations/reliability-and-recovery.md`, and the focused Bifrost
references under `architecture/references/domain/`. Direct consumers are every
later task in this packet, Bifrost deployment/runbooks, and final review.

Do not change the approved spec, production code, public schemas, dependency
pins, peer-security model, ingest, Scribe, Forge, or Iceberg behavior. Do not
turn deferred behavior into a deprecated compatibility surface.

## Ordered implementation scenarios

### Scenario 1 — One-attempt distributed lifecycle

**Behavior.** Architecture says every selected Analytical query has one attempt;
peer/transport loss after selection is terminal, and a caller may submit a new
logical query. Maps REQ-007, INV-004, INV-008, AC-003, AC-008.

**RED.** Static authority inspection currently finds mandatory retry language in
`architecture/bifrost-design.md`,
`architecture/operations/reliability-and-recovery.md`, and the focused OLAP,
DataFusion, and reliability references. The decisive failure is any normative
statement permitting an automatic successor attempt. Inspect with:

```bash
rg -n "peer-loss retry|peer retry|re-execute|retry epoch|second loss|optional retry" architecture/bifrost-design.md architecture/operations architecture/references/domain
```

**GREEN.** Rewrite those exact authorities to bind one immutable cut, deadline,
cancellation tree, and execution attempt; make peer loss terminal after
Analytical selection; retain retry only where it belongs to unrelated ingest,
catalog, or Forge protocols. Update telemetry wording from retry to terminal
failure without deleting legitimate non-query retry observations.

**REFACTOR.** Keep one canonical statement in `bifrost-design.md`; focused and
operations documents summarize it without creating a second lifecycle contract.

### Scenario 2 — Deferred EXPLAIN and the minimal approved baseline

**Behavior.** The v1 query operation remains the only public execution entry;
public distributed `EXPLAIN` is deferred, and Analytical compatibility is
limited to the approved representative baseline. Maps REQ-001 indirectly via
REQ-002/REQ-003, INV-001, INV-008, AC-002, AC-008.

**RED.** Authority inspection currently finds a required public typed EXPLAIN
surface and broader windows/subqueries/deduplicating-set promises. The proof is
static because the production source must not be changed in this task:

```bash
rg -n "EXPLAIN|partitioned windows|subqueries|deduplicating set|exact, snapshot-bound statistics" architecture/bifrost-design.md architecture/operations architecture/references/domain
```

**GREEN.** Make EXPLAIN explicitly deferred/non-goal for this delivery. Describe
the supported baseline as filtered/projected scans, fixed-width grouped
`COUNT`/`SUM`/`MIN`/`MAX`, multi-input equi-join, streamed network exchange, and
one real spilling DataFusion operator. State that other shapes remain
Interactive or fail safely; do not promise a public plan or exhaustive matrix.

**REFACTOR.** Preserve the general DataFusion and Iceberg references as reusable
guidance; scope the delivery-specific compatibility claim at the Bifrost query
authority rather than deleting valid background material.

### Scenario 3 — Existing-classifier selection and practical containment

**Behavior.** Existing optimized-plan classification nominates candidates;
supported distributed physical planning plus a real exchange selects
Analytical. Operators/exchange share one pool, Wyrd-owned controls are finite,
and dependency-internal queues retain only pinned byte backpressure. Maps
REQ-002, REQ-005, INV-005, INV-007, INV-008, AC-004, AC-008.

**RED.** Static inspection fails while authority requires a new exact routing
facts contract or an exchange child allocation. Inspect with:

```bash
rg -n "exact.*statistics|exchange child|exchange budget|operator.*sublimit|item-count" architecture/bifrost-design.md architecture/operations architecture/references/domain
```

**GREEN.** Complete the already-started aggregate-pool reconciliation, state the
existing classification/physical-plan split, name every Wyrd-owned finite
control, and preserve the honest dependency byte-backpressure exception. Align
operations/readiness language with terminal peer failure and joined cleanup.

**REFACTOR.** Do not duplicate numeric configuration defaults or dependency API
details across documents; keep those in their existing implementation/config
owners.

## Cross-scenario decisions and authority

This task resolves an authority contradiction, not a product choice: revision 3
already records the human-approved decisions. Later tasks depend on the
integrated result and must not implement against the pre-reconciliation prose.

Authority: `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/wyrd-security-posture.md`, the approved spec, and
`architecture/references/languages/spec-driven-development.md`.

## Broader verification

```bash
mise run docs:check
mise run check:design-sync
git diff --check
```

Also inspect the final authority diff and rerun the three scenario `rg` commands;
remaining matches must be either explicit deferral/prohibition or unrelated
Scribe/Forge/catalog retry language.

## Completion evidence

- A line-by-line authority diff tied to the five approved reconciliation bullets.
- Search output classifying every remaining retry/EXPLAIN/operator match.
- Passing docs/design checks and clean diff check.
- Confirmation that existing uncommitted authority edits were preserved and
  that no production/schema/dependency file changed.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if reconciliation would require changing the
approved one-attempt semantics, adding EXPLAIN, broadening the operator promise,
or weakening peer security, tenant isolation, audit, or joined cleanup.

## Execution evidence

Static authority reconciliation only. No production, schema, contract, or
dependency file was changed. Changed owners: `architecture/bifrost-design.md`,
`architecture/operations/reliability-and-recovery.md`,
`architecture/references/domain/datafusion.md`,
`architecture/references/domain/olap-serving.md`,
`architecture/references/domain/analytical-operations-reliability.md`.

Proof is static inspection, not an executable test: these obligations are
documentary authority with no runtime surface.

### Scenario 1 — One-attempt distributed lifecycle

- **RED.** `rg -n "peer-loss retry|peer retry|re-execute|retry epoch|second loss|optional retry" architecture/bifrost-design.md architecture/operations architecture/references/domain`
  returned 8 matches across 5 files, including the mandatory single-retry
  rebuild in `bifrost-design.md`, the "One deterministic peer-loss retry may
  re-execute" bullet and `retry epoch` stage binding in
  `reliability-and-recovery.md`, the deterministic re-execution sentences in
  `datafusion.md` and `olap-serving.md`, and retry telemetry in
  `analytical-operations-reliability.md`.
- **GREEN.** `bifrost-design.md` now binds one immutable cut, deadline,
  cancellation tree, and execution attempt; every post-selection failure class
  is terminal with no successor attempt and no Interactive rerun. Frame
  identity moved from "attempt epoch" to owning attempt identity.
  `reliability-and-recovery.md` replaces the retry bullet and `retry epoch`
  binding with attempt identity plus terminal post-selection failure, and
  updates the required Oracle indicator and qualification list from "peer
  retry"/"retry-on-pinned-cut" to terminal peer failure with joined cleanup.
  `datafusion.md` and `olap-serving.md` state one execution attempt per selected
  analytical query; `datafusion.md` ownership prose now says "failure policy"
  rather than "retry", and the work-stealing constraint keys on attempt
  identity. `analytical-operations-reliability.md` measures terminal peer
  failure. Scribe, Forge, catalog, and commit retry language is untouched.
  Re-run of the RED command returns zero matches.
- **REFACTOR.** `bifrost-design.md` holds the single canonical lifecycle
  statement; the operations and focused references summarize it without
  restating a second contract.

### Scenario 2 — Deferred EXPLAIN and the minimal approved baseline

- **RED.** `rg -n "EXPLAIN|partitioned windows|subqueries|deduplicating set|exact, snapshot-bound statistics" ...`
  returned 9 matches across 3 files: a required typed EXPLAIN surface plus a
  `POST /v1/query/explain` public-surface bullet in `bifrost-design.md`, and the
  windows/subqueries/deduplicating-set operator family in `bifrost-design.md`,
  `datafusion.md`, and `olap-serving.md`.
- **GREEN.** `bifrost-design.md` declares the public distributed-plan and
  execution-path `EXPLAIN` surface deferred, removes the
  `POST /v1/query/explain` bullet, adds EXPLAIN to "Bifrost does not provide",
  and describes Analytical as the approved v1 baseline (filtered/projected
  scans, fixed-width grouped `COUNT`/`SUM`/`MIN`/`MAX`, multi-input equi-join,
  streamed exchange, one real spilling operator) with an explicit disclaimer
  against an exhaustive operator matrix. `datafusion.md` and `olap-serving.md`
  scope their analytical-path descriptions to the same baseline.
  No `/v1/query/explain` route or `query_explain` symbol exists in `crates/` or
  `python/`, so the surface removal has no production counterpart to change.
- **REFACTOR.** General DataFusion/Iceberg guidance was retained; only the
  delivery-scoped compatibility claim was narrowed.

### Scenario 3 — Existing-classifier selection and practical containment

- **RED.** `rg -n "exact.*statistics|exchange child|exchange budget|operator.*sublimit|item-count" ...`
  returned 9 matches across 4 files, including the exact snapshot-bound routing
  statistics contract in `bifrost-design.md`, the same routing rule in
  `olap-serving.md`, and a per-worker `exchange child budget` in
  `olap-serving.md`.
- **GREEN.** `bifrost-design.md` routing now reuses the existing optimized-plan
  classification over immutable pinned facts, adds no second optimizer or
  routing-facts subsystem, and makes selection conditional on successful
  distributed physical planning inside the supported baseline with a real
  exchange; pre-selection failure falls back to Interactive with a bounded
  reason. `olap-serving.md` matches that rule and drops the exchange child
  budget in favor of the one aggregate query pool. `datafusion.md` keeps
  statistics guidance but no longer ties routing to exact statistics.
  `bifrost-design.md` and `reliability-and-recovery.md` state that cleanup
  timeout or failure is never reported as a successful release and must surface
  in readiness/shutdown evidence.
- **REFACTOR.** No numeric defaults or dependency API details were duplicated.
  The already-uncommitted aggregate-pool, no-exchange-child, and
  dependency-owned byte-backpressure reconciliations were preserved verbatim in
  all three files.

### Remaining match classification

- `bifrost-design.md:278` — explicit prohibition of the broader operator family.
- `bifrost-design.md:410`, `:637` — explicit EXPLAIN deferral and non-goal.
- `bifrost-design.md:318`, `analytical-operations-reliability.md:48`,
  `datafusion.md:38`, `:42` — retained approved reconciliations (single
  aggregate pool, no predicted exchange child, honest dependency byte
  backpressure).
- `datafusion.md:13` — provider-level snapshot-bound file facts, unrelated to a
  routing-facts subsystem.
- Remaining `retry` matches in these files belong to Scribe ingest, Forge,
  catalog commit, and generic backpressure protocols.

### Broader verification

- `mise run check:design-sync` — FAIL, pre-existing and unrelated. It reports
  `ValaQueryService`, `BifrostTracePayload`, `BifrostLogPayload`,
  `BifrostGenAiPayload`, and `BifrostAgentTracePayload` missing from
  `architecture/wyrd-design.md`. Verified identical failure at `HEAD` with this
  task's edits stashed. `wyrd-design.md` is not an owner of this task.
- `mise run docs:check` — FAIL, pre-existing and unrelated. Generated
  `docs/src/content/docs/api/openapi.md` and `schemas.md` drift from
  `wyrd-spec` running-query routes and schemas, not from any architecture
  document. Verified identical failure at `HEAD` with this task's edits stashed;
  regenerated files were reverted so no generated artifact is left modified.
- `git diff --check` — clean.
- Final diff audit: only the five architecture documents changed. Pre-existing
  unrelated modifications to `.agents/skills/*` and `.claude/skills/*` and the
  untracked `changes/active/` packet were preserved untouched.
