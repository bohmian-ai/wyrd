# SPEC-bifrost-variant revision 26: Variant is a DataFusion feature

Status: approved by the user on 2026-10-08 ("revise the spec and update the
plan. This is your new objective and required implementation. We want a fully
self-contained Variant type implementation in datafusion that supports struct
pushdown like the proposed plan outlines. The goal is to add this as a new
feature that will eventually become the hardened variant feature in core
datafusion. It should also work with datafusion-distributed … Wyrd only needs
to turn the feature on to leverage its performance improvements").

Kept apart from `spec.md` for the same reason as revisions 20–25. It is
applied after them (frontmatter `revision: 26`).

Numbering note: `spec.md`'s history already holds a "Revision 21 (one read
path)" entry, and `spec-revision-21-sampling.md` is a different revision 21.
Both stay as approved; this file refers to them as "revision 21 (one read
path)" and "revision 21 (sampling)".

## Why

Variant querying is DataFusion logic: reading a Parquet Variant, converting a
shredded file to the table's Variant, path functions, leaf projection through
Structs and Lists, `unnest`, and pruning. Wyrd wrote parts of it (the Variant
SQL functions, a Variant file adapter, a `FileRead` UDF, its own leaf-path
pushdown), which made Wyrd a second DataFusion and forced a custom codec for
distributed plans. The user is the DataFusion fork's maintainer for this
work. Variant becomes one self-contained, upstream-shaped DataFusion feature;
Wyrd turns it on and uses it.

## Changes to `spec.md`

### Scope

Replace "Variant SQL operators and functions in every Oracle query session."
and the "DataFusion shared nested-field projection …" bullet with:

> - A self-contained Variant feature in the DataFusion fork (`variant` cargo
>   feature on `datafusion`), shaped to become DataFusion's own Variant
>   support: type, functions, reading, leaf projection, `unnest`, pruning, and
>   distributed execution. Wyrd enables the feature and uses it.

### Non-goals

Delete "Moving the published read path to DataFusion's `ParquetSource`"
(revision 21 (one read path) already did it).

Replace "Upstreaming changes to DataFusion, iceberg-rust, or
compaction-core; fork changes stay on the pinned forks." with:

> - Opening upstream DataFusion pull requests in this change. The Variant
>   feature is written as an upstream feature (no Wyrd names, types, or
>   behavior), but it ships on the pinned fork. iceberg-rust and
>   compaction-core changes stay on their pinned forks.

### New section: DataFusion Variant feature (replaces "Semantic Variant access on shared nested-field pushdown")

Replace the whole section with:

> **Type.** A Variant is the Arrow extension type `arrow.parquet.variant`: a
> Struct of `metadata` and `value` binaries, optionally with `typed_value`
> when shredded. A table declares the canonical (unshredded) Variant; a file
> may hold any standard shredded layout.
>
> **Crate and feature.** A new in-tree crate, `datafusion-functions-variant`,
> owns every Variant function and planner rule. The `datafusion` crate gains
> a `variant` feature that depends on it and on parquet's
> `variant_experimental` feature. With the feature on, a default
> `SessionState` registers the Variant functions and expression planner;
> with it off, nothing Variant-related is compiled or registered.
>
> **Functions.** `variant_get(v, path)`, `variant_get_text(v, path)`,
> `parse_json(text)`, `try_parse_json(text)`, `to_json(v)`, and an
> `ExprPlanner` that lowers `v -> k` to `variant_get` and `v ->> k` to
> `variant_get_text` (REQ-017 semantics). They are ordinary
> `ScalarUDF`s: stateless, resolved by name, with no session state.
>
> **Reading.** DataFusion's default `PhysicalExprAdapter` converts a file's
> Variant to the table's Variant at any Struct or List depth: it validates the
> bytes, then calls arrow-rs `unshred_variant`. A value under a null parent is
> null. Malformed bytes raise a DataFusion error, never a panic. Validation is
> arrow-rs's full Variant validation, including its nesting limit; DataFusion
> adds no Variant limit or config option of its own. Bifrost's stricter
> limits are enforced when it writes.
>
> **Leaf projection.** The adapter's existing cast pushdown (upstream
> `#24109`) moves that conversion onto the narrowest value read, including
> through List element access and `array_transform` lambdas. `variant_get`
> and `variant_get_text` declare, through `required_input_fields`, that they
> accept a Variant in any layout and which leaves they read: the path's
> `typed_value` leaf, each path-local residual `value`, and `metadata`. The
> adapter then drops the conversion under them, and the Parquet reader
> decodes only those leaves, through Struct and List chains. Struct field
> access is DataFusion's existing `get_field` pushdown, unchanged.
>
> **`unnest`.** The logical optimizer passes a leaf expression on an `unnest`
> output through the `Unnest` as `array_transform(list, x -> expr)`, only
> when that reads fewer leaves than the whole element. Otherwise `Unnest`
> stays a barrier.
>
> **Pruning.** The Variant crate rewrites a typed comparison on a shredded
> path (for example `v ->> 'k' = 'x'`) per file into a comparison on the
> shredded leaf, guarded by the residual being all-null (see "Safe
> pruning"). Row-group, page, and decoder filtering then use DataFusion's
> existing machinery. Paths through a List are projection-only: no
> statistics pruning.
>
> **Distributed.** Nothing extra crosses the wire. The functions travel by
> name through `datafusion-proto`'s default UDF encoding, and the Variant
> read is part of the default adapter every `ParquetSource` already uses.
> `datafusion-distributed` needs no Variant codec, adapter codec, or worker
> hook.
>
> **Generic pieces.** The mechanisms that are not Variant-specific (List
> element access in cast pushdown and read planning, lambda element access,
> `unnest` pushthrough) live in their owning DataFusion crates and know
> nothing about Variant. Each has its own DataFusion tests.

Delete the `PerFileParquetReadPlanner` facade text and diagram only where it
describes Wyrd calling it; the facade stays a DataFusion internal used by
`ParquetSource`.

Replace the "One immutable workspace-level `[patch.crates-io]` …" paragraph's
list of eight crates with:

> every DataFusion crate the workspace resolves, including
> `datafusion-functions-variant`, all at the same fork revision.

### Oracle registration and distributed wire

Replace the first paragraph with:

> Every Oracle session (leader, admission, follower, analytical, worker) is
> built from a DataFusion `SessionState` with the `variant` feature on, so
> the Variant functions and planner are present before any planning, plan
> decode, or execution. Oracle registers no Variant function, planner,
> adapter, or codec of its own. `ORACLE_VARIANT_SQL_VERSION` is deleted; the
> peer contract fingerprint includes the DataFusion fork revision, and a
> worker on a different revision refuses before decoding a plan.

In the paragraph listing what crosses the wire, delete the Variant file
adapter bullet (the adapter-factory codec hook and the `FileRead` UDF). Only
the tenant footer proof remains. Replace its bullet with:

> - the tenant footer proof, reattached by a `datafusion-distributed`
>   `WorkerPlanRewriteHandler`. The proof refuses any file whose footer does
>   not name the authenticated tenant before a row is decoded.

The footer proof is the tenant boundary. It checks the file's own contents,
which is a stronger check than the path, so no separate path-prefix check is
required.

### Safe pruning and hidden row lineage

Replace "Otherwise Oracle evaluates the semantic `variant_get` with residual
and metadata inputs." with:

> Otherwise DataFusion evaluates `variant_get` with residual and metadata
> inputs.

### Nested-field performance evidence

Append:

> **Latency requirements (revision 26).** Measured with
> `mise run bench:bifrost:reads` and the fork's own benchmarks, before and
> after the feature, on the same machine, p50 and p95:
>
> | Query kind | Planning latency | Execution and read latency |
> |---|---|---|
> | No Variant column | unchanged | unchanged |
> | Struct field | unchanged | unchanged versus today's Struct pushdown, which already reads only the selected leaves |
> | Variant, unshredded file | unchanged | unchanged |
> | Variant, shredded file | unchanged | dramatically lower than the same query on the unshredded file |
>
> "Unchanged" means within run-to-run noise of the before value. The
> shredded improvement is shown by the after-state numbers side by side with
> the unshredded numbers for the same query, plus read metrics showing only
> the declared leaves were decoded. Results are reported as p50/p95 values,
> not ratios.

### REQ-017 — Operators and functions

Replace "Every Oracle query session (…) provides:" with "Every Oracle query
session provides, from DataFusion's `variant` feature:". Replace the last
paragraph's "lower to the semantic `variant_get` UDF backed by Arrow-rs" with
"lower to DataFusion's `variant_get`, backed by arrow-rs".

### REQ-019 — Stable failures

Amended by explicit human direction on 2026-10-08 (choice "DataFusion's
errors"). Replace the first paragraph with:

> Invalid JSON, a numeric value no Variant type can hold, a Variant nested
> beyond depth 64, and a Variant exceeding 8,388,608 encoded bytes fail
> with stable catalog codes before acknowledgement on writes. They are never
> truncated or stored partially. Queries use DataFusion's Variant feature
> as-is: its rules, limits, and errors are DataFusion's.

The second paragraph (late terminals keep the catalog error) is unchanged.

### REQ-022 — Shredded files are read correctly everywhere

Append:

> The conversion from a file's layout to the table's Variant is DataFusion's,
> at any depth, and a malformed file is an error, never a panic or a wrong
> value.

### REQ-023 — Leaf projection

Replace with:

> A Struct field is DataFusion's `get_field`. A literal Variant path is
> DataFusion's `variant_get`, which declares the leaves it reads. DataFusion
> decodes only those leaves, through Structs, List element access, and
> `unnest`. Unshredded, missing, or incompatible layouts read the whole
> Variant and get the same result.

### REQ-024 — Leaf filtering and row-group skipping

Replace "DataFusion's shared nested-field pipeline handles both logical
forms" with "DataFusion's Variant feature and its existing Struct pushdown
handle both forms". Append: "Predicates on paths through a List are evaluated
after a leaf-only read, without statistics pruning."

### REQ-025 — One read path for every persisted scan

Replace "with the Variant adapter carried by the fork's codec hook and the
tenant footer proof reattached before execution" with "with the tenant footer
proof reattached before execution; Variant needs nothing on the wire".

### INV-008 — One nested-field pushdown pipeline

Replace with:

> All Variant querying is DataFusion's `variant` feature. Wyrd adds no
> Variant function, planner rule, adapter, reader, pruner, codec, or leaf-path
> logic. Struct pushdown is DataFusion's, unchanged.

### Material constraints

- Replace "and Variant query kernels in `vala-bifrost-redux`" with "; no
  Variant query logic in `vala-bifrost-redux`. Logic that is not
  Wyrd-specific and would need to change DataFusion, arrow-rs or
  `datafusion-distributed` internals belongs in those forks. Wyrd-specific
  logic, including Bifrost's write path calling the public `parquet-variant*`
  APIs, stays in Wyrd".
- Replace the "exact eight `[patch.crates-io]` entries" text with "every
  DataFusion crate the workspace resolves, including
  `datafusion-functions-variant`".
- In the Iceberg fork bullet, delete the reader sentences ("The reader
  validates … `typed_value` schemas."). Oracle reads published files through
  `ParquetSource`; the Iceberg reader is not on its path.
- Add: "Wyrd keeps only the write side (REQ-020 shredding choice and the
  Bifrost write limits) and the `variant` feature flag. The
  fork's adapter-factory codec hook from revision 21 (one read path) is
  deleted."

### Required system boundaries and flow

Replace the last four lines before "SDK result terminals" with:

```text
  -> DataFusion `variant` feature: get_field / variant_get declare leaves
  -> default adapter converts layouts; reader decodes declared leaves
  -> decoder filter, conservative pruning (DataFusion)
```

### AC-007 — Leaf reads and pruning

Replace "while both produce the same physical nested-field requirements and
the same DataFusion per-file facade outputs …" through "expression." with:

> and that DataFusion reads only the declared leaves through Structs, List
> element access, and `unnest`, on hot, published, and distributed scans.
> The distributed case uses no Wyrd codec. DataFusion's own tests in the fork
> cover each generic piece and the Variant crate (conversion, malformed
> input, null parents, leaf projection, pruning, proto round trip).

Append: "The revision-26 latency table is met and reported as p50/p95."

### AC-010 — Verification

Append: "The fork's tests for every changed DataFusion crate pass, with and
without the `variant` feature."

### Revision history

Prepend:

> - **Revision 26 (2026-10-08, approved):** By explicit human direction,
>   Variant querying is one self-contained `variant` feature in the
>   DataFusion fork, shaped for upstream: the `datafusion-functions-variant`
>   crate (functions, `->`/`->>` planner, typed pruning rewrite), layout
>   conversion in the default adapter, leaf reads through Struct, List and
>   `unnest`, and distributed execution with no custom codec. Wyrd only
>   enables it. Deletes Wyrd's Variant SQL module, leaf-path pushdown, file
>   adapter, `FileRead`, adapter codec, and `ORACLE_VARIANT_SQL_VERSION`.
>   Adds four latency requirements. Bifrost limits and catalog codes apply
  to writes only; queries use DataFusion's Variant feature as-is (REQ-019,
  amended 2026-10-08). Supersedes the parts of revision 21 (one
>   read path) and revision 23 that put Variant logic or its wire handling in
>   Wyrd.
