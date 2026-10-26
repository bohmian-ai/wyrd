---
id: TASK-006-R1
kind: remediation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 14
requirements: [REQ-019, REQ-021, INV-001, INV-002, AC-010, AC-014]
depends_on: [TASK-006]
parent_task: TASK-006
parent_scenario: 5
remediates: [S5-INPUT-COPIES, S5-INCONSISTENT-READBACK, S5-PERSISTENCE-COST]
---

# Simplify verification storage and meet the direct-verification limit

## Outcome and value

Remediate Scenario 5 of [TASK-006](TASK-006-l1-revision-and-qualification.md).
Every verification execution uses one input store, one output store, one small
result summary, one persistence owner and one result reader. PostgreSQL
coordinates work; it never stores verification input values. Direct verification
must meet the unchanged overhead p99 limit of **strictly less than 500 µs at
64 clients**, with correct results and no unexpected staging refusals.

Today a submitted input is persisted in Bifrost and PostgreSQL and, at committed
HEAD, again inside its result. Grouped execution copies selected values into
PostgreSQL, while scheduled drift has no equivalent input record. The dirty
revision-13 reader joins inputs and consequently cannot reconstruct executions
that never wrote them. After this task, a result references its execution's
input record and its own output record; full API responses assemble those facts
without persisting them again.

**Scope authority.** The maintainer explicitly approved the revised plumbing
and the three-table proposal in the conversation requesting this remediation.
The finding IDs above identify that diagnosis, not an immutable task-review
verdict. Revision 13 is the existing baseline; its direct-only, two-table storage
wording is superseded within this bounded scope by the contract below. Record
the approved changes in the owning specification and design text before changing
executable behavior. Do not infer approval for additional product behavior.

This is one remediation task. It does not reopen TASK-006's Bifrost, Gateway,
instrumentation, L2 or reference-promotion work.

## What changes and what must stay true

### Three canonical verification tables

Use exactly these tables in the `bifrost.verification` namespace. Reuse existing
execution and result identities; do not introduce independent input/output IDs.
An application/model response supplied for judgment is a verification **input**.
Verification **output** means the Verifier's detailed judgment.

| Table | Identity and authoritative facts | Excluded payloads |
|---|---|---|
| `execution_inputs` | One logical record per execution ID. Exact input declarations for direct/submitted work; a settled-range selection and ordered source references for grouped work; an observation selection for scheduled drift. Retain the accepted activation context required to run queued work. | Materialized copies of source executions' input values or drift observations. |
| `execution_outputs` | One logical record per result ID, correlated with execution ID. One typed Variant containing `requirements` and `steps`, including scores, reasons, compared actual values, citations, check/provenance details, custom data and captured step outputs. | The original evidence map, a second overall verdict, or a whole embedded `VerificationResult`. |
| `results` | Existing result/execution identities, exact Verifier and subject attribution, binding/group/trigger/window metadata, execution status, overall verdict and timing. Execution ID references inputs; result ID references outputs. | `outcome`, input values, requirement breakdowns, step outputs or copied drift feature detail. |

For authored schemas, retain existing `execution_inputs` columns and add a
required `origin` string with the closed values `direct`, `submitted`, `group`,
and `schedule`. Its existing `inputs` Variant remains non-null: supplied origins
store the existing named declaration map; group/schedule origins store the
selector described below. Keep `queue_execution` true only for submitted
activation; background group/schedule preparation owns its existing enqueue
handoff and must not activate twice. `execution_outputs` has required
`result_id`, `execution_id`, `ended_at`, and sensitive Variant `output`, followed
by the standard shared/managed dimensions. `output` contains exactly
`requirements` and `steps`. `results` retains its existing authored columns
except `outcome`; it needs no new pointer UUIDs or embedded projection.

Keep managed tenant, subject/actor, owner, Run and deployment-environment
attribution on the records that need it. Repeated join/attribution identifiers
are not duplicate input payloads. Keep existing daily partitioning and lookup
pruning through the authored table owners; register outputs through the same
built-in catalog and schema machinery. Inputs/activation and outputs remain
sensitive payloads under existing authorization. Removing the summary's Variant
must not grant a new permission or expose detailed payloads through summaries.

For direct/submitted work, the input record retains the exact named
`InputDeclaration` map: JSON by value; File by existing digest/length/format;
Artifact and Trace by their existing declared references. Do not copy external
bytes into these tables or promise repeatable provider responses. Existing Trace
availability retries and Artifact verification continue to apply.

Grouped/drift selectors are internal storage contracts, not new public input
kinds. Their records must identify the execution origin, exact source scope,
half-open range, and fixed selected membership. Ordered group references include
execution ID, the selected result ID when present, source field/slot and null
positions. Drift references identify the selected observation rows and configured
series without copying feature values. Preserve duplicate/missing-series
validation; a selector must not silently deduplicate malformed observations.
Persist membership once and resolve it on execution/retry; do not rediscover a
different population using the same range.

Preserve genuine computed `actual` values and check-produced data in outputs.
The prohibition is against automatically embedding the original evidence map
again, not against preserving required judgment detail.

### PostgreSQL is coordination only

Remove persisted input JSON and materialized grouped arrays from
`wyrd.verifier_executions`, its insertion/claim shapes, submission outbox
payloads after handoff, and group execution snapshots. Keep execution IDs,
leases, attempts, status/error, idempotency coordination, scheduling boundaries,
selected result IDs, snapshot identity and the minimum references required to
coordinate work. Store the authoritative selection itself in Bifrost inputs.
Do not retain a second PostgreSQL selection/payload for runner convenience.

Accepted routes/authority needed to execute are read from the canonical input
activation context. PostgreSQL may retain the existing authority/attribution and
idempotency fields required for authorization, conflict decisions or
coordination; it must not retain another serialized activation envelope or input
map. Do not change permissions, delegation, idempotency scope or conflict codes.

Producer request handlers add no PostgreSQL IO. Keep bounded staging,
acknowledgment-driven batched queue creation, shutdown ordering and existing
documented abrupt-process-loss semantics. This task adds no WAL, repair scan,
activation service, persistent notification ledger or stronger delivery promise.
A runnable accepted execution survives worker restart by resolving its input
record, not by retaining an in-memory or PostgreSQL copy of input values.

### One write path

Extend the existing verification payload builder and Scribe outbox owner. All
modalities use the same input encoding, output encoding and summary encoding.
Input persistence and result persistence are two moments of this one workflow,
not competing storage paths:

1. Direct work executes from request memory and stages its input record and
   output/summary bundle. Queued submission stages its input record first;
   acknowledged background handoff creates coordination state only.
2. Grouped/scheduled preparation records its selector once before the runnable
   execution consumes it. Existing background handoff/queue ownership handles
   this, with stable identities and idempotent retries.
3. The runner resolves the canonical input record, executes the Verifier and
   builds output plus summary once. Replays resend the same sealed bytes and
   identities, rather than minting a different output on transport retry.
4. Submit input before activating work; write output before its summary through
   the existing detail-before-summary ordering. Preserve queued settlement's
   existing non-waiting relationship to Scribe acknowledgment.

Do not claim cross-table visibility is atomic. Capacity accounting reserves all
required rows/bytes before staging; a newly added output record cannot become an
unaccounted allocation or a second pressure refusal after accepting part of a
direct bundle. Reuse logical outbox reservations; three tables do not prescribe
three queue slots or synchronous RPCs.

Encode each stored Variant directly from its typed value exactly once. Reuse
the current `EncodedVariant::from_serialize` work after validating its bounds,
errors and semantic equivalence. No JSON-text serialization/parse round trip,
duplicate result encoding, cloned input map for persistence, benchmark-only
path, new dependency or new execution runtime.

### Grouped and scheduled selection

Keep the existing `group_end.after` debounce, PostgreSQL-clock deadlines,
pending-row claims, internal tenant-scoped read authority, source validation,
process capacity and settlement-order Operator reactions. Preparation reads
only the declared tenant/owner/subject/exact-Verifier sources within an explicit
settled range. It never builds a PostgreSQL evidence snapshot.

**Approved group range: cumulative history through each settled cutoff.** Late
matching evidence schedules another cumulative run, not a disjoint next
interval. Keep `group_id` and the exact declared source scope as the history
boundary; never include another group or advance an exclusion cursor after a
run. For each claimed due item, use its PostgreSQL `due_at` as the exclusive
range end. It is the end of the existing `after` settlement wait, not the
worker's local clock or the later query completion time. Select retained source
rows with `wyrd_ingested_at < range_end`; record the earliest selected source
admission time as the inclusive range start (start equals end for an empty
selection). Both inputs and source result summaries are range-bounded. A source
result arriving beyond the cutoff remains absent/null in that execution even
if its input was selected. Every new cumulative selection starts from retained
group history again, so it can include earlier-admitted evidence that became
visible later and results absent from an earlier selection.

Once a selection is recorded, retries use its recorded range and identities,
even if pending activity later moves a scheduling deadline. A new matching
acknowledgment uses the existing debounce/handoff to schedule the next run;
later visibility without a new acknowledgment retains the existing documented
notification limitation. Do not add a background history-repair scan or
visibility repair loop.
Identical membership, selected result revisions and order remain an enqueue
deduplication hit even when a later cutoff differs; a changed cutoff alone is
not new evidence. State these cumulative semantics in the owner design and
refactor late-evidence proof accordingly.

Settlement means the existing trigger's wait has elapsed and preparation freezes
the membership visible to its bounded consistent selection. It does not mean
that all earlier-admitted writes are globally visible. `wyrd_ingested_at` is an
admission timestamp, not a completeness watermark. A timestamp predicate alone
never replaces fixed membership. Do not add a watermark/snapshot service.

Preserve ordering: declared `order_by` for its source tuple, with current
number/string/missing handling and stable fallback ordering by input
`recorded_at`, then execution ID. Select a source result by the existing
latest-visible ingest/result-ID rule within the range and pin that result ID.
Missing outcome values retain their null positions; a visible result lacking
its required input/output is publication-incomplete, never a fabricated empty
input. Preserve the existing bounded incomplete-selection retry/error policy
and stable error code; refactor it to check all required canonical records.

Every grouped execution has its own reference-based input record, so another
group Verifier can consume it. Resolve the existing declarative source fields
without duplicating earlier values or expanding source permissions. Unchanged
membership/order/source identities retain existing enqueue deduplication.

Scheduled drift retains `[previous cron boundary, due)` on observation
`created_at`, exact subject and configured series, skipped missed occurrences,
and current empty/incomplete-sample judgments. At preparation, freeze selected
observation membership for the execution. Every requirement and retry uses that
same population; no per-requirement population rediscovery. Later observations
do not mutate an already prepared run, and this task adds no drift backfill or
late-data-triggered execution. A window with no selected observations is a real
empty selector, not a missing input row.

References name logical rows in current Bifrost table authority, not object
paths or historical Iceberg snapshot IDs. Forge's existing compaction,
publication and expiry owners must preserve those live logical rows; expiring
a superseded physical snapshot must not break input resolution. Do not add a
row TTL, extend a configured TTL silently, or introduce a retention/pin service.
For the retained lifetime of a dependent execution/result, its selected source
rows must remain available; any configured source deletion horizon must be no
earlier than that lifetime. If an existing configuration cannot satisfy that
relationship, report the exact conflicting policy rather than copy values or
invent a retention setting. Explicit user deletion/reset keeps its existing
semantics and does not promise continued evidence availability. Never silently
substitute a newer result or drop missing rows. Reference membership storage
and resolution obey existing query/output/memory bounds; pressure defers work
rather than truncating the input population.

### One result reader and unchanged public result shape

