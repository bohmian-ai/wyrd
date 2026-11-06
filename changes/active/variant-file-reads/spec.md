---
id: SPEC-variant-file-reads
revision: 2
status: approved
---

# Variant path consolidation and faster memory and staged queries

Scribe learns Variant layouts during claim assembly and applies them to new hot
generations, including in-memory rows and their staged runs. The reported failure
is that in-memory data adopted this shredding but Variant query performance did
not improve. Proving that storage contains typed leaves does not prove that a
query uses them efficiently. The execution path must be traced to establish where
the expected benefit is lost; unchanged timing alone does not establish the cause.

Consolidate Variant path handling across the existing source execution owners so
projection and filtering benefit from learned leaves in memory and staging.
Staged and published Parquet retain one existing per-file reader policy. Success
requires measured query performance improvement for memory and staging, supported
by evidence of reduced execution work. L1 Variant workloads across memory, staged
and published Parquet sources must be comparable.

For example, `COUNT(*) WHERE attributes ->> 'region' = 'eu-west'` returns the same
count from canonical and learned sources. When the learned layout covers that
path, execution uses its leaves without reconstructing unrelated attributes, and
matched memory and staged measurements demonstrate the query benefit. Selecting
all attributes still returns its complete logical value, including residuals.

This reuses the current learning, storage and query owners. The cost is adapting
query requirements to each source layout and proving both correctness and actual
performance. Whole-root reads and unsupported paths still require complete
reconstruction; the change promises no speedup for those cases.

## Required behavior

- **REQ-001:** ordered projection/filter requirements reach the existing source
  execution owner for local/remote memory, staged and published data, with fixed
  output schema/order and exact-versus-pruning status before IO.
- **REQ-002:** literal paths covered by a source's learned layout use only their
  required leaves, metadata and residuals for projection and filtering. Coverage
  includes the literal nested/list paths and comparison, `IN` and null predicates
  required by the nested-read authority. Missing/null and mixed types retain exact
  semantics; whole-root uses, unshredded paths and unsupported expressions retain
  complete fallback. Zero projected columns preserve cardinality. Statistics
  never substitute for exact filtering.
- **REQ-003:** staged and published files share the existing per-file planning,
  expression adaptation, projection, pruning and footer/range-access policy.
  Preserve truthful sizes/rows/event-time metadata and conservative typed unknowns.
  Sharing code does not erase their different cache eligibility/lifetime rules.
- **REQ-004:** data owners plan their own files. Preserve tenant/table/version and
  signed-assignment proof, permissions, deadlines, source-generation leases,
  publication cuts and retirement/cancellation semantics. A narrow query or COUNT
  cannot bypass tenant/footer proof or require leader-side remote footers.
- **REQ-005:** executed-work evidence distinguishes unknown from zero, counts each
  accepted source once and excludes refused/stale attempts. Decoded Arrow bytes
  are not labelled compressed file IO.
- **REQ-006:** covered leaf-focused Variant queries show repeatable reductions in
  query CPU and latency on both memory and staged sources against the unchanged
  baseline. Execution evidence must explain the improvement through use of the
  learned layout and reduced unnecessary work. Confirmed shredding, a changed
  plan or an unchanged performance result alone does not satisfy this requirement.

## Scope and constraints

Reuse Oracle, current private source assignment, the existing DataFusion/Arrow
Variant and nested-read support, and existing storage/reader/cache owners. Keep
IO-free contracts free of runtime execution types. A compatible repin of the
existing fork may expose needed supported capabilities; no new planner, Variant
implementation, decoded cache, second resident representation, public execution
API, dependency family, major version or persisted format is authorized.

Keep canonical WAL/fingerprint/fence/ACK, learning/sampling/residual, source
publication and Forge behavior. Query execution over captured sources is in
scope; gateway capture and ingest pipeline changes, outbox scaling, gateway
accounting and cache-fill work are not. This spec must build, test and compare
against the main baseline independently of resource-scaling and gateway changes.

