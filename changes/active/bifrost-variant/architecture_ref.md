# Bifrost Variant architecture reference

This reference explains the approved revision-10 architecture in plain terms.
The specification remains authoritative.

## Intended outcome

Bifrost stores fixed shapes as Struct and open shapes as standard Parquet
Variant. Struct fields and shredded Variant fields both receive nested leaf
projection, decoder filtering, and conservative statistics pruning, but they
do not pretend to have the same logical semantics.

- Struct access is exact DataFusion `get_field`.
- Variant access is semantic Arrow-backed `variant_get` with typed-leaf,
  residual, metadata, missing/null, conversion, and fallback behavior.
- DataFusion's shared physical nested-field machinery handles both.

## TASK-001 — Logical storage and query foundation

TASK-001 adds the canonical Variant type, fixed limits and failures, stable
fingerprint, built-in Variant/Struct schemas, and every named producer and
consumer. Every Oracle session registers the same `variant_get`, `parse_json`,
`try_parse_json`, and `to_json` behavior.

It also makes Bifrost fresh Iceberg-v3-only. Table creation, hidden row-lineage
preservation through repeated Forge rewrites, and v3 garbage collection land
together. There is no migration, mixed-version rollout, or public exposure of
Iceberg metadata columns.

The task uses the current workspace DataFusion source for semantic
`variant_get`, SQL lowering, registration, codec round trips, and full-root or
residual correctness. It does not own the physical DataFusion fork or repin.

## TASK-002 — User tables and atomic admission

TASK-002 lets Rust, Python, and TypeScript users declare and write Struct,
List, and Variant fields without language-specific durable logic.

Row insertion validates and converts the complete logical input before one
queue reservation. `write_batch(table, batch)` keeps its current public API,
calls authoritative `describe(table)` at the start of the async operation,
normalizes only fields declared Variant, and then attempts direct-send
admission. Describe or conversion failure mutates no queue, budget, or send
state. There is no cache, overload, or caller-supplied schema.

## TASK-003 — Per-file shredding and shared physical pushdown

### Write path

Scribe recovery-stage Parquet runs written by `encode_batch` remain
unshredded. Their exact logical schemas can be merged and restored after WAL
retirement without reconciling inferred physical layouts. No new staging
format or schema-union path exists.

The pinned iceberg-rust fork owns the one pure analyzer and standard Arrow
schema-application wrapper. Scribe calls them only from
`encode_ordered_claim`. Forge uses the fork's concrete deferred
`VariantParquetWriterBuilder` inside the existing rolling writer: every
`build(output)` starts with no Parquet encoder and fresh sample state, while
builder clones contain policy constants only. The five-field Forge handoff is
unchanged.

Each final Scribe hot object and each deferred Forge output independently:

1. retains a prefix ending at 4,096 rows or 67,108,864 bytes (64 MiB) of
   admitted Arrow backing memory, whichever arrives first;
2. stops before a later row or batch slice would cross the byte limit, except
   that an oversized first row is retained and fully charged for progress;
3. infers a physical shredding schema for that file;
4. opens the Parquet writer, replays the buffer once, and streams the rest; and
5. writes only the standard Parquet `metadata` / `value` / `typed_value`
   layout.

Each rollover starts again. Close analyzes a short buffer; empty output creates
no file. The existing writer memory measure charges retained Arrow buffers and
releases the charge exactly once on success, error, cancellation, or retry.
Forge's task reservation includes one 64-MiB prefix per concurrently open
output and re-infers independently for every 1-GiB target output. At each object
node, fields need 10% frequency, inference tracks at most 1,000 candidates,
keeps at most 300 by frequency and alphabetical tie-break, emits
alphabetically, and stops at depth 50. Integer and decimal widths widen within
their families. Arrays and incompatible values remain residual.

Arrow `ShreddedSchemaBuilder` and `shred_variant` apply the schema. There is no
top-128 rule, byte-ranking, Wyrd footer, summary merge, sidecar, spill, table-
wide shredded schema, or strategy abstraction. The row and memory limits are
internal tuning constants, not file-format or public contracts.

### Query path

```text
Struct syntax
  -> exact get_field
  -> struct_field_access()
                         \
                          -> DataFusion shared nested projection
                         /   -> decoder filtering
Variant syntax               -> conservative statistics pruning
  -> semantic variant_get
  -> required_input_fields(typed_value, residual value, metadata)
```

TASK-003 alone owns `https://github.com/bohmian-ai/datafusion`, based on
DataFusion v55 commit `d5552342012888b7d1a3ab88d92e3d292fc0cde0`, and the
workspace repin. Its narrow `datafusion-datasource-parquet` facade returns an
owned `PerFileParquetReadPlan` containing the leaf projection mask, projected
Arrow schema, optional decoder row filter, optional row-group and page
predicates, and a conservative-fallback reason. It never mutates or opens a
reader. `HotParquetExec` and the Iceberg `ArrowReader` apply this exact plan and
retain their existing metrics. Missing or invalid requirements return a
successful full-root/no-prune plan; genuine file schema/metadata errors remain
typed DataFusion errors. A compatible
shredded file can read only the necessary leaves; partial, incompatible, or
unshredded files use residual Variant evaluation. Whole-Variant reads are
unshredded before cross-file union; incompatible `typed_value` schemas are
never unioned table-wide.

Typed Variant statistics may skip a row group or page only when the matching
residual `value` is all-null and the predicate type matches. Otherwise the
engine evaluates `variant_get` normally. This is why Variant needs Struct
pushdown without becoming Struct extraction.

Distributed assignments remain unsigned. Peer mTLS authenticates the sender,
the canonical digest protects the assignment, and authorization occurs on the
logical source column before physical leaf binding or I/O. The actual
`wyrd.v1.proto` uses one `ScanLeafRef` oneof for top-level, Struct, and Variant
paths; `ScanPredicate` holds that leaf plus repeated literals and adds `IN`.
The sole protobuf/domain conversion enforces exact operator cardinalities,
non-empty paths, same-typed `IN` values, and schema-compatible leaf kinds.

## Performance outcome

The benchmark uses 262,144 rows and varying 8 KiB string siblings across
narrow, wide, and nested Struct plus equivalent shredded Variant data. Before
timing it proves exact results, the correct logical expression (`get_field` or
`variant_get`), equivalent physical field requirements, and zero reads of
unselected siblings. It reports optimized medians on whatever machine is
available; it has no host qualification, baseline, speedup, or absolute timing
gate.

## Delivery order

TASK-001 establishes logical contracts and safe v3 storage on the current
DataFusion source.
TASK-002 adds user-facing writes after TASK-001. TASK-003 adds per-output-file
shredding at final-object boundaries through the fork-owned deferred writer,
the complete shared physical pushdown path, the one DataFusion fork/repin, and
the protobuf leaf predicate after
TASK-001; it can proceed alongside TASK-002. Only TASK-003 runs
`mise run verify:bifrost`, once, as the last command after all three tasks are
integrated.