Extend `VerificationControl`'s existing execution/result read owner. A summary
query reads `results` alone. Full readback uses execution ID to resolve inputs
and selected result ID to resolve outputs, reconstructing the existing
`VerificationResult`/`VerificationOutcome`: overall verdict from summary,
requirements/steps from outputs, evidence from inputs. A single full-detail
Bifrost statement can join the records; do not impose one RPC per table.

Grouped evidence is assembled from its recorded ordered references into the
existing named slot declarations. Scheduled drift preserves the existing public
empty evidence map; its actual observation selection is recorded in inputs.
Do not expose the internal selector as a new wire declaration type.

For queued work, PostgreSQL's selected result ID remains authoritative; never
choose an abandoned attempt because its summary is newer. For a direct
execution with no queue row, resolve only records whose input origin establishes
direct execution and return the existing completed status shape once readable.
No synthetic PostgreSQL execution insertion is needed. Unknown IDs remain
not-found. Preserve authorization, tenant isolation and existing SDK wait
behavior for delayed publication. A partial join must not report a complete
result with missing evidence/output or permanently hide a queued execution.

MCP, HTTP and all SDKs use this same assembly. Preserve public fields, stable
errors, dispatch projection and capture permissions; add no route, alias,
summary API or client-owned workflow.

Move the sole drift feature detail into outputs. Remove the duplicate
`bifrost.drift.result_features` writer/table registration and update its active
SQL/MCP/docs consumers to read outputs. Retain original observations and fitted
baseline ownership. Do not keep a compatibility table/view or duplicate feature
projection. Historical benchmark records and review evidence remain untouched.

### Schema and documentation transition

Keep the established pre-release reset policy: built-in Bifrost definitions
change in place; old-schema development catalogs/object stores require a reset
and continue to fail existing fingerprint checks. No retained-data backfill,
dual write, compatibility alias or upgrade journey. Do not reset the user's
workspace/data as part of implementation without an explicit request.

PostgreSQL migrations remain immutable: add an ordered migration removing
payload columns and revising shape constraints/SQL functions for references.
Fresh migrations must apply and remain idempotent; do not edit applied files.

Update only the affected authority and active explanatory text: benchmark
REQ-021 and parent Scenario 5's references; Wyrd grouped selection, execution
storage/status and scheduled population rules; Bifrost verification table
ownership; the queue spec's conflicting producer-SQL wording; and the
long-horizon spec's superseded PostgreSQL snapshot/two-table contracts.
Update result SQL examples, MCP guidance and verification flow diagrams for
the three tables. Keep the public API unchanged and leave unrelated historic
revision narratives intact.

## Ordered implementation scenarios

### Scenario 1 — Direct execution records each fact once and reads back by ID

**Behavior.** A real SDK direct invocation returns its existing judgment and
evidence. Once published, its returned ID can be read through status despite
having no PostgreSQL queue row. Inputs occur only in inputs; detailed output
only in outputs; the summary contains no payload Variant. Full readback matches
the original response, including captured outputs and permitted actual values.
Missing IDs and cross-tenant access retain their existing errors.

**RED.** Extend the direct journey in V1/V2/V3 and the shared-reader MCP direct
case in V4. Add the planned V5 persistence test for exact row contents and direct
readback. Confirm expected failure is missing three-table/direct lookup behavior.

**GREEN.** Implement canonical table/writer/read assembly and the direct
fallback. Preserve delayed-visibility and pressure behavior. Run focused direct
selectors and V5.

**REFACTOR.** Remove result `outcome` persistence and obsolete direct-only result
reconstruction. Keep the current public result types and one builder/read owner.

### Scenario 2 — Submitted execution resolves one input record across retries

**Behavior.** Submit staging adds no producer SQL. Acknowledged background
acceptance creates coordination without input JSON. A worker restart/claim
resolves the exact declaration and accepted routes/authority from Bifrost.
Transport replay duplicates no logical records; an abandoned attempt's output
cannot replace the queue-selected result. Publication lag defers safely.

**RED.** Extend submitted journeys in V1–V4 and planned V5/V6 storage/lease tests
to expose the PostgreSQL payload dependency and output selection. Retain
idempotency conflict and pressure recovery coverage in their existing owners.

**GREEN.** Replace payload-bearing submission/claim/runner paths with input
references, preserving staging reservations, accepted identity, attempt
accounting, settlement fencing, retry/error behavior and existing loss bounds.
Rerun Scenario 1 and the submitted proofs.

**REFACTOR.** Delete duplicate activation/input conversions and unread
persistence paths. Do not add recovery promises or a second activation system.

### Scenario 3 — Grouped execution resolves a settled selection without copies

**Behavior.** A due group selects only its declared settled range and freezes
ordered input/result identities. Later visibility never mutates that execution;
late activity follows the approved group range policy. Retries and chained
groups resolve the same selected values, null positions and result revisions,
with no PostgreSQL input arrays. Incomplete publication retains bounded retry
and exact error behavior.

**RED.** Refactor grouped journeys in V1–V4, the late-evidence journey in V7 and
group integration proof in V6. Assert boundaries, selected references, stable
ordering, retry identity and one downstream group; do not add a new journey
file or reproduce group engine mathematics in each SDK.

**GREEN.** Replace unbounded all-visible selection and value pinning with the
approved bounded selection and input references. Preserve source scope,
coordination concurrency and verdict-change reactions. Rerun earlier scenarios.

**REFACTOR.** Delete PostgreSQL materialized snapshots, unused digest/value
conversion code and tests whose only contract was that removed storage shape.

### Scenario 4 — Scheduled drift records its selection and reads detailed output

**Behavior.** A scheduled run has a real input selector, including an empty
selector when appropriate. All requirements and retries use its frozen
population. Detailed drift output is stored once and full readback reconstructs
the same public outcome without requiring a supplied-evidence map. No duplicate
feature table/write remains.

**RED.** Extend the scheduled journey in V1–V3, add the one planned scheduled
readback MCP case in V4, and revise V5/V6 drift persistence tests. Keep current
empty-window and multi-requirement negative/regression coverage.

**GREEN.** Reuse scheduled-window and observation-query owners with canonical
selector resolution. Move drift detail to outputs and update all active feature
SQL consumers. Rerun earlier scenarios.

**REFACTOR.** Delete feature-projection storage and summary-only special cases;
retain distinct drift evaluation/completeness checks.

### Scenario 5 — The simpler production path meets the unchanged performance limit

**Behavior.** Normal L1 direct verification at c1/c64 retains exact judgments,
evidence, authentication, tenant isolation, audit and staging/durability work.
c64 overhead p99 is judged strictly below 500 µs using the specification's
median repetition metric. Every repetition retains correct outcomes, no
unexpected refusals and valid measured populations. Report CPU/work and throughput/core
alongside latency. Extra table plumbing is included, not deferred outside the
measurement or bypassed for benchmarks.

**RED.** Parent TASK-006 Scenario 5 and the stored failed records supply
performance RED. Use V8 diagnostics to distinguish input encoding, output
encoding, summary assembly and staging CPU from elapsed waits. Existing elapsed
histograms are not per-write CPU evidence; do not assert the brief's 443 µs
figure or that encoding dominates without measurements.

**GREEN.** Remove measured redundant production work within the agreed owners;
use typed encoding, shared schemas and existing batching. Run V8 under ordinary
unprofiled settings until valid full L1 passes. Retain before/after source,
configuration, operands and records; a dirty diagnostic cannot qualify.

**REFACTOR.** Remove temporary diagnostic instrumentation and dead paths. No
new per-function monitoring, changed fixture, relaxed threshold or reference
promotion. Rerun affected correctness proof after the final production change.

## Acceptance criteria and minimal proof

| Criterion | Required observation and proof |
|---|---|
| S5-INPUT-COPIES | V5/V6 inspect the three canonical records and PostgreSQL coordination: supplied payload once, selectors reference sources, no input JSON/arrays in queue storage. Negative and replay cases preserve exact behavior. |
| S5-INCONSISTENT-READBACK | V1–V4 cover direct, submitted, grouped and actual scheduled drift through real SDK/MCP → server + repository PostgreSQL → caller. Public outcomes reconstruct with correct evidence/output; direct lookup works; V5/V6 cover missing records, tenant separation, visibility lag and selected attempts. |
| Settled selection | V6/V7 prove both half-open boundaries, frozen membership/result revisions, declared ordering, late evidence, chained groups and incomplete publication. Drift uses one selected population across requirements/retries and preserves empty/incomplete outcomes. |
| S5-PERSISTENCE-COST; REQ-019/021; AC-010/014 | V8 valid full normal L1 proves <500 µs c64 overhead p99 by the existing median-repetition rule, with correctness, zero unexpected staging refusals and valid CPU/work accounting in every repetition. No passing claim from smoke/profiled/dirty/partial/INVALID runs. |
| INV-001/002 | Existing authorization, audit, attribution, idempotency, lease, pressure and replay assertions remain; V1–V6 exercise changed seams without bypasses. Audits never block. PostgreSQL computes coordination deadlines. |
| Contract cleanup | V9 and source inspection show exactly three verification tables, no result payload duplication or drift feature projection, immutable new SQL migration, unchanged public wire fields, accurate authority/docs/SQL examples and no compatibility route/view/alias. |

Minimal journey budget is **one existing direct, submitted, grouped and
scheduled story per Rust/Python/TypeScript**, the corresponding existing MCP
cases plus one scheduled MCP case, and one Rust late-evidence regression.
Extend existing stories; do not create a cross-product of mechanisms, languages,
failure injection and storage assertions. Internal invariants belong in the
small existing server integration modules, not SQL inside SDK journeys.

### Test removal and refactoring

Refactor the existing group integration tests that assert PostgreSQL pinned
arrays/all-visible-history selection, the publication-incomplete retry case,
and pending debounce/handoff. Refactor runtime tests that assert
`result_features`-before-summary or summary-only unscored drift into
output-before-summary and canonical empty-output behavior. Preserve their
audit/metrics, replay deduplication and reclaimed-attempt assertions.

Update pressure tests to account for all three records' bytes without assuming
one queue slot per table. Update MCP custom-result SQL to join outputs. Remove
only obsolete assertions, duplicate plumbing cases and now-unused fixtures or
helpers after naming their replacement in task evidence. Do not delete a whole
story merely because its storage assumptions changed; keep distinct errors,
empty/incomplete drift, multi-requirement judgment, permissions, attribution,
Operator reactions and retries. No ignored/weakened tests or widened globs.

## Implementation reference

These are existing workflow owners, not a private helper/file allowlist.

| Capability and inspected owner | Gap and selected extension |
|---|---|
| `components/verification/service.rs::VerificationControl` (`run_direct`, `enqueue`, `get_execution`, `stage_direct`, `read_result`) | Direct lookup requires PostgreSQL; dirty reader requires absent grouped/drift input rows. Extend its shared assembly and origin-aware direct lookup. |
| `verification/results.rs::ResultPayloadBuilder`, `ResultPayload`, `execution_inputs`; `wyrd-queue/src/variant.rs::EncodedVariant::from_serialize` | Existing builder already owns all modalities, schema reuse and sealed payloads. Extend it for outputs/summaries/selectors; remove full-outcome/feature projection. Validate existing dirty encoder rather than add another codec. |
| `scribe_outbox.rs::ScribeWrite`, `ScribeSink`; `verification/submissions.rs` | Existing ordered delivery, byte admission, acknowledged handoff and batched SQL owner suffice. Change record sets and carry input references after handoff; do not duplicate the workflow. |
| `wyrd-sql/src/queries/verifier_executions.rs::VerifierExecutionQueue`, `Submission`, `PinnedGroup`, `GroupSelection`; runner claim/dispatch/settlement | Claims currently return payload JSON and persisted group snapshots discard selected result IDs. Keep coordination and fencing; resolve canonical input/activation/selection at execution. |
| `verification/groups.rs::GroupPoller`; `scheduler.rs` | Current query has no time predicates and pins arrays in SQL. Add the approved settled-range selector through these existing owners, preserving internal read authority and ordering. |
| `verification/drift.rs::ObservationWindow`, observation evaluation; scheduler/runner | Current per-requirement queries can observe different populations. Use the canonical selection across requirements/retries, without copying observations. |
| `vala-bifrost-redux/src/tables/verification/{execution_inputs,results}.rs`, registry in `tables/mod.rs`; `tables/drift/{observations,result_features}.rs` | Add the output owner through existing authored schemas; remove duplicate feature owner/registration and update fingerprint/schema expectations. Keep observation ownership. |
| `wyrd-spec/src/verification/execution.rs`; shared client verification handle; MCP `verification.rs`/`guide.rs`; three SDK story owners | Preserve result wire shape and dispatch projection; change shared server assembly and active SQL examples. No new client workflow. |
| Server `pg_verification_{groups,routes,runtime}`; `wyrd-ai` `verify_in_real_time` and `scheduled_drift_alerts_operator`; parallel SDK and MCP stories | Reuse real Postgres/server fixtures and existing stories. Replace storage assumptions at their internal owner and keep public assertions through public SDKs. |

