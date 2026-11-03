---
id: SPEC-bifrost-pruning-statistics
revision: 3
status: approved
---

# Correct empty-route pruning and preserve usable partial statistics

## Problem and recommendation

Oracle uses DataFusion statistics to rule out files, in-memory batches, or live
time-partition routes that cannot match a query. A container means one such
file, batch, or route. Two independent defects need correction. Wyrd invokes
DataFusion pruning for empty live-route selections, producing conversion
warnings even though no data is eligible. In the pinned DataFusion fork,
mixing an exact column bound with an unknown bound can also fail
while constructing the statistics array: the unknown entry has no column type.
DataFusion then discards that array and logs a warning. This preserves the
conservative fallback but loses usable pruning evidence. Observe queries make
the warning frequent because several queries evaluate the same predicates.

Correct both generic behaviors in DataFusion: empty container selections and
conversion of partial statistics. Restore the isolated shared statistics
correction already present in its fork, add the empty-input and diagnostic
behavior below, and consume the compatible pinned revision in Wyrd. Keep available exact evidence,
represent unavailable evidence without a type conflict, and preserve the rule
that uncertainty alone cannot exclude a container. Do not suppress the logger,
change the UI's queries, manufacture exact statistics, or introduce a Wyrd
replacement for DataFusion pruning.

For example, a filter asks for `start_time_unix_nano >= T`. Batch A has a valid
maximum below T, while batch B has no usable maximum. Before correction, the
mixture can discard both maxima and keep both batches. After correction, A is
excluded and B remains eligible. Query rows must match execution without this
statistics shortcut. Unknown statistics do not mean that the stored rows are
SQL null.

The investigation reproduced the reported warning pattern through an actual
DataFusion predicate over empty route statistics and traced a reachable empty
execution-partition path in Wyrd. It also reproduced mixed-bound conversion
failure. The inspected span file and Iceberg manifest have bounds for every
reported column. The original requests' precise route counts and output
partitions were not captured; no claim depends on missing persisted statistics.

Wyrd supplies some statistics because its query source combines published
Iceberg files, unpublished Parquet objects, and live in-memory batches. Iceberg
already prunes published files using manifest metrics; DataFusion reads Parquet
footer statistics for row-group pruning. Neither automatically obtains min/max
bounds for Wyrd's custom live memory source or time-partition routes. Scribe
records batch bounds through existing DataFusion accumulators, and route
selection supplies each route's declared time-partition interval. Those route
bounds describe where live data can occur, not newly computed file statistics.

The adapter also installs the required per-file tenant proof and joins the
pinned published cut with authenticated live captures. This change preserves
that boundary and existing library-owned file pruning. It does not replace
Iceberg's statistics implementation or expand collection of live string bounds.

## Costs, alternatives, and limits

The conversion remains proportional to the number of containers for each
requested statistic. A typed array with unknown entries can use more memory
than an all-null array, but preserves useful evidence and avoids the failed
conversion path. There must be no new storage reads, ingestion-time statistics
collection, dependency, background process, public option, or benchmark system.
All-unknown and empty inputs provide no bounds and must not require a useful
statistics array to be materialized.

Fewer eligible containers can reduce IO, decoding, predicate evaluation, and
repeated warning formatting. Queries whose data cannot be excluded may gain
nothing. Correct scan estimates may also change admission or execution choices;
no percentage latency improvement or unchanged physical plan is promised.
Compare all-known, mixed, and all-unknown cases using a bounded existing fixture
and existing measurement facilities, separating logical pruning work from cache
and timing effects. No synthetic host load is required.
Timing observations are reported without an invented percentage gate. Acceptance
requires unchanged work scaling, no added IO for conversion, and no increase in
containers scanned for the same all-known or all-unknown evidence.
The dependency-owned empty-input correction removes conversion and log work, not data scanning:
an empty route selection contains nothing to scan in the first place.