The expensive boundaries remain unchanged: server owners execute source-local
queries, physical learned layouts remain private, and callers receive logical
results through the existing public query surface. No public selector for source
tier or learned layout is added.

## Acceptance

- **AC-001:** real SQL agrees on values/types/cardinality for canonical versus
  learned data across memory, staged and published sources, locally and remotely.
  Cover scalar/filter-only, whole-root, fallback, COUNT, null/missing and nested/list cases.
- **AC-002:** covered queries demonstrably avoid unrelated reconstruction and use
  the correct leaves in memory and staged reads, including after a claim teaches
  a newly opened generation. Trace the query requirements through the actual
  source execution to establish where the baseline loses the learned-layout
  benefit and show that the change removes that cause. Staged/published scans
  share applicable reader policy while retaining source-specific cache/lifecycle
  safety; already-shared behavior requires preservation evidence, not replacement.
- **AC-003:** publication, retirement, cancellation and remote-failure journeys
  preserve exact rows, tenant proof and retained generations/leases. Refused and
  stale attempts contribute no accepted-source work; completed accepted sources
  contribute once, with unknown measurements remaining distinct from zero.
- **AC-004:** the existing L1 benchmark owners exercise the same Variant query
  matrix across memory, staged and published Parquet sources, using identical
  keys, values, row counts, selectivity and query shapes. Record actual settings,
  source placement, layout coverage and cache state so tier-specific IO is
  visible. Compare baseline versus changed execution within each tier and
  canonical versus learned layouts on matched logical data. Report query CPU,
  latency and reconstruction work, plus write/staging cost. Preserve the existing
  L1 workloads, target reporting and write/durability checks.
- **AC-005:** repeated matched L1 measurements demonstrate memory and staged
  query CPU and latency gains beyond observed run-to-run variation for covered
  leaf-focused queries. Report each tier separately and connect the gains to
  AC-002's execution evidence. A benchmark's valid exit status or a result within
  measurement noise is insufficient proof of improvement. Published Parquet is
  the shared-reader comparison and correctness reference; its results are
  reported, with no new speedup target imposed by this change.

Use narrow owner tests and real SDK-to-server query journeys for correctness,
including negative paths, and L1 measurements for performance. Acceptance maps
AC-001 to REQ-001/002, AC-002 to REQ-002/003, AC-003 to REQ-004/005, AC-004 to
REQ-005 and AC-005 to REQ-006. No combined gateway/resource qualification project
is required. Read gains waive no existing correctness or durability obligation.

## Open decisions

No open material product or architecture decision. The exact execution defect
and reversible repair remain to be established during implementation. If matched
measurements show no repeatable memory or staged gain, the performance objective
remains unmet and the evidence must be brought back for a maintainer decision.

## Authority and baseline

Fresh baseline: main at `5234961d51798581a026834cfffd5efaadc226b4`. Existing owners are Oracle's query
source/expression, per-file reader and storage paths, plus the pinned fork.
Follow [Variant SQL](../../../architecture/bifrost-design.md#variant-sql),
[nested reads](../../../architecture/bifrost-design.md#nested-field-reads),
[source authority](../../../architecture/bifrost-design.md#immutable-planning-and-source-authority)
and [resource invariants](../../../architecture/bifrost-design.md#resource-and-failure-invariants).
The existing [L1 benchmark contract](../../../benchmarks/README.md) owns workload
execution and reporting; this spec adds change-specific performance acceptance
without changing benchmark exit semantics.

Revision 1 was drafted 2026-11-06 from the maintainer's fresh-start decision.
Revision 2, on the same date, makes Variant path consolidation and measurable
memory/staged query improvement the goal, records the reported shredding-without-
speedup failure, and makes L1 cross-tier comparability explicit. The maintainer's
2026-11-06 instruction to decompose this revision with `wyrd-plan` approves it
for task planning. Implementation and benchmark execution are separate work.