Design updates own the lasting contract; source, tests and this task do not
silently override them. Relevant anchors are Wyrd design Grouped verification,
execution routes/storage and Trigger schedule; Bifrost design timestamp meaning,
query cuts/captures and verification tables; security payload authorization;
the SQL directory's immutable-migration and `TenantConn` rules.

## Verification and evidence

Run Cargo/build commands sequentially. Confirm planned selectors after adding
tests, before RED. Existing named selectors below were inspected; if moved,
record equivalent exact selectors rather than substitute an entire suite.

### V1 — Rust public journeys

Prerequisite: `mise run build:server`. Reuse the existing four tests and extend
their public typed assertions; no internal SQL in these stories.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::agent_answer_passes_its_verifier) | test(=verify_in_real_time::submitted_answer_is_judged_in_the_background_and_resumes_by_id) | test(=verify_in_real_time::grouped_support_resumes) | test(=scheduled_drift_alerts_operator::passing_window_alerts_no_one)'"
```

### V2 — Python public journeys

Prerequisites: `mise run py:setup` and V1's server build.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_agent_answer_passes_its_verifier tests/integration/test_verify_in_real_time.py::test_submitted_answer_is_judged_in_the_background_and_resumes_by_id tests/integration/test_verify_in_real_time.py::test_grouped_support_resumes tests/integration/test_scheduled_drift_alerts_operator.py::test_passing_window_alerts_no_one'
```

### V3 — TypeScript public journeys

Prerequisites: `mise run ts:build`, `mise run ts:build:testing` and V1's server build.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verify-in-real-time.test.ts tests/integration/scheduled-drift-alerts-operator.test.ts -t "^(agent answer passes its verifier|submitted answer is judged in the background and resumes by id|grouped support resumes|passing window alerts no one)$"'
```

### V4 — MCP public journeys

The scheduled selector below is **planned**, in the existing MCP module/target.
Confirm it before RED; the other selectors already exist.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::an_agent_executes_a_verifier_on_inline_inputs) | test(=verification::pg_tests::an_agent_enqueues_a_verifier_and_reads_its_result) | test(=verification::pg_tests::grouped_verification_discover_act_observe) | test(=verification::pg_tests::scheduled_drift_execution_reads_its_inputs_and_output)'"
```

### V5 — Canonical storage and read assembly

One **planned** server integration test, table-driven over execution origins,
asserts record contents/identities, no PostgreSQL payloads, detail read assembly,
direct lookup, replay and partial visibility. Use existing fixtures, not a new
test binary. Keep specific authorization/error coverage at its existing owner.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_runtime::verification_records_have_one_payload_owner)'"
```

### V6 — Affected server integration seams

Run the three existing modules after their assertions are refactored. These
are affected seam tests, not a full user-journey suite. During RED use the exact
individual selector for the case being changed and append that command to the
execution evidence before running it.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(/^pg_verification_groups::/) | test(/^pg_verification_routes::/) | test(/^pg_verification_runtime::/)'"
```

### V7 — Late grouped evidence