Hiding the warning leaves the lost optimization. Disabling pruning increases
work. Collecting every missing bound adds write cost without fixing conversion.
An unrelated dependency upgrade broadens risk. The selected correction retains
the current ownership and compatible dependency APIs.

## Required behavior

- **REQ-001 — Partial evidence survives.** For minimum and maximum statistics,
  valid exact bounds remain usable when other containers have absent or inexact
  bounds or lack the requested column-statistics entry. Container order and
  count stay aligned. This prevents one unknown entry discarding usable bounds.
- **REQ-002 — No-evidence inputs are ordinary.** Empty inputs, unknown columns,
  and inputs whose bound entries are all unavailable return unavailable statistics without a
  conversion warning. This prevents normal absence being reported as failure.
  DataFusion accepts zero containers as an ordinary empty selection, including
  calls from Wyrd's live-route adapter. This covers both filters removing every
  route and an execution partition receiving no routes. The correction belongs
  in the dependency; it must not require a new Wyrd guard or wrapper.
- **REQ-003 — Parameterized types remain exact.** Supported bound types retain
  their representation, including timestamp unit and timezone, string
  representation, and decimal precision and scale. Unknown entries must not
  introduce a type mismatch. This prevents conversion from changing semantics.
- **REQ-004 — Genuine failures stay visible.** An actual incompatible exact
  statistic retains conservative behavior and a diagnostic that identifies the
  failing statistic or describes bounds generically, with a useful conversion
  reason. Maximum failures must not be labeled minimum failures. Diagnostics
  must not expose stored scalar values or query parameters.
- **REQ-005 — All existing consumers receive the correction.** Wyrd's scan
  sizing, batch exclusion, full-match proofs, live-route selection, and dependency file-pruning paths
  use the corrected shared owner without a parallel implementation.
- **REQ-006 — Close the observed incident with evidence.** Reproduce the reported
  Observe time predicates on `start_time_unix_nano`, numeric predicates on
  `kind`, and string predicates on `service_name` and `gen_ai_operation_name`
  under an identified server/dependency revision,
  identify a reproduced failing statistics shape on the reachable production
  path, and show that the correction eliminates
  conversion warnings caused by legitimate unknown evidence. Any remaining
  warning receives a source-backed explanation; absence of warning alone is
  not proof of restored pruning. Distinguish reproduction from historical
  attribution; reconstructing each original request's route count is not
  required when that historical trace is unavailable.

## Invariants and boundaries

- **INV-001 — Never lose matching rows.** A container cannot be excluded by an
  unavailable or inexact bound alone. Query results, duplicates, and SQL-null
  behavior match execution without statistics-based exclusion. Valid independent
  evidence may still exclude a container.
- **INV-002 — Do not invent evidence.** An unknown bound never becomes zero,
  an empty string, an extreme sentinel, a SQL-null data assertion, or an exact
  bound. Inexact evidence is not promoted to exact.
- **INV-003 — Keep existing source authority.** Tenant authorization, per-file
  tenant proof, field-id schema projection, pinned-source selection, and
  exact-once visibility remain unchanged. The correction cannot make an
  aggregate bypass a required source proof.
- **INV-004 — Preserve contracts and ownership.** No public API, SDK, SQL syntax,
  schema, durable data format, wire contract, or deployment topology changes.
  No stored-data rewrite or migration is required. The existing DataFusion fork
  owns the fix; Wyrd owns its compatible pin and integration verification.
- **INV-005 — Keep work bounded.** No additional IO or asymptotic increase in
  conversion work is introduced. Existing resource accounting and limits apply.

Changing which existing containers are provably irrelevant is intended. A
different plan or resource estimate is acceptable only with the same query and
source-authority semantics. Reverting the dependency pin restores the previous
optimization behavior without a data migration.

## Acceptance and evidence

