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