Refactor this existing Rust regression for the approved range policy; keep
old/new execution evidence distinct and prove prior inputs remain fixed.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::grouped_executions_include_late_evidence)'"
```

### V8 — Production performance

Use the existing opt-in experiment. Diagnostics may use the documented dirty
mode or profile selection, but cannot supply final acceptance evidence.

```bash
mise run bench:l1:verification -- --profile --steps direct-verify-c64 --allow-dirty
mise run bench:l1:verification
```

Run final acceptance on ordinary telemetry, unprofiled, complete c1/c64 workload,
quiet reference host and clean caller-authorized source. No automatic commit.
Retain records even when FAIL/INVALID. Measure per-write CPU with bounded
diagnostics or controlled comparisons; aggregate CPU and elapsed histograms do
not establish individual encoding costs. Final proof includes every production
record, all required writes and audit; no diagnostic omitted-write build counts.
Reference promotion and two-suite cross-engine qualification remain parent work.

### V9 — Schema, migration, contracts and final diff

```bash
mise run db:migrate:all
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::variant_contract_and_builtin_schemas_are_stable) | test(=tables::tests::every_builtin_declares_its_iceberg_form) | test(=tables::tests::canonical_signal_schemas_round_trip_arrow_parquet_and_iceberg)'
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
mise run fmt
mise run lints
mise exec -- git diff --check
```

Use `mise run py:format`, `mise run py:lints`, `mise run py:typecheck` when
Python changes; `mise run ts:typecheck` when TypeScript changes; and
`mise run docs:check` for affected docsite text. Do not run `gate`, full SDK
journey suites, L2 or unrelated engine benchmarks for this task. Add exact
focused commands for any additional changed named test to the evidence.

Inspect tracked/untracked diffs and active references for obsolete outcome or
feature-table reads, PostgreSQL payloads, duplicate builders and stale diagrams.
Do not alter unrelated dirty Variant/audit work or historical evidence.

## Completion evidence and material stop conditions

Append one acceptance-to-source-and-command table, expected RED/actual GREEN,
before/after performance record paths, test-removal replacement mapping and
non-goal audit. Mark unexecuted proof UNVERIFIED; no fabricated run counts or
claims of five known failing journeys. Independent task review remains required
for completion; this document is not a review verdict.

An implementer owns private decomposition and fixture choices. Stop for a
material decision only if correctness requires a new public API/input kind,
different range/late-data policy, weaker retained-reference guarantees,
retained-data upgrade, new dependency/service, stronger crash-recovery protocol,
changed security/audit/admission semantics or changed performance acceptance.
Never solve these by reintroducing payload copies or weakening tests.

## Review against the approved proposal

Author comparison completed after writing; independent read-only comprehension
review completed against this task, parent TASK-006 and the current spec. The
review corrected an accidental every-repetition latency threshold back to the
existing median rule and clarified logical-reference retention. The maintainer
resolved the remaining group range choice as cumulative history. No production
implementation, runtime proof or task-review PASS is claimed.

| Approved recommendation | Included without added scope |
|---|---|
| Inputs once; PostgreSQL coordination only | Canonical input declarations/selectors; no queue payload or materialized group arrays. |
| Three tables with distinct roles | Inputs per execution, detailed outputs per result, payload-free summary; existing IDs and public response shape. |
| One obvious writer and reader | Extend builder/Scribe and shared service assembly; direct ID lookup and queue-selected attempt preserved. |
| Settled-range grouped/drift reads | Cumulative group ranges, existing cron windows, frozen references and existing ordering; no new trigger, watermark or discovery service. |
| Duplicate drift details removed | Outputs owns details; remove feature projection and update active SQL consumers. |
| Existing pre-release schema policy | New immutable PostgreSQL migration; Bifrost definition reset policy, no upgrade/alias/dual write. |
| Minimal proof and old-test cleanup | Reuse four stories per SDK, existing MCP stories plus scheduled readback, one late-evidence regression and affected internal seams; delete only replaced obsolete proof. |
| Simpler and faster production plumbing | Typed encoding once, shared batching and summary-only queries; unchanged measured-work boundaries and <500 µs median-repetition c64 p99 target. |

Excluded additions: new public API/input kind, independent input/output IDs,
new runtime/dependency, retention service, durable snapshot/watermark, stronger
crash recovery, repair scan, drift backfill, compatibility surface, L2 and
reference promotion. The owner spec/design synchronization above documents
already approved changes; any further material policy change requires the
maintainer rather than an implementation guess.

## Authority links

- [Parent Task 006, Scenario 5](TASK-006-l1-revision-and-qualification.md#scenario-5--verification-meets-its-unchanged-64-client-limit)
- [Benchmark specification](../spec.md), REQ-019/021 and INV-001/002.
- [Spec-driven development](../../../spec-driven-development.md) and
  [implementation execution](../../../implementation-execution.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), Verifier and Trigger.
- [Bifrost design](../../../../architecture/bifrost-design.md), timestamps,
  verification tables, query consistency and retention.
- [Security posture](../../../../architecture/wyrd-security-posture.md).
- [Testing](../../../../TESTING.md) and
  [SQL rules](../../../../crates/wyrd/wyrd-sql/AGENTS.md).
- [Queue specification](../../verification-execution-queue/spec.md) and
  [long-horizon specification](../../long-horizon-verification/spec.md), for
  the explicitly superseded storage contracts and existing pre-release reset.

## Execution evidence

Commits: `2000a51ac` (three-table storage, writer, reader, tests), `55146eb36`
(outcome-only output reads, re-baselined statement counts), `4c3c70517`
(retained-data readers on outputs, docs table names). Unrelated dirty
`vala-bifrost-redux/src/tables/audit/projection.rs` and untracked
`benchmarks/*` records were left untouched. Cargo commands ran sequentially.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| S5-INPUT-COPIES | `verification/inputs.rs` (`InputResolver`, one input record per execution; group/schedule selectors reference sources), `verification/results.rs` and `scribe_outbox.rs` (output then summary), new immutable `wyrd-sql` migration dropping queue `inputs`/`route_overrides`/`group_sources` | V5 `pg_verification_runtime::verification_records_have_one_payload_owner` (record per origin, outputs↔summaries by result ID, `information_schema` has no payload columns); V6 68/68 | PASS |
| S5-INCONSISTENT-READBACK | `components/verification/service.rs` (one joined summary⋈outputs⋈inputs read; direct lookup by input origin; RBAC refusal → not found) | V1 4/4, V2 4/4, V3 4/4, V4 4/4 (incl. new `scheduled_drift_execution_reads_its_inputs_and_output`); V5; wyrd-testing `verification_runtime::` 29/29 | PASS |
| Settled selection | `verification/groups.rs` (`wyrd_ingested_at < due_at`, SQL `overdue`), `scheduler.rs` (frozen drift `record_ids` via `DriftEngine::select`), `drift.rs::checks(&ScheduleSelector)` | V6 groups/runtime; V7 `grouped_executions_include_late_evidence`; `wyrd-sql` `pg_verifier_executions` 27/27; `window_lookup_preserves_population` | PASS |
| S5-PERSISTENCE-COST | Variant encoded once; summary-only queries; outputs read only for outcome slots | V8 (below) | see V8 |
| INV-001/002 | Audit/attribution/lease/replay assertions kept; coordination deadlines remain `statement_timestamp()` | V1–V6, `drift_runner_without_local_oracle_reads_through_a_peer` (each peer read audited) | PASS |
| Contract cleanup | `result_features` owner and registration removed; three verification tables; docs name `execution_inputs` | V9: `db:migrate:all` (run as `db:migrate:all:inner` under `with-test-postgres.sh`), 3 table tests, `codegen:check`, `check:deps`, `check:tenant-isolation`, `fmt`, `lints`, `docs:check`, `git diff --check` all exit 0; `git grep result_features` empty outside `changes/` and migrations | PASS |

Additional exact commands run (all PASS after fixes):

- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --lib"` — 575/575.
- `... cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(/verification_runtime::/)'` — 29/29.
- `... cargo nextest run --locked -p wyrd-sql --test integration -E 'test(/pg_verifier_executions/)'` — 27/27.
- `... cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::custom_verdict_data_round_trips)'` — 1/1.
- `... uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_verifier_building_blocks.py::test_custom_verdict_data_survives_readback` — 1/1.
- `... pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t '^custom_verdict_data_survives_readback$'` — 1/1.
- `mise run py:format`, `py:lints`, `py:typecheck`, `ts:typecheck`, `docs:generate` (no generated drift) — exit 0.

### Diagnoses

| Failure | Evidence | Cause | Fix site |
|---|---|---|---|
| Routes: caller without query permission got 403 on GET execution | route trace: direct lookup reached `records.direct()` and returned `PermissionDeniedRbac` | the direct-lookup fallback surfaced the Bifrost read refusal instead of not-found | `components/verification/service.rs::direct_execution` maps the RBAC refusal to `None` (only caller) |
| `pg_verification_groups` incomplete-snapshot retry (line 759) | diagnostician: rows excluded by `wyrd_ingested_at < due_at` after the test moved `due_at` back | test rewound `due_at` past the snapshot's ingest | test-support `Scheduler::with_overdue_groups`; SQL `overdue` covered in `pg_verifier_executions` |
| `pg_verification_groups` debounce (line 860) | diagnostician: the GET poll inside `turn()` consumed the armed query fault | ordering in the test | test reordered: flush and pending assertion before arming the fault |
| Hosted busy-capacity deferral | diagnostician: the spinner's input read raced the adder's admission | concurrency in the test fixture, not production | test holds the sandbox admission itself (`HostedBlocks::sandbox`, test-support) |
| Span-name and write-list assertions | runtime trace shows `bifrost.gate.*`/`bifrost.oracle.query` under attempts; writes `[OUTPUTS, RESULTS]` | runner reads its input record; result writes are output then summary | assertions updated to the designed reads/writes |
| Statement counts (`window_lookup` 3→9, `completed_result` 2→4, peer reads 1→3, group 2→14, readback files 1→3) | diagnostician attributed every statement from profile/trace (`g2.txt`, `c1.txt`): input-record reads (task lines 119/135), group probe and resolution (line 92), one joined readback over three tables (line 239), one file per table with older day partitions pruned | designed reads; one avoidable outputs read in the group order probe | `InputResolver::slots` reads outputs only when a slot names an outcome (group 16→14); assertions re-baselined with per-statement docs |
| MCP/Python/TS `custom_verdict_data` SQL read `results.outcome` | `git grep` for result payload reads | `results` is payload-free | readers query `execution_outputs.output` requirements |

### Test removal and replacement mapping

| Removed or rewritten | Replacement |
|---|---|
| `results.rs` drift feature-before-summary tests | `results.rs::outcome_writes_output_then_summary` |
| summary-only unscored drift test | renamed and asserted as canonical empty-output behavior (`unscored_report`) |
| outbox tests assuming `result_features` | redesigned `scribe_outbox.rs` output-then-summary cases with byte accounting across records |
| PostgreSQL pinned-array/all-history group assertions | `pg_verification_groups` selector/range assertions and V5 record checks |
| incomplete-snapshot test rewinding `due_at` | `with_overdue_groups` pass; `overdue` SQL in `pg_verifier_executions` |
| hosted busy test spinner | held sandbox admission |

### Non-goal audit

No new public API, wire field or input kind; no independent input/output IDs,
new runtime, dependency, retention service, durable snapshot/watermark,
crash-recovery protocol, repair scan, drift backfill, compatibility
route/view/alias, L2 or reference promotion. No `#[allow]`, `#[ignore]`,
widened glob or deleted story. Material risk: a refused oversized output still
lets its summary write (existing outbox semantic, unchanged).

## Revision — bounded memory and concurrent write partitioning

### Outcome, authority and diagnosis

Complete Scenario 5's unfinished performance obligation without undoing the
three-table design. Reduce actual retained allocation, correct demonstrated
shared-buffer overcharging, and distribute independent verification executions
over the existing generic outbox lanes. Preserve durable acknowledgments,
retry identity, tenant isolation and the unchanged benchmark workload/limits.

The maintainer requested this revision after the implementation's throughput
regression and approved direction to use generic partitioning and high-throughput
producer practices. This section extends the revision-14 task contract; it is
not another task, a claim of passing V8, or approval to introduce Kafka. Its
ordering policy explicitly supersedes the security posture's fixed verification
lane-zero rule. Update that owning paragraph and the corresponding Bifrost/Wyrd
producer description before implementing the routing change. Document the
already requested policy; do not invent a specification approval or silently
leave contradictory authority behind. Synchronize REQ-021's implementation
description where needed without changing any numeric criterion.

Historical comparison: the matched clean, unprofiled full records
`20261026T123533Z-0bdabaf8b.json` and `20261026T173720Z-4c3c70517.json`
under `benchmarks/verification/direct-verify/` show median c64 throughput
41,566.6 → 31,837.6 successful verifications/s, CPU 510.7 → 607.3 µs/success,
and refusals 38,184 → 154,260 per repetition. Total CPU use fell rather than
rose. Successful overhead p99 improved 1.336319 → 1.132543 ms; refusals are
excluded from that histogram. The later dirty/profiled diagnostic hit the
805,306,368-byte reservation threshold in every parsed staging-refusal warning,
establishing byte pressure for that run, not an item-count limit. Direct work
still reserves two logical outbox items; its result item holds output and
summary batches. Three tables do not imply three reservations or a linear
2/3-throughput rule.

Single-lane serialization and excessive one-row allocation are demonstrated
mechanisms. Their individual contribution to the measured regression is not
yet isolated. Mixed `outbox_write_seconds` includes multiple producers/lanes,
encoding, scheduling and acknowledgment phases; it is neither a fsync timer nor
a measurement of one verification lane's service capacity. Do not claim CPU
saturation merely from ~19 active logical cores on a 32-logical-CPU host.

### Required architecture and control flow

Use the existing generic `OutboxSink::LANES`/`lane` and bounded writer machinery.
Producers supply their ordering key; queueing, concurrency, retries, accounting
and shutdown stay in the established owners. No second partitioner, queue,
worker framework, per-execution task fan-out, global buffer registry or broker.

```text
Verification                 Gateway capture        Audit
execution-ID ordering key    request-ID key         existing request key
          |                         |                      |
          +-------------------------+----------------------+
                                    |
                       producer memory/item admission
                                    |
                  existing generic (tenant, hashed-key lane)
                                    |
                       ordered lanes 0 / 1 / 2 / 3
                                    |
                    existing bounded concurrent slice writers
                    batch rows by destination table + schema
                                    |
                  verification details ACK -> summaries ACK
                                    |
                                    v
Bifrost native / OTLP ----------> Scribe ingress admission
payloads bypass producer outbox      |
                      hash(tenant, table, batch ID)
                                    |
                         configured Scribe shard owners
                                    |
                     WAL -> group sync -> durable fence
                                    |
                           memtable insertion -> ACK
                                    |
                  complete slice -> release producer charges
```

Outbox lanes and Scribe shards are independent. Keep Scribe's existing batch
routing, shard mailbox bounds, tenant/table scheduling, shared resource roots,
WAL/fence protocol and acknowledgment point. Producer concurrency may feed more
work into one shard's group commit; it does not create another WAL owner or
require a shard per lane. Do not raise writer/lane/shard counts or memory limits
to hide the regression. Record actual configured shard geometry during
measurement: the source default of one does not prove what a benchmark process
inherited from its environment.

Apply the generic policy to the producer branches that actually use the outbox.
Gateway capture/audit already share request-key routing; preserve it. Server
spans and records lacking an ordering key retain their existing fallback.
Native/OTLP Bifrost payloads reach Scribe directly and must not be redirected
through this outbox. Their audit side effects can use it. Bifrost read benchmarks
use Oracle and are not evidence of this producer bottleneck. This revision
permits necessary shared-owner consumer closure and focused Gateway/audit
regression proof, not unrelated Gateway/Bifrost optimization or new L2 work.

### Memory accounting and allocation contract

Keep memory admission distinct from transport-frame sizing:

- Charge retained allocated capacity of owned payload buffers while queued,
  dispatched, in flight or retained for retry. Logical/encoded row bytes remain
  the frame-sizing operand; they cannot replace memory charges.
- Construct small Arrow columns with capacity appropriate to their actual
  values. Do not allocate a large default buffer and then add a compacting copy
  on the hot path. Reuse the current sized text construction, validate its
  empty/null/non-ASCII cases and examine other in-scope builders only where
  measured retained allocation shows the same problem.
- Do not repeatedly charge the same shared allocation. Establish the ownership
  boundary explicitly: queue-owned buffers are charged for their retained
  lifetime; process-wide immutable buffers are accounted once as shared
  resident memory. Merely cloning an `Arc` must not create another backing-buffer
  charge. Preserve charges for distinct owned buffers and avoid overlap errors
  for slices sharing a backing allocation.
- Extend the existing weight/admission owners with the smallest sufficient
  ownership-aware accounting. Avoid a global pointer-deduplication service or a
  per-column allocation scan on every reserve. Conservative structural overhead
  is acceptable when explained; omitting owned backing capacity is not.
- Preserve item and byte limits, their reserved-share split, reservation
  sequencing, release-on-settlement/loss, fail-fast errors and observed-loss
  accounting. Do not release a charge on dequeue or before a retry completes,
  raise the configured byte budget, or turn an admitted bundle into silent loss.

At revision authoring, `verification/results.rs::text` already uses
`StringBuilder::with_capacity`, and its sized-allocation regression exists.
Treat passing implemented behavior as verification-only. Preserve concurrent
audit projection work and existing diagnostic edits; validate the final
integrated candidate rather than duplicate or revert another correction.

### Partitioning, batching and retry contract

Verification input items and their output/summary result item use the same
execution-ID ordering key, regardless of direct/submitted/grouped/scheduled
origin or which attempt produced the result. Hash through the existing lane
mechanism; a result ID, table ID, frame ID or group ID must not split or serialize
that execution incorrectly. Different executions must be able to occupy
different lanes within one tenant. Preserve existing per-tenant separation and
bounded scheduler progress across tenants; add no strict fairness guarantee.

An execution's input is staged before its result item. One active slice per
tenant/lane and FIFO across subsequent slices preserve that relationship.
Same lane does not promise the same slice. Within result delivery, retain
successful output/detail acknowledgment before summary submission; a retryable
detail failure prevents advancing to summaries. Keep these rules when an
acknowledgment is lost, a slice is retrying, or another tenant progresses.
Replays retain the sealed content/identities and deduplicate through existing
batch fences; they must not mint a new result or allow later same-lane writes
to overtake an uncertain earlier write.

The historical terminal-rejection exception in the execution evidence remains
unchanged: an unsplittable output that Scribe rejects terminally is logged,
counted lost and consumed, and its summary can still be written. It is outside
the successful-detail ACK rule, not a new exception for full producer memory or
a reason to return a fabricated complete result. Preserve that exact negative
case in RV2 and the existing reader's incomplete-result behavior. This revision
does not redesign terminal settlement or promise atomic cross-table retention.
Its admission correction must introduce no additional post-admission loss.

Keep destination/schema batching, bounded chunks in flight and the existing
global writer pool. Do not send one request per execution/table, add unbounded
concurrency, merge outputs back into summaries, or weaken cross-table read
assembly to excuse missing inputs/outputs. Keep source activity notification
and shutdown handoffs after their existing acknowledgment boundaries.

### Delay and backpressure policy

Do not add an artificial batching sleep, linger setting, server admission wait,
larger queue, new retry policy or background timer in this revision. Existing
backlog batching and bounded in-flight work provide the initial batching policy;
milliseconds of deliberate delay cannot be assumed compatible with the
unchanged sub-500-µs overhead target.

Preserve producer fail-fast staging refusal and Scribe's bounded admission.
Pressure on background work uses the existing queue/defer/retry behavior.
Audit never delays or refuses the originating operation, including under full
buffers and while verification lanes are stalled. Do not suppress required
audits or warning/loss accounting to make the benchmark appear faster.

Real callers retrying a refused operation should use capped backoff with jitter
and bounded outstanding requests. That is guidance, not authorization for an
SDK retry implementation in this task. The benchmark's immediate next-request
loop, client counts, warmup, windows and sample floors stay unchanged. Adding
client sleeps/backoff, reducing arrivals or judging only a friendlier workload
cannot close V8. Any bounded-wait or deliberate-linger policy requires a later
explicit latency/API decision backed by measurements.

These choices use Kafka's partitioned ordering, batching, bounded buffering and
retry-safety principles without copying its numeric defaults, sequence protocol
or blocking-buffer API. Reference: [Kafka producer configuration](https://kafka.apache.org/41/configuration/producer-configs/),
[partitioning](https://kafka.apache.org/41/getting-started/introduction/), and
[Arrow memory semantics](https://arrow.apache.org/rust/arrow_data/struct.ArrayData.html#method.get_buffer_memory_size).

### Ordered implementation scenarios for this revision

#### Scenario R1 — Retained memory is reduced and charged accurately

**Behavior.** Small nullable text columns retain capacity near their contents;
larger owned values remain fully charged. Shared immutable buffers are not
repeatedly charged as distinct allocations. Pending and retrying work retain
their memory/item charges, and full admission preserves existing refusal and
nonblocking audit behavior.

**RED.** Use [RV1](#rv1--allocation-and-admission) for existing sized-allocation
and capacity proof. Verify the implemented sized builder rather than manufacture
RED. Add the planned shared-buffer test and demonstrate the existing repeated
charge, including a distinct-allocation counterexample, before correcting it.

**GREEN.** Reuse the sized builder and existing sink weight/admission owners.
Record representative input/output/summary retained allocation and admission
weight before/after, distinguishing owned capacity from shared memory. Pass
RV1/RV2 without increasing limits or bypassing admission.

**REFACTOR.** Delete redundant size calculations or copies at their owning
boundary. Retain a cheap stage path and avoid a new memory-accounting subsystem.

#### Scenario R2 — Independent executions drain concurrently with ordered replay

**Behavior.** One tenant's unrelated execution can make progress while another
lane is stalled. Input/result items for the same execution remain ordered even
when dispatched in different slices; output is acknowledged before its summary.
Lost-ACK replay deduplicates and blocks later same-lane overtaking. Other tenants
and existing request-key Gateway/audit routing retain their current progress.

**RED.** Extend RV2's planned verification-key and concurrent-delivery tests.
Hold one submission deterministically and assert another lane progresses;
inject lost acknowledgment and check identical replay and phase order. Do not
prove distribution solely by hashing IDs or infer concurrency from a stopwatch.

**GREEN.** Update the owner lane rule and use execution keys in existing generic
routing; preserve shared writer/chunk bounds, batching, retry fencing and group
notifications. Run RV1/RV2 and affected real-server V1/V6/V7 proof. Existing
V1–V7 results can be carried forward only where dependencies remain unchanged.

**REFACTOR.** Remove fixed verification lane-zero assumptions and obsolete test
assertions, preserving their replacement map. Add no parallel producer framework
or replacement Scribe sharding.

#### Scenario R3 — Normal production measurements establish the improvement

**Behavior.** Full normal L1 verification retains exact results, all required
storage/audit work and valid populations. Median-repetition c64 overhead p99
is strictly below 500 µs; correctness and zero unexpected refusals hold in every
repetition. Report successful throughput, CPU/success, actual memory/geometry
and refusal counts alongside latency. Lower p99 with lower successful throughput
and more refusals does not establish successful remediation.

**RED.** The matched records above supply the regression; the existing V8 limit
miss remains performance RED. If diagnostic cost attribution is needed, use
existing timings/profiles and bounded temporary probes. Do not treat mixed
outbox durations as fsync CPU or fast refusals as successful latency samples.

**GREEN.** Use [RV3](#rv3--production-evidence-and-consumer-closure), then the
existing final V8 command on clean caller-authorized source. Keep source/config
identities and per-repetition operands. Separate allocation/accounting and
partitioning diagnostic results so their contributions can be assessed; only
the integrated ordinary production configuration supplies acceptance.

**REFACTOR.** Remove temporary hot-path probes before final measurement and
rerun affected correctness proof. Preserve historical evidence and do not
promote references or claim a passing benchmark from a dirty/profiled run.

### Source-backed reuse and proof

| Owner | Existing capability, precise gap and required extension |
|---|---|
| `wyrd-runtime/src/outbox.rs::OutboxSink`, reservation/load and writer dispatch | Generic keyed lanes, retained charges, bounded global writers and ordered retry already exist. Reuse them; change only demonstrated accounting gaps and preserve multi-tenant progress. |
| `wyrd-server/src/scribe_outbox.rs::ScribeSink::{lane,weight,write}`, frames/submit | Gateway/audit already hash request keys; verification falls back to zero. Route its existing item variants by execution identity; preserve frame batching and ACK phases. Current weights sum Arrow array memory and can repeat shared allocations. |
| `verification/results.rs::text`, `ONE_NULL_TEXT`, `ResultPayloadBuilder` | Sized text allocation is already present; shared null arrays remain reused. Verify capacity/representation and correct charges at the established owner, without another encoder or duplicate payload. |
| `vala-bifrost-redux/src/scribe/{routing,ingress,shards}.rs`; Gate native/OTLP ingestion | Existing independent shard routing, bounded queues, group commit and durable deduplication. No new shard or payload routing is needed for this revision. |
| Security posture producer-outbox paragraph; Bifrost producer/durability sections; benchmark REQ-021 | Replace tenant-wide verification serialization wording with execution-key ordering and independent lanes. Explain retained-memory versus frame-byte budgets and unchanged backpressure/durability. |

The following planned names live in existing modules/targets; confirm actual
selectors after adding them, before RED. Existing names were checked from source.

#### RV1 — Allocation and admission

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::results::tests::a_one_row_text_column_holds_about_its_value)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::shared_buffer_charges_preserve_owned_memory)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::large_items_shed_on_bytes_before_items) | test(=outbox::tests::bounded_staging_covers_retrying_and_inflight_work) | test(=outbox::tests::bounded_audit_staging_remains_nonblocking)'
```

The shared-buffer selector is **planned**. Its assertions cover shared backing
storage, distinct owned allocations, null/empty text and retained lifetime;
allocation-size expectations must express the property rather than freeze a
dependency's incidental struct layout. If the correction changes generic load
ownership, extend the relevant existing admission test at that owner as well.

#### RV2 — Partitioning, phase ordering and replay

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::verification_items_share_execution_lanes_and_executions_spread) | test(=scribe_outbox::tests::verification_lanes_keep_phase_order_across_retry)'
mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=outbox::tests::a_tenants_lanes_write_concurrently_and_each_keeps_order)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::a_calls_writes_share_a_lane_and_calls_spread) | test(=scribe_outbox::tests::a_slice_of_small_writes_travels_as_one_frame) | test(=scribe_outbox::tests::a_retry_resubmits_identical_frames) | test(=scribe_outbox::tests::a_slice_keeps_a_bounded_number_of_frames_in_flight) | test(=scribe_outbox::tests::an_unsplittable_row_is_submitted_whole_and_terminal_refusal_consumes_it)'
```

The first two server selectors are **planned**. Reuse the current recording
Scribe and blocked-lane seams. Add deterministic cross-tenant progress and
separate-slice/lost-ACK variations within those existing owners; no new test
binary or duplicated SDK journey. Reuse V1/V6/V7 for real-server readback and
grouped late-evidence closure; no full cross-language journey-suite rerun solely
to prove a shared internal partitioner.

#### RV3 — Production evidence and consumer closure

Use existing V8 diagnostics and final normal verification command; do not
duplicate their recipes here. Run the existing `mise run bench:l1:gateway`
when shared Scribe weight/frame/audit projection changes affect Gateway, to
retain capture accounting and unchanged limits. This is consumer-regression
proof, not a new Gateway optimization task. Outbox-only changes do not justify
running Bifrost read benchmarks or claiming their payload write path improved.
An unexpected need to alter Scribe shard routing/durability is a material scope
change, not permission to tune it until the benchmark passes.

Capture resolved shard count, lane/writer bounds, byte/item budgets and ordinary
telemetry configuration in the execution evidence alongside record paths.
Measure retained allocation/weight on representative batches and byte/item
refusal reasons using existing diagnostics; do not add high-cardinality or
per-function permanent metrics. Compare like-for-like unprofiled full records.
Keep normal benchmark source/host/config checks, no automatic commits, and the
existing V9 format/lint/boundary/diff closure. This document-only revision uses
static link, command-owner and scenario checks; it runs no runtime proof.

### Acceptance mapping and boundaries

| Obligation | Completion observation |
|---|---|
| REQ-021; S5-PERSISTENCE-COST | RV1–RV3 and V8 prove actual allocation reduction, safe shared-buffer accounting, concurrent keyed delivery and the unchanged performance/correctness criteria; no larger budget or hidden shedding. |
| INV-001/002 | RV1/RV2 and affected V1/V6/V7 preserve bounded admission, nonblocking audit, attribution, selected results, grouped ordering and lost-ACK deduplication. |
| REQ-019; AC-010/014 | Comparable ordinary records distinguish successful work from refusals and component elapsed time from CPU. Temporary diagnostic probes are absent from final acceptance. |
| Generic ownership | Existing outbox owners handle every affected producer; Gateway request-key behavior and direct Bifrost payload ingestion remain correct. Source-owner text describes two independent partitioning layers. |

Append revision evidence separately from the historical implementation table.
Map removed/replaced assertions to their retained proofs; preserve existing
correctness evidence where unchanged. A passing sized-allocation check alone
does not prove drain capacity or complete the task.

Excluded: a Kafka dependency, new queue/partitioner/accounting service, larger
resource limits, new shard geometry, deliberate linger/admission waits, changed
SDK retries or benchmark arrival loop, weaker audit/durability, re-embedded
outputs, new routes/wire contracts, L2 and reference promotion. If correctness
needs one of these or another expensive-to-reverse policy, stop with its exact
owner/evidence instead of silently expanding this revision.

Revision authoring checks: source owners, existing test names, document links
and the three complete scenario blocks were checked. An independent read-only
comprehension pass against the task/spec found one terminal-rejection ambiguity;
the explicit preservation rule and RV2 negative proof above resolve it. No
implementation, runtime verification or performance PASS is claimed by this
document revision.

### Revision execution evidence

Uncommitted candidate on `415082a72` (no commit authorized). Unrelated
`projection.rs` edits were preserved; the revision adds only the sized text
builder and its test to that file.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| R1 sized allocation (reuse) | `verification/results.rs::text` (existing, `415082a72`) | RV1 `a_one_row_text_column_holds_about_its_value`, extended to null, empty, multi-byte: PASS (verification only, no RED manufactured) | PASS |
| R1 shared-buffer charge | `ResultBatch::owned_bytes` skips the process-wide `ONE_NULL_TEXT`; `ScribeSink::weight` uses it for inputs and results | RV1 `shared_buffer_charges_preserve_owned_memory`: RED 6,328 charged vs 4,024 owned; GREEN equal; a 64 KiB output stays charged above its bytes | PASS |
| R1 audit allocation (measured, same defect) | `tables/audit/projection.rs` text closure sized with `StringBuilder::with_capacity` | `text_columns_hold_about_their_values`: RED `trace_id` 1,216 B; GREEN all < 256 B | PASS |
| R1 admission unchanged | no limit, share, or release change | RV1 `large_items_shed_on_bytes_before_items`, `bounded_staging_covers_retrying_and_inflight_work`, `bounded_audit_staging_remains_nonblocking` 3/3 | PASS |
| R2 execution lanes | `ScribeSink::lane`: inputs and results by `execution_id`; queued submission inputs by `SubmissionSink::lane` (idempotency key); capture/audit by request; spans lane 0 | RV2 `verification_items_share_execution_lanes_and_executions_spread` (RED: all lane 0), `verification_lanes_keep_phase_order_across_retry` (held lane, other lane and other tenant drain, lost-ACK replay keeps batch id, inputs → outputs → summary across separate slices) | PASS |
| R2 existing ordering/batching | slice fixtures now attempts of one execution (`detail`) so they stay one slice | RV2 generic `a_tenants_lanes_write_concurrently_and_each_keeps_order`; five existing server selectors 5/5; `scribe_outbox::` 17/17 | PASS |
| R2 group activity under concurrent lanes | `RECORD_GROUP_ACTIVITY_SQL` upserts `ORDER BY binding_id, group_id` (consistent lock order) | `wyrd-sql` `pg_verifier_executions` 27/27 | PASS |
| R2 authority text | security posture producer-outbox paragraph, Bifrost audit/outbox paragraph, REQ-021 bullet: producer lanes by request, execution, or submission key; distinct from Scribe shards; owned-memory accounting | `docs:check` exit 0 | PASS |
| Carry-forward V1/V6/V7 | — | V1 + V7 `wyrd-ai` 5/5; V6 68/68; `wyrd-server --lib` 579/579 | PASS |
| R3 / V8 limit | — | dirty diagnostics below; c64 overhead p99 1.17 ms | FAIL |

Retained-memory weights (owned Arrow bytes per staged write):

| Write | Before | After |
|---|---|---|
| Result (output + summary) | 6,328 | 4,024 |
| Direct inputs record | 3,571 | 2,419 |
| Audit decision | 20,704 | 3,465 |

Commands (all exit 0 unless stated):

- `mise exec -- cargo nextest run --locked -p <crate> --lib -E '<RV1/RV2 selector>'` for each RV1/RV2 line above, with quoting preserved.
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(/^pg_verification_groups::/) | test(/^pg_verification_routes::/) | test(/^pg_verification_runtime::/)'"` — 68/68.
- V1/V7 `wyrd-ai --test integration -P journey --run-ignored=all` exact selectors — 5/5; `wyrd-sql` `pg_verifier_executions` 27/27; `wyrd-server --features test-support --lib` 579/579.
- `mise run fmt`, `lints`, `docs:check`, `check:deps`, `check:tenant-isolation`, `git diff --check` — exit 0.

Diagnosis — `pg_verification_routes::enqueued_executions_replay_conflict_and_read_back`:

- **Symptom:** intermittent: the first keyed execution settled `errored` with an idempotency conflict instead of `pending`.
- **Evidence:** trace `submission conflicts ... execution_id=<first> original=<later>`; an independent read-only diagnostician confirmed it.
- **Cause:** keyed submissions are decided in `SubmissionOutbox` arrival order (`wyrd-design.md` "Requests sharing an idempotency key are written in arrival order"), which held only because all inputs rows shared lane 0. Execution-ID lanes let a later same-key request be acknowledged first.
- **Fix site:** `ScribeSink::lane` lanes a queued inputs row by `SubmissionSink::lane`. The other `ExecutionInputs` callers (direct, group, schedule) have `queued: None` and are unchanged.
- **Result:** 20/20 after the fix.

Test mapping: `detail()` fixtures become attempts of one execution, so `a_slice_keeps_a_bounded_number_of_frames_in_flight` and `a_silent_peer_times_out_and_the_slice_retries_on_the_next_scribe` still assert one slice's frame bound and phase order. No assertion was weakened.

V8 diagnostics (dirty, `mise run bench:l1:verification -- --steps direct-verify-c64 --allow-dirty`; INVALID by construction, not acceptance):

| Record | Host | Successful/s | CPU µs/verification | Errors | c64 overhead p99 (median rep) |
|---|---|---|---|---|---|
| `20261026T173720Z-4c3c70517` (clean, before) | quiet | 31,837.6 | 607.3 | 154,260 refusals/rep | 1.133 ms (refusals excluded) |
| `20261026T193423Z-415082a72` | other load present | 49,722.6 | 417.1 | 0 | 1.218 ms |
| `20261026T193658Z-415082a72` | quiet (QEMU VM ~0.3 core) | 50,807.4 | 409.8 | 0 | 1.169 ms |

Accounting and partitioning were not isolated in clean records. Dirty record `20261026T180424Z-415082a72`, taken earlier in this revision, reported 146,889–178,328 errors per repetition (median 155,621); with lanes, both later records report 0.

Configuration: Scribe shards 1 (`WYRD_MEM_TABLE_BUCKET_NUM` unset, source default); `SCRIBE_WRITERS` 4; `SCRIBE_LANES` 4; `FRAMES_IN_FLIGHT` 8; `scribe.outbox_capacity` 65,536; `scribe.outbox_bytes` 1 GiB; ordinary telemetry.

Profiled diagnostic `20261026T193910Z-415082a72`, profile `target/benchmarks/verification/direct-verify/20261026T193910Z/profiles/profile-1223081.json.gz`, CPU shares of server:

- Tokio threads 59.8%; Scribe ingress 14.5%, persistence 13.6%, coordinator 6.2%, WAL 5.8%.
- Outbox `Chunk::encode` ≈ 8.9%, almost entirely `concat_batches` of one-row batches; freeing written slices ≈ 3.9%; mimalloc ≈ 17% of Tokio-thread self time.
- During the measured window the server used 20.8 cores and the driver 5.3 of 32 logical CPUs, while mean request phases were admit 0.021 ms, stage 0.068 ms and engine 0.125 ms. The p99 tail therefore tracks CPU scheduling under host saturation rather than per-request work.

Remaining limitation: V8 c64 overhead p99 fails (1.17 ms against < 0.5 ms); the task is not complete. Producer-side encode CPU is within this revision's owner. Scribe ingest/persistence CPU (~40%) is Bifrost Scribe's and outside this revision.

## Approved revision — build batches once and remove shared Scribe copies

### Outcome, approval and precedence

The maintainer explicitly approved the implementation plan in the conversation
and requested that it be recorded here. This is the next revision of the same
Task 006 remediation, under approved specification revision 14 plus that
direction. It is not a new task or a claim that the unfinished latency gate
passes. Preserve all earlier implementation and diagnostic evidence.

This revision brings the shared producer and Scribe work into scope **now**.
It supersedes the preceding evidence's statement that Scribe work is outside
the revision, the earlier prohibition on changing the local producer payload
path, and the earlier conditional requirement to run Gateway/Bifrost proof.
Verification, Gateway and Bifrost must all satisfy the existing agreed L1
criteria before acceptance. Do not move Scribe remediation to a later task.

Keep the three canonical verification tables and unchanged public results.
Remove four demonstrated costs in order: validation-only buffer copies,
one-row producer batches later concatenated, local IPC decode/revalidation of
producer-proven rows, and unsorted payload concatenation before persistence
sorting. The material cost is changing internal staging and local delivery
ownership while preserving resource bounds, attribution and exact replay.

Record the approved producer/canonical delivery and persistence behavior in
the owning Bifrost design, security posture and REQ-021 description before
executable changes. Do not manufacture another approval, a new public trust
flag or a spec revision number; preserve all numeric requirements. This task
record fixes the decisions below; private decomposition remains implementation
owned.

### Fixed architecture and configuration decisions

| Boundary | Approved decision |
|---|---|
| Stored facts | Inputs, outputs and payload-free results remain separate; no PostgreSQL input values or new durable tables. |
| Producer ordering | Preserve current execution-key lanes and the same-idempotency-key submission ordering correction. Preserve Gateway/audit request-key routing and deterministic group-upsert lock order. |
| Staged verification/audit data | Own scalar values and already-encoded Variant buffers; do not build a one-row Arrow batch for each record. |
| Dense batch construction | Append staged values directly to table-specific Arrow builders at drain; preserve destination/schema grouping, input order and attribution. |
| Gateway capture | Keep its existing owned capture and batch-projection capability; do not replace it with another row representation or re-encode/redact bodies. |
| Local Scribe delivery | Use existing canonical-batch ingestion for producer-proven rows, through ordinary admission and managed-envelope checks. |
| Peer/native transport | Keep IPC and full receiver-side boundary validation. No wire flag permits a caller to claim canonical trust. |
| Batch identity | Keep existing IPC-derived content hashing, domain separator and deterministic frame splitting. Local and peer delivery of the same prepared frame use the same identity. |
| Persistence sorting | Sort using key columns; gather payload directly from source batches into bounded sorted output. Do not first concatenate the entire unsorted payload. |
| Shards and worker pools | Keep the production shard default at **one**, and keep existing worker-pool provisioning, lane/writer/chunk bounds and resource limits. No topology sweep or automatic tuning in this revision. |
| Pressure and delay | Keep bounded fail-fast admission, current background retry/defer behavior and nonblocking audit. Add no linger, sleep, admission wait, larger queue or SDK retry change. |
| Benchmark work | Same clients, arrivals, fixtures, timing, telemetry, criteria and required durable work. No friendly workload or delayed persistence to manufacture a pass. |

One shard can use many ingress, persistence and WAL workers. Shard count does
not determine the count of producer lanes or worker threads. Record resolved
geometry/pool/resource configuration with measurements, not merely whether an
environment override was present. Keep these values fixed across before/after
comparisons; do not substitute a topology change when latency remains red.

```text
verification + audit                         Gateway capture
owned scalars + encoded Variants             existing owned validated capture
                  \                           /
                   bounded producer admission
                              |
              existing keyed lanes / bounded slice writers
                              |
              build dense table/schema batches once
                              |
          deterministic IPC sizing, splitting and content identity
                         /                 \
             local canonical rows        peer IPC bytes
             ordinary memory admission   full boundary validation
                         \                 /
                  existing Scribe shard routing
                              |
                 WAL -> sync -> fence -> memtable -> ACK
                              |
            freeze existing bounded persistence candidate
                              |
             sort keys -> gather sorted payload once
                              |
          staged Parquet -> preflight -> existing bounded merge
```

### Step 1 — Validation without constructing discarded owned values

**Owners:** `wyrd-queue/src/variant.rs::EncodedVariant` and
`vala-bifrost-redux/src/tables/mod.rs::validate_variant_values`.

Add the approved `EncodedVariant::validate_bytes` capability returning
`Result<(), VariantViolation>`. Delegate to the existing borrowed validator;
do not expose its upstream Variant type, implement another scanner or alter
its size/depth/numeric/canonical checks, panic containment or error precedence.
Use it where the table validator currently constructs an owned Variant solely
to discard it. Keep `from_bytes` for callers that need owned buffers.

**Behavior.** Valid and invalid raw Variants receive identical acceptance and
catalog errors, including nested values and row/column attribution, without
copying metadata/value buffers merely for validation.

**RED.** The current copy/discard call and allocation profile supply performance
RED. Add the planned equivalence test in P1 and use existing registry dispatch
proof, exercising the same valid, malformed, oversized, too-deep and numeric
fixtures through both entry points. A missing API or compilation failure is
not behavioral RED; do not invent a different validation failure for an
optimization that preserves existing semantics.

**GREEN.** Extend the existing owner, replace the validation-only call and pass
P1. Confirm profile/allocation evidence no longer attributes discarded owned
buffer construction to table validation. Full validation cost is not claimed
to disappear.

**REFACTOR.** Remove only the unnecessary owned construction at validation-only
call sites. Keep one raw-byte validation authority and its existing renderers.

### Step 2 — Construct producer batches directly at drain

**Owners:** verification `ResultPayloadBuilder`/input projection, audit
projection, `CallCapture::project` and `ScribeSink` frame construction.

Verification input, output and summary records retain their exact typed
scalars, accepted context, identity/times and already-encoded Variant bytes.
Audit staging retains normalized decision fields and encoded detail. Perform
required canonical encoding, redaction and validation at the existing producer
boundary; do not defer permission decisions, capture policy or audit safety.
Owned values must outlive the caller without borrowing request data.

At drain, the existing frame owner groups destination/schema records and
appends values directly to the existing table-specific Arrow builders. Allocate
from the batch's actual row/value sizes, produce dense bounded batches, and
append subject/actor/request attribution exactly as today. No intermediate
one-row verification/audit Arrow batches and no subsequent concat of those
batches. Keep Gateway's existing batch capture projection, rather than adding
a second capture representation. Leave native/OTLP payloads on their current
direct Gate-to-Scribe path.

Charge retained owned values until settlement/loss, shared backing buffers at
their established ownership boundary, and active construction/encoding buffers
under existing bounded work/admission owners. Preserve temporary overlap
accounting; a scalar staging representation is not permission to stop charging
its payload. Keep item count, reserved-share split and byte limits unchanged.
Do not introduce a global allocation registry, a general row engine, JSON
round trips or another queue. Change lane-key extraction to use the retained
typed identity when one-row columns no longer exist; keep its semantics.

**Behavior.** Dense projection preserves exact schema, values, nulls, row order,
attribution, grouping and frame bounds while eliminating per-record Arrow
construction and concatenation. Empty/null/non-ASCII data and large Variants
retain existing representation and pressure behavior.

**RED.** Add P2's dense-projection test in the existing outbox module, comparing
fixed expected records across verification, audit and capture cases. Use RV1/RV2
for lifetime, pressure, frame limits, ordered lanes and replay. Show current
one-row allocation/concat costs in the baseline profile; existing passing
sized-string tests remain verification-only.

**GREEN.** Change staging and projection together with their callers, weights,
row-owner mapping, tests and necessary table-projection exports. Run P2, RV1/RV2
and affected V1/V6/V7 real-server proof. Report actual allocations/copying and
encoded geometry, not only producer elapsed time.

**REFACTOR.** Remove superseded one-row constructors, concat branches and tests
whose only property is that removed representation. Map them to dense-schema,
allocation, pressure and replay proof; preserve distinct negative assertions.

### Step 3 — Local canonical delivery with transport-equivalent identity

**Owners:** `ScribeBatch`, `ScribeRoute`, existing frame construction,
`CanonicalIngress::unreserved`, ingress/execution-lane preprocessing.

Use the dense batches from Step 2 for local delivery through existing
`IngressPayload::Canonical`. Eligibility is limited to the server-owned
projections that establish their value contracts. Merely receiving a
`RecordBatch`, matching a table name or selecting a local route is not proof.
Preserve required validation in producer construction; raw external input and
peer IPC never acquire a caller-controlled canonical bypass.

Keep deterministic encoded frame-size checks/splitting and the existing hash
of IPC bytes, including attribution. Local delivery may discard the transport
bytes after obtaining the frame's identity and pass its corresponding dense
row range to ordinary Scribe admission. Peer delivery retains/transmits IPC.
Do not retain an uncharged duplicate payload. Keep staged source ownership
through retries so re-preparation reproduces the same split frames/identities.
Encoding still exists for identity/size; this step removes local decode and
repeated producer-proven value walks, not the encoding needed by that contract.

Preserve tenant/schema/fingerprint/reserved-column checks, managed stamping,
expanded/request limits, memory adoption, source partitioning, logical row
identity, WAL fencing and acknowledgment timing. Both payload modes use the
same durable path. A retryable detail failure still blocks summaries; the
existing logged/counted terminal oversized-output exception remains as defined
in the preceding revision. No atomic cross-table retention promise is added.

**Behavior.** Equivalent local canonical and peer IPC delivery retain identical
logical rows and attribution and the same prepared frame IDs, error behavior,
admission bounds and lost-ACK deduplication. Local delivery performs no IPC
decode of those rows. Canonical eligibility cannot weaken external validation.

**RED.** Add P3's local/peer outbox test, including separate slices and lost
acknowledgments, and extend the existing real-WAL canonical/native equivalence
test for the producer shapes. Include invalid externally supplied data and
forced pressure. Confirm actual absence of local native decode through the
payload seam, rather than infer it from matching final values.

**GREEN.** Extend the existing route/frame owners without a new ingestion API
or peer protocol. Pass P3, RV2 and affected public readback/permission proof.
Check retained memory for local and peer attempts separately.

**REFACTOR.** Remove the local IPC decode/revalidation detour only for eligible
producer frames. Keep a single identity/splitting implementation and a fully
validated peer/native path.

### Step 4 — Gather sorted persistence payload without an unsorted copy

**Owners:** `scribe/parquet_writer.rs::ParquetBatchEncoder`, current sort/layout
logic, `ScribeMemberStager` and existing resource accounting.

For each existing bounded persistence candidate, compute ordering from its
sort-key columns rather than concatenating every payload column first. Preserve
the exact resolved comparator, descending/null handling and equal-key ordering
over source batch/row positions. Map ordered positions back to the original
source batches and gather payload columns directly into the bounded sorted
output batches using the installed Arrow gather/interleave capability. Key
materialization is allowed; a second unsorted materialization of all payloads
is not. Do not introduce another sorting framework or dependency.

Keep candidate boundaries, scratch/resource charges, artifact identity/ordinal
rules, row-group/object targets, tenant/table/partition checks and deterministic
retry output. Budget key/index and output working memory explicitly through
the existing persistence owner. Do not split a candidate into independent
unsorted runs or change logical row multiplicity to save memory.

Retain staged Parquet preflight before authority/WAL-retirement transitions
and the later bounded sorted-run merge into target objects. Those reads prove
durability/readability and object geometry; they are not removed as duplicate
work. No WAL/checksum/fence or readback safety check is weakened.

**Behavior.** Gathered sorting emits the same sorted logical rows, nulls,
Variants and artifact attribution as the current concat-then-sort path with
less payload copying. Memory pressure remains bounded/retryable and recovery
retains every accepted row exactly once.

**RED.** Add P4's gathered-sort equivalence test with multiple source batches,
equal keys, nulls, nested/Variant payloads and partition boundaries. Use the
old concat-then-sort behavior only as a test reference, together with explicit
expected rows and existing generation/sort/tenant tests. Add the planned
admission test for key/index/output working memory before optimizing the path.

**GREEN.** Replace unsorted payload concatenation at the current encoder owner,
pass P4 and existing real Scribe publication/recovery journeys, and confirm
copy/allocation reductions without growing downstream merge work.

**REFACTOR.** Delete the obsolete production concat/sort materialization path
and unused helpers; retain the shared comparator and required durable stages.

### Step 5 — Integrated configuration and performance acceptance

Run Steps 1–4 in that order. Retain separate diagnostic evidence for each
intervention, then measure the integrated production candidate. No promise is
made that inclusive profile percentages are fully removable or that saving a
CPU percentage produces the same percentage reduction in p99.

**Behavior.** One-shard normal production configuration passes the existing
verification, Gateway and Bifrost L1 criteria with complete durable/audit work,
valid measurement populations and bounded memory. Successful throughput,
CPU/work and refusal/loss counts accompany latency. Tail-request evidence
distinguishes measured waits from hypotheses about scheduling.

**RED.** The historical 1.17 ms c64 overhead result and prior throughput/refusal
records remain performance RED. Existing successful allocation/lane tests do
not establish performance acceptance. Use existing traces/profiles for
attribution, distinguishing blocking tasks from async workers and inclusive
from self CPU.

**GREEN.** Run P5's sequential full normal experiments after focused correctness
proof. Median-repetition verification c64 overhead p99 must be strictly below
500 µs; correctness and zero unexpected refusals hold in every repetition.
Gateway/Bifrost retain all current declared latency, throughput, durability,
capture and correctness criteria. Clean, unprofiled source and the reference
host/configuration are mandatory for final acceptance. Commits require separate
caller authorization; do not fabricate clean-source evidence.

**REFACTOR.** Remove temporary probes before final acceptance and rerun affected
proof. A remaining red criterion keeps this task incomplete; it authorizes no
silent topology, pool, resource, workload, validation or durability change.

### Reuse map and verification commands

| Existing owner / inspected behavior | Selected extension and consumer closure |
|---|---|
| `EncodedVariant::from_bytes` calls private borrowed `validate`, then copies; table Variant validation immediately drops the result | Add validation-only owner capability and use it at that call; retain renderer/owned-construction semantics. No new validator. |
| Verification builder and audit projection create one-row Arrow batches; `Chunk::encode` concatenates and appends attribution | Retain owned staged fields/encoded bytes and construct dense Arrow once through existing schema owners. Close weight/key/row-owner/frame callers and tests together. |
| `CallCapture::project(&captures)` already projects a group of owned captures | Reuse this batch projection and existing privacy/canonicalization policy; no capture rewrite. |
| `ScribeBatch` stores IPC and `ScribeRoute::Local` currently builds `ArrowIpc` frames; canonical ingress already accepts internal validated batches | Carry the corresponding dense rows locally under existing admission while retaining IPC-derived identity; keep peer/native IPC checks. |
| `ParquetBatchEncoder::prepare_sorted_candidate` concatenates all columns, then `sort_batch` gathers via indices | Keep key/comparator authority, gather payload directly from source positions using installed Arrow `interleave`/`interleave_record_batch`; retain candidates, resources and durable artifact owners. |
| Existing member preflight, claim merge, WAL/fence and real Scribe journeys | Preserve these boundaries; prove exact source/readback/replay after changing materialization. |

Paths and symbols identify reuse, not a private type/file allowlist. Add no new
general row, transport, sorting or ownership framework. All new/changed Rust
items keep required rustdoc and errors at the owning tier. Native/OTLP Bifrost
clients continue to use their existing ingest surface.

The new test names below are **planned**, in inspected existing package/module
owners. Confirm each actual selector after adding it and before RED. Reuse
existing fixtures and targets; do not add test binaries or full SDK matrices.

#### P1 — Borrowed validation equivalence

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::validation_only_matches_owned_validation)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::tests::builtin_registry_dispatches_canonical_value_validation)'
```

The first selector is planned. Include identical violation precedence and nested
row/column attribution in the owning tests; retain existing hostile-input proof.

#### P2 — Dense projection and retained pressure

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::dense_projection_preserves_records_and_attribution)'
```

This selector is planned. Reuse RV1/RV2 exact commands for admission lifetime,
batch bounds, replay, nonblocking audit and concurrent-key ordering; update
obsolete representation-specific assertions with an explicit replacement map.

#### P3 — Local/peer delivery and real durable equivalence

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::local_canonical_and_peer_ipc_preserve_frames_and_replay)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path)'
```

The first selector is planned. Existing real-WAL proof must be extended to the
eligible producer shapes and matching identity/pressure cases, not replaced
with a mock. Reuse V1/V6/V7 for real-server public readback and grouped behavior;
carry other surface evidence forward only where dependencies are unchanged.

#### P4 — Gathered persistence equivalence and resource bounds

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::gathered_sort_matches_concatenated_reference) | test(=scribe::parquet_writer::tests::gathered_sort_respects_persistence_admission)'
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::parquet_writer::tests::generation_encoding_preserves_sort_tenant_and_artifact_identity) | test(=scribe::parquet_writer::tests::parquet_writer_honors_sort_order) | test(=scribe::parquet_writer::tests::tenant_binding_mismatch_fails_before_encoding)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=cross_shard::scribe_cross_shard_generations_publish_packed_objects_once) | test(=source_boundary::scribe_local_stage_to_hot_source_transition_is_exact)'"
```

The first two selectors are planned. The existing four-shard journey remains
regression proof, not a change to the one-shard benchmark/default. Keep affected
WAL/crash/recovery negative proof at its owning tests, with exact selectors
recorded before execution rather than substituting a full journey suite.

#### P5 — All required normal L1 experiments

```bash
mise run bench:l1:verification
mise run bench:l1:gateway
mise run bench:l1:bifrost:writes
mise run bench:l1:bifrost:hot
mise run bench:l1:bifrost:parquet
```

Run sequentially without overlapping builds/tests/benchmarks. Builds and setup
follow the existing mise owners. Dirty/profiled/partial/smoke experiments are
diagnostics only. Record source/configuration identity, effective geometry,
rows/frame, decoded batch geometry, allocations/copied bytes and existing
stage/CPU/work operands using bounded diagnostics. Add no permanent per-function
or high-cardinality metrics. Do not infer fsync counts from mixed outbox timing.

Reuse V9's format, lint, boundary and diff checks, plus necessary codegen/docs
checks when their sources change. This task adds no wire/schema migration,
new benchmark criterion, L2 execution or reference promotion. Existing transient
diagnostic changes in the worktree are not acceptance source; preserve unrelated
edits and inspect the integrated final diff.

### Acceptance mapping, stop conditions and evidence

| Obligation | Required owner/proof observation |
|---|---|
| REQ-021; S5-PERSISTENCE-COST | P1–P5 demonstrate fewer owned copies and producer allocations, no local re-decode of eligible frames, sorted payload gathered once, and passing unchanged full L1 limits across all three engines. |
| INV-001/002 | P1/P3 preserve full untrusted validation and local eligibility proof; RV1/RV2/V1/V6/V7 preserve memory, audit, attribution, ordering, accepted authority, selected results and replay. |
| REQ-019; AC-010/014 | Per-intervention diagnostic evidence and final clean normal records distinguish CPU savings from work transfer, changed load and measurement artifacts. Independent review still required. |
| Approved fixed decisions | One-shard default, pool/concurrency/resource bounds, three tables, public API, frame identity algorithm, fail-fast admission and benchmark arrivals remain unchanged. |

Append a new implementation-evidence subsection for Steps 1–5; do not rewrite
the earlier failing records or assert that the prior scheduling explanation was
proven. Link every removed/replaced test to its retained property. Preserve
existing terminal rejection/loss semantics and no fabricated partial results.

The implementer owns private mechanics inside these boundaries. Stop for
maintainer direction if correctness requires a different shard/pool/default,
identity protocol, validation trust boundary, durable format, memory/admission
policy, public contract or benchmark criterion. Do not substitute an unapproved
alternative or report partial performance success as completion. These are
material stop conditions, not authorization to defer agreed Scribe work.

Authoring verification: inspected existing owners, installed Arrow gather
support, test packages/targets, setup wrappers and mise experiment commands.
Static checks confirmed local links, five complete scenario blocks, planned
selector labels and all seven named existing selectors. An independent
read-only comprehension pass over the full task and current spec found no
material ambiguity or contradiction in this revision. No implementation,
runtime proof, benchmark execution or performance PASS is claimed here.

### Approved revision execution evidence — Steps 1–5

Commits: `b47082554` (Step 1), `cd26d66d3` (design owners), `c5d0ed5ad`
(Steps 2–3; also carries the maintainer's earlier uncommitted
`tables/audit/projection.rs` detail edit), `f50fdd9fb` (Step 4). Shards 1,
writer/lane/frame/outbox bounds, fail-fast admission, frame identity and the
three tables are unchanged.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Step 1 / P1 validation-only Variant | `wyrd-queue/src/variant.rs::EncodedVariant::validate_bytes`; `tables::validate_variant_values` delegates to it | `variant::tests::validation_only_matches_owned_validation`; `tables::` 47/47; `wyrd-queue --lib` 61/61 | PASS |
| Step 2 / P2 dense producer batches | `tables/staged.rs::{Cell, StagedRow}` (`project` with exactly sized builders); `stage_audit_event`; `verification/results.rs::ResultRow`; `scribe_outbox.rs::Part::dense` builds one batch per destination at drain | `scribe_outbox::tests::dense_projection_preserves_records_and_attribution`; `tables::staged::tests::staged_rows_project_into_one_dense_batch`; `scribe_outbox::tests::staged_writes_charge_their_owned_memory` | PASS |
| Step 3 / P3 local canonical, peer IPC | `FramePayload::{Ipc, Rows}`; local route submits `IngressPayload::Canonical` with IPC-derived `batch_id` and wire bytes; peer route keeps IPC validation (`grpc/capture_peer.rs`) | `scribe_outbox::tests::local_canonical_and_peer_ipc_preserve_frames_and_replay` (same batch-id→rows map on both routes, lost-ACK replay); `scribe::scribe_persistence_path::canonical_nested_batches_share_one_managed_wal_path` extended with dense audit rows canonical and as IPC, and a corrupt Variant stream refused; `gate::attribution::tests::batch_card_uids_match_the_encoded_stream` | PASS |
| Step 4 / P4 gathered sort | `scribe/parquet_writer.rs::ParquetBatchEncoder::prepare_sorted_candidate`: key-only `concat` + `lexsort_to_indices` + `interleave_record_batch`; one `Persistence` lease for keys, positions and output before building; `sort_batch` removed, comparator kept as `sort_keys` | `gathered_sort_matches_concatenated_reference` (4 source batches, equal keys, nulls in two keys, desc/nulls-first, hour boundary, Variant + list payloads, five candidate ranges, explicit expected keys); `gathered_sort_respects_persistence_admission` (RED: fails with the reservation removed; GREEN); existing 3 + `sort_batch_uses_one_key_for_every_table` 6/6; `scribe::` lib 379/379 with Postgres; `wyrd-testing --test scribe` `cross_shard` + `source_boundary` 2/2 | PASS |
| Carry-forward V1/V6/V7, RV1/RV2 | — | `wyrd-server --features test-support --lib` 580/580; V6 68/68; V1+V7 `wyrd-ai` 5/5; `vala-bifrost-redux` `tables::|audit|execution_lanes` 91; `wyrd-runtime` RV selectors 4/4 | PASS |
| Boundaries | — | `fmt`, `lints`, `docs:check`, `check:deps`, `check:tenant-isolation`, `git diff --check` exit 0 | PASS |
| Step 5 / P5 all L1 criteria | — | clean records on `f50fdd9fb` below | FAIL |

Test replacement mapping: `a_one_row_text_column_holds_about_its_value`
(verification one-row text sized to its value) and the audit
`text_columns_hold_about_their_values` (every audit text column under 256 B)
→ `staged_rows_project_into_one_dense_batch` (asserts a projected text
buffer under 256 B), with both producers now building text through
`StagedRow::project`, and `StagedRow::owned_bytes`;
`shared_buffer_charges_preserve_owned_memory` (charge equals owned bytes, large
output charged above its value) → `staged_writes_charge_their_owned_memory`
(same two assertions over staged rows). No assertion was weakened.

Diagnosis — `pg_verification_groups::available_rows_form_expiring_group_snapshots`:

- **Symptom:** 2 of 5 runs panicked at the `next` snapshot assertion with `group: null`.
- **Evidence:** an independent read-only diagnostician traced the scheduler staging the selector record after the Postgres commit; `get_execution` reads the group from Bifrost, which had not yet published it. Reproduced on HEAD before this revision.
- **Cause:** the test read a Bifrost projection without flushing the write it depends on.
- **Fix site:** test `Support::snapshot` calls `server.flush_bifrost()` before reading; production read semantics unchanged. 68/68 afterwards.

P5 records (clean, unprofiled, `f50fdd9fb`, same host and configuration as
the earlier records: 1 shard, `SCRIBE_WRITERS` 4, `SCRIBE_LANES` 4,
`FRAMES_IN_FLIGHT` 8, outbox 65,536 items / 1 GiB):

| Experiment | Record | Verdict | Red criteria | Prior record |
|---|---|---|---|---|
| verification | `20261026T215213Z`, `20261026T215339Z` | INVALID | smoke prepass `driver.saturated` c64: 6.96 and 6.74 cores > 6.40; full run not admitted | `193658Z-415082a72` c64 p99 1.169 ms |
| gateway | `20261026T215452Z` | INVALID | chat-capture c64 p99 4.514 ms, capture-shed; chat c64 p99 0.517 ms, capture-shed | `095850Z-300c60a1d` chat-capture c64 4.432 ms, capture-shed |
| bifrost writes-parquet | `20261026T220142Z` | INVALID | `budget.exceeded` 311.8 s > 300 s; c64 native 2 of 3 reps | `073045Z-69f3a066d` 351.4 s, OTLP spans floor 15.2 < 30 |
| bifrost hot | `20261026T220746Z` | INVALID | point-c64 p99 8.0 ms ≥ 5 | `095656Z-300c60a1d` 9.62 ms |
| bifrost parquet | `20261026T221256Z` | INVALID | point-c64 p99 13.13 ms ≥ 5 | `073846Z-69f3a066d` 13.98 ms |

Diagnosis — verification smoke `driver.saturated`:

- **Symptom:** both prepasses INVALID on driver CPU; no full run admitted.
- **Evidence:** smoke c64 completed 200,645 verifications (vs 166,274 in the `415082a72` smoke PASS) with driver CPU 20.9 s vs 16.5 s, about 104 vs 99 µs per request; smoke c64 overhead p99 0.740 ms vs 0.985 ms.
- **Cause:** server-side cost per verification fell, so the closed-loop 64-client driver issues more requests at an unchanged per-request driver cost and crosses its 80%-of-8-cores guard.
- **Fix site:** the capacity driver's budget or per-request cost — a benchmark criterion/driver change outside this approval. Smoke p99 0.74 ms also remains above 0.5 ms.

Gateway capture, Bifrost writes budget and Bifrost point-c64 failures were
present at earlier commits; Steps 1–4 do not touch the Gateway capture
handoff or the query paths.

Remaining limitation: P5 is red. Verification c64 is not measurable under the
current driver guard and its smoke p99 misses 0.5 ms; Gateway and Bifrost
retain their prior red criteria. The task is not complete. Maintainer
direction is required on the driver budget/efficiency and on the Gateway and
Bifrost criteria, which this revision does not authorize changing.

Commands: P1–P4 exact selectors as listed in the reuse map;
`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::/)'"`;
`mise run bench:l1:verification` (twice), `bench:l1:gateway`,
`bench:l1:bifrost:writes`, `bench:l1:bifrost:hot`, `bench:l1:bifrost:parquet`,
sequential with no overlapping builds.

Review `TASK-006-R1-batch-once-c9794a0b0` remediation: FIND-006R1B-2
`Part::dense` refuses a frame mixing staged rows and batches instead of
rebuilding one-row batches; FIND-006R1B-3 `# Panics` added to
`StagedRow::column` and the `results.rs` test helper `project`;
FIND-006R1B-4 corrected above. FIND-006R1B-1 (per-step profile, allocation
and retained-memory evidence) remains open: profiled verification L1 runs on
the candidate commits stop at the same `driver.saturated` smoke prepass, so
they need the maintainer's P5 decision first.