| ID | Obligations | Situation and expected observation |
|---|---|---|
| AC-001 | REQ-001, REQ-003; INV-001, INV-002 | Dependency regression tests mix exact, absent, inexact, and missing-entry bounds, varying which container comes first. Minimum and maximum results retain exact entries and unknown positions with the correct type. Include integer, timestamp with timezone, string representations in use, and decimal bounds. The current revision must fail the mixed-evidence regression before correction. |
| AC-002 | REQ-002, REQ-004 | Empty, entirely unknown, and unknown-column cases return unavailable statistics without conversion warnings. For an all-null data column, existing exact typed-null bounds retain their type; unavailable bounds stay unavailable. An incompatible exact statistic remains conservative and diagnostic; the diagnostic does not mislabel max as min or disclose scalar contents. Assertions inspect behavior and diagnostics, not only a successful query. |
| AC-003 | REQ-002, REQ-005; INV-001, INV-002, INV-005 | A Bifrost seam fixture contains an excludable container, a matching container, and an unknown-bound container. Existing pruning evidence shows the first is excluded and the unknown-bound container stays eligible. Sizing and batch full-match behavior remain conservative. Empty live selections, including a pruned-away route set and an execution partition with no assigned routes, finish empty without conversion warnings or fragment dispatch. Compare results with the same fixture scanned without the statistics shortcut. Record bounded work and memory observations for all-known, mixed, and all-unknown cases; investigate material regressions rather than setting an invented throughput target. |
| AC-004 | REQ-006; INV-001, INV-003 | A real SDK-to-server Observe query journey covers time, numeric kind, and selected string predicates over real persisted and live sources, including negative and SQL-null cases. Results match the reference rows; source and tenant checks remain enforced. Reproduce the empty-route and mixed-bound warning triggers, show no legitimate-empty/unknown-evidence conversion warnings after correction, and state the limit of historical attribution. Use existing journey owners and source-proof coverage; do not create a new UI workflow. |
| AC-005 | INV-003, INV-004, INV-005 | Wyrd resolves the compatible corrected fork consistently across its patched DataFusion packages and lockfile. Required targeted Bifrost verification, format, lint, dependency, and tenancy checks pass. No source-authority checks or tests are weakened. |
| AC-006 | REQ-001–REQ-005; INV-001, INV-002 | A bounded review of adjacent statistics adapters and their consumers checks empty inputs, mixed precision, typed nulls, schema alignment, and parameterized types. Every additional claimed defect has a reproducer and consequence. Equivalent defects within this shared conversion contract are corrected; unrelated material behavior changes return for separate specification rather than expanding this change silently. |

## Why existing coverage missed it; related-risk assessment

The mixed-bound defect was caught and fixed: Wyrd commit `3b7f9d5af` pinned
fork revision `8d6d6bf58` and added a Wyrd mixed-nanosecond-bound regression.
Commit `07a157cb9` removed both in the broader TASK-008 rollback later on
October 29. The current source therefore lacks regression coverage that once
existed. The isolated fork correction differs from the current pin only in
the shared conversion file; restoring it does not authorize restoring the
broader rolled-back work.

At the current revision, common statistics tests exercise known integer bounds,
entirely missing column entries, and empty inputs. The empty-input test asserts
unavailable results without diagnostic silence. Wyrd's route tests cover
populated route selections, without asserting warning-free empty execution
partitions. Scalar conversion rejects mixed types as designed; the adapter
supplies them incorrectly. A result-only journey can pass both defects: an
empty selection remains empty, and conservative fallback over real containers
returns the same rows while reading more data.

The shared helper also logs minimum-statistic wording for maximum failures and
attempts array conversion for zero containers. Both are confirmed issues in
scope. Wyrd already guards zero-container calls in `BatchPruning`, but its
live-route adapter calls the dependency directly without that guard. The
existing typed-unknown fork fix still attempts conversion on empty inputs,
so restoring that dependency pin alone cannot eliminate the route warnings.
A bounded search found the untyped-null fallback in this helper within the
inspected common, pruning, and Parquet datasource source directories. This is
not evidence that all other statistics adapters are correct. Timestamp zones,
string representations, schema evolution, and false full-match proofs merit
the bounded acceptance review; they are risk cases, not confirmed additional
row-correctness bugs. No repository-wide correctness assurance is claimed.

