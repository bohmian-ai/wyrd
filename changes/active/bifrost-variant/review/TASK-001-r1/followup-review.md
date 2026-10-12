# Focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Conflicts investigated: `MR-001` and `BVR-BEH-002` only

The candidate remained at the stated commit throughout this follow-up.

## Source paths inspected

- `crates/vala/vala-bifrost-redux/src/oracle/{mod.rs,variant_sql.rs}`
- `crates/shared/wyrd-client/src/error.rs`
- `crates/wyrd-spec/src/vala/error.rs`
- pinned `datafusion-distributed` revision `4cfa166d233207ee188a277a7849bee7d9dfd2de`, especially
  `src/protocol/grpc/errors/{mod.rs,datafusion_error.rs}`,
  `src/protocol/grpc/{worker_service.rs,worker_client.rs}`, and
  `tests/error_propagation.rs`
- `crates/vala/vala-bifrost-redux/src/forge/{publication.rs,worker.rs}`
- `crates/vala/vala-bifrost-redux/src/forge/managed/{executor.rs,handoff.rs}`
- pinned `iceberg-compaction` revision
  `94db7b94f72c48c75c937d36a59e80c237e8ca72`, especially
  `core/src/executor/datafusion/{mod.rs,datafusion_processor.rs,iceberg_file_task_scan.rs}`
  and `core/src/compaction/mod.rs`
- `changes/active/bifrost-variant/{spec.md,tasks/TASK-001-variant-storage-and-query.md}`
- `architecture/references/domain/{datafusion,iceberg}.md`,
  `architecture/references/languages/{errors,maintainer-style}.md`, and
  `architecture/bifrost-design.md`
- the five conflicting discovery reports named in the assignment

Primary external grounding:

- [Apache Iceberg table specification, Row Lineage and Field-level Metrics](https://iceberg.apache.org/spec/#row-lineage)
- [Apache Iceberg Flink row-lineage rewrite implementation, PR 14149](https://github.com/apache/iceberg/pull/14149/files)
- [Pinned datafusion-distributed source](https://github.com/datafusion-contrib/datafusion-distributed/tree/4cfa166d233207ee188a277a7849bee7d9dfd2de)

## Conflict 1: distributed Variant error identity

### Reachable producer-to-terminal path

1. `ParseJson::invoke_with_args` converts a strict parse failure into
   `DataFusionError::External(Box<BifrostError>)` at
   `oracle/variant_sql.rs:573-600`.
2. Interactive execution keeps the concrete error in the local source chain;
   `variant_query_error` downcasts it back to `BifrostError` at
   `oracle/mod.rs:4278-4294`.
3. Analytical execution crosses the pinned dependency's gRPC worker stream.
   The dependency does use standard gRPC status details to carry a structured
   `DataFusionErrorProto`, but its `External` protobuf arm is only a `string`.
   `DataFusionErrorProto::from_datafusion_error` calls `err.to_string()`, and
   decoding constructs `DistributedDataFusionGenericError { message }`.
   The original Rust error type and source chain are irretrievably absent.
4. The coordinator therefore reaches `variant_query_error` with only the
   `BifrostError` human `Display` text. Candidate `remote_variant_error`
   recognizes exact English fragments and reconstructs the public catalog
   variant before the existing HTTP/gRPC/SDK projection.

The dependency constraint claimed by the Oracle domain review is real. There
is no public custom-error field or codec hook in this pinned revision that can
carry `BifrostError` as typed metadata. Adding one would change and repin the
dependency-owned protobuf/error protocol, which TASK-001 expressly excludes.

That constraint does not make reparsing human `Display` prose an established
mechanism. `remote_variant_error` duplicates the derive-backed fields as a
second delimiter grammar (`" row "`, `", limit "`, `" at "`) and its unit
test fixes that prose as the internal wire representation. This conflicts with
the repository's typed/serialized error source of truth and with the standing
direction. `MR-001` is therefore **confirmed as DRIFT**, while its proposed
correction needs narrowing.

Deleting the parser without replacement is not valid: the Analytical branch
of `published::variant_sql_registry_covers_every_session` would regress from
`WYRD_VALA_400_VARIANT_INVALID_JSON` to
`WYRD_VALA_500_QUERY_EXECUTION_FAILED`, violating REQ-019 and TASK-001's
explicit same-error-identity proof.

The smallest correction does not require a new public API, protobuf field,
dependency repin, or second error model. Reuse the existing serde representation
already derived on `BifrostError` inside the dependency's existing
`External(String)` carrier: the local wrapper can retain the typed
`BifrostError` in its source chain while rendering its serialized form for the
remote hop, and the coordinator can deserialize that form. This is a private,
reversible projection through the only carrier the pinned dependency exposes;
it removes the prose grammar and keeps the existing public catalog. The
closure proof should exercise both the local downcast and the real Analytical
journey while allowing `Display` wording to change independently.

### Resolution

- `maintainer-review.md` is correct that the prose parser is non-standard
  drift.
- `domain-review-oracle-datafusion.md` is correct that the pinned dependency
  reduces `External` errors to a string and that deleting reconstruction would
  violate the task.
- The maintainer remediation is **revised**: structured transport cannot be
  added to the dependency in TASK-001; use `BifrostError`'s existing serde form
  through the existing string arm and delete only the prose grammar.

## Conflict 2: Forge lineage validation

### Producer-to-commit path

1. The pinned compaction fork adds the two reserved Iceberg metadata fields to
   the internal scan and writer schemas in
   `datafusion_processor.rs:867-940`; `iceberg_file_task_scan.rs` resolves them
   by reserved field ID.
2. Each output batch reaches `collect_row_lineage` before
   `DataFileWriter::write`. The helper refuses a missing, non-`Int64`, or null
   `_row_id`/`_last_updated_sequence_number`, then retains every `_row_id`.
3. After all writer tasks drain, `ensure_unique_row_ids` merges the retained
   IDs, sorts them, and rejects a duplicate before returning the output
   `DataFile`s.
4. Wyrd passes those descriptors unchanged through the five-field
   `RewriteHandoff`. `RewriteCommitRequest::derive` then requires each output's
   `value_counts` to equal `record_count` and `null_value_counts` to equal zero
   for both reserved field IDs.
5. Only after that gate does `RewriteCommitRequest::encode` add the files to
   the native Iceberg rewrite transaction, and `commit_rewrite` submits the
   single catalog commit.

### Standard and comparable implementation evidence

Iceberg v3 requires every existing row moved to another data file to copy its
non-null `_row_id`; an unmodified row must also copy its existing non-null
`_last_updated_sequence_number`. `_row_id` is table-unique. These are required
row values, so the internal projection and the bounded per-batch
presence/type/null refusal directly guard required writer input.

Iceberg separately defines `value_counts` and `null_value_counts` as optional
field metrics. A missing map or field entry means missing statistics, not a
missing physical column. Consequently the publication metric gate can reject a
standards-compliant output, and complete counts still cannot prove that the
values equal the input lineage. It is not valid lineage evidence.

The comparable Apache Iceberg Flink implementation in PR 14149 follows the
ordinary mechanism: it extends the projected/read/write schema with the two
metadata columns, copies rows through the existing reader and writer, and
tests the resulting lineage values. It adds neither a publication-time metrics
requirement nor a rewrite-wide uniqueness collection/sort.

### Check classification

- **Required:** projecting both reserved fields by Iceberg field identity,
  writing them unchanged, and refusing missing, mistyped, null, or otherwise
  unwritable lineage before returning a handoff. This is the source-side
  guard for the approved fresh-v3-only contract.
- **Invalid under the standing direction:**
  `ensure_row_lineage_evidence` in `forge/publication.rs`. Optional metrics are
  neither physical-column proof nor exact-value proof; the standard and the
  comparable Apache implementation do not make them a commit prerequisite.
- **Redundant drift under the standing direction:** retaining every row ID and
  sorting the entire rewrite in the compaction fork. A valid v3 input already
  has unique row IDs, and the standard one-to-one read/write projection copies
  those exact IDs. Apache's comparable rewrite relies on that mechanism and
  verifies the observable result rather than adding an `O(n)` retained set and
  `O(n log n)` pre-commit proof. The revision-10 sentence that duplicate
  lineage fails does not justify a novel rewrite-wide verifier: with the
  required exact-copy path, a duplicate is not produced from valid input; the
  repeated-rewrite equality journey is the task's direct closure proof.

`BVR-BEH-002` is therefore **confirmed**. The domain and system reports correctly
trace fail-before-commit placement, but incorrectly treat optional metrics and
the rewrite-wide scan as standard lineage mechanisms. The minimal correction
is the behavior review's one: keep field-ID projection, bounded batch
validation, unchanged writing, and the before/after lineage journey; delete
the publication metrics gate and rewrite-wide ID retention/sort. Add no
replacement checker, metric requirement, option, or setting.

## New proposed findings

None. This follow-up resolves the assigned conflicts and does not broaden the
finding union.

## Overall result

**RESOLVED**