## Material decisions and scope

The recommendation fixes all material choices: DataFusion owns empty-input
pruning, statistics conversion, and their diagnostic semantics; Wyrd supplies
source facts and consumes a compatible pinned correction; preserve conservative uncertainty and
source authority; require both pruning and row-result evidence; include the
adjacent empty-input and diagnostic defects; and bound related-bug discovery
to statistics conversion and its direct consumers. Generic file-format behavior belongs to Iceberg and Parquet; generic query,
statistics, and pruning behavior belongs to DataFusion. Wyrd owns integration
and Wyrd-specific semantics: tenant proofs, pinned cuts and live captures,
routing facts, and resource/admission policy. Do not add another Wyrd pruning
abstraction or extend its generic logic to compensate for these defects. This
change does not authorize wholesale relocation of existing generic code;
separate such work when existing dependency APIs cannot absorb it within this
correction's scope. Ordinary implementation mechanics remain open. No material product or architecture decision remains
open. The maintainer explicitly approved revision 3 on 2026-11-03 and requested
one task, executed independently from main in a new worktree.

The single [implementation task](tasks/TASK-001-dependency-owned-pruning.md)
covers dependency-owned empty-input and mixed-statistics corrections,
regression restoration and compatible pinning, and incident/verification closure. A published immutable corrected
dependency revision is a delivery dependency, not authorization to push or
publish it. The spec remains separate from TASK-004 UI acceptance.

## Authority and inspected evidence

- [Wyrd doctrine and Bifrost public ownership](../../../architecture/wyrd-design.md).
- [Oracle source authority, nested reads, and resource invariants](../../../architecture/bifrost-design.md).
- [Spec approval and comprehension rules](../../spec-driven-development.md).
- [Investigation and executable evidence](./investigation.md).
- [Contributor authority](../../../AGENTS.md) and [maintainer style](../../../agent_docs/maintainer-style.md).
- [DataFusion pins](../../../Cargo.toml): revision `19796a13ed46877cf998013886947b28a6f774b3`, version 55.1.0.
- Pinned fork `datafusion/common/src/pruning.rs`: `get_exact_column_statistics`,
  the minimum/maximum adapters, and their existing tests; `scalar/mod.rs`:
  `iter_to_array`; `pruning/src/file_pruner.rs`: per-file consumer. Local blame
  attributes the fallback and diagnostic to upstream commit `c6e5c91dff` from
  2025-05-30, rather than TASK-004.
- [Batch pruning and sizing](../../../crates/vala/vala-bifrost-redux/src/scan_sizing.rs),
  [live-route selection](../../../crates/vala/vala-bifrost-redux/src/oracle/live.rs),
  [live source](../../../crates/vala/vala-bifrost-redux/src/oracle/memtable_source.rs),
  [file bounds](../../../crates/vala/vala-bifrost-redux/src/oracle/exec.rs), and
  [batch statistics](../../../crates/vala/vala-bifrost-redux/src/scribe/memtable.rs).

## Revision history

- Revision 1, 2026-11-03 — Draft the shared conversion correction, conservative
  semantics, bounded related-risk review, and required incident closure evidence.
- Revision 2, 2026-11-03 — Add the reproduced empty-route defect, identify the
  removed fix and regression test, correct `kind`'s type, and record that the
  inspected persisted statistics are present. Status remains draft.
- Revision 3, 2026-11-03 — Apply the maintainer's ownership preference: both
  generic corrections belong in DataFusion; Wyrd owns integration and
  Wyrd-specific semantics. No new Wyrd empty-input workaround or pruning wrapper.
  Explicitly approved by the maintainer with the one-task planning request;
  execution starts independently from main, not the UI/TASK-004 branch.
