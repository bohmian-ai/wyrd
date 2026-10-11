# Scribe ingest, admission, and durability domain review

## Reviewed boundary

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation evidence: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

CodeGraph was used first to locate `decode_rows`,
`enforce_builtin_source_contract`, every `BuiltinTableDefinition` validator,
the native IPC material and fixed-IPC owners, and the WAL/ACK continuation.
The cumulative base-to-candidate source, the round-5 remediation range, the
applicable Bifrost/Arrow/reliability authorities, the pinned `parquet-variant`
59.3 validation behavior, and the recorded round-5 proof were then inspected.

## Authority and source coverage

| Concern | Source evidence | Result |
|---|---|---|
| One pre-durability admission seam | Canonical batches go through `ScribeIngressCpuPool::decode` and `decode_rows` in `scribe/ingress.rs:329-393`. Raw Arrow IPC is retained as `NativeAdmittedRows`; each decoded batch goes through `stamp_native_source` -> `decode_native_batch` -> the same `decode_rows` in `scribe/preprocess.rs:220-275,335-405` and `scribe/execution_lanes.rs:454-577`. `prepare_rows` exhausts and validates every native source into a local slice set before `PreparedAppend` can be returned, charged, or sent to a shard (`scribe/preprocess.rs:573-630`; `scribe/ingress.rs:552-619`). An invalid later IPC batch therefore drops earlier local slices rather than partially dispatching them. | **PASS** |
| Every built-in reaches its owner validator | `ScribeImpl::builtin_definition` resolves all server-owned built-ins, `BUILTIN_TABLES` binds each of the eleven definitions to `DomainTable::CANONICAL_VALIDATOR`, and `decode_rows` calls `enforce_builtin_source_contract` whenever that definition is present (`scribe/ingress.rs:498-519`; `tables/mod.rs:1087-1120`; `scribe/execution_lanes.rs:533-621`). No built-in has a sibling ingest path that bypasses this seam. | **PASS** |
| Locked schema/value order | `decode_rows` first rejects duplicate/reserved managed names. The table validator then rejects undeclared fields and establishes declared shape before any Variant byte walk; canonical ledgers bind by name and reassemble in declaration order, while predeclared tables leave shape drift to the following fingerprint check. Variant extension/value checks occur before semantic Struct checks, and all table checks occur before source fingerprinting, scope validation, stamping, preprocessing, WAL, or ACK (`execution_lanes.rs:537-577,601-634`; `tables/mod.rs:197-337`; `tables/signal.rs:880-952`). This preserves revision 13's envelope/schema/size-encoding-numeric-depth precedence rather than letting hostile values hide an earlier schema refusal. | **PASS** |
| Semantic nullable Structs | `validate_predeclared` invokes `refuse_partial_structs` after schema and Variant validation for `ResultsTable::{drift_report,eval_summary}` and `CallsTable::resolved_model`; `validate_metric_points` invokes it for both exponential bucket Structs. The helper compares every child's physical validity with the parent per row and returns `BifrostError::SchemaParse` on either partial-present or absent-parent/retained-child state (`tables/mod.rs:256-337,479-485`; `tables/verification/results.rs:75-84`; `tables/gateway/calls.rs:81-88`; `tables/metrics/projection.rs:290-336`). Required `requested_model` retains non-null children, so it cannot acquire the optional model's wider state. | **PASS** |
| Recursive Variant wire and semantic domain | `validate_declared_variants` walks every declared top-level and nested Variant row-first and field-in-logical-order, requires the canonical extension/wire layout, and calls `EncodedVariant::from_bytes` for each present cell (`tables/mod.rs:197-253,339-432`). `from_bytes` checks size first, performs a bounded explicit-stack scan, records malformed encoding, noncanonical Decimal16, and depth, then selects malformed -> numeric -> depth. It calls upstream full validation for accepted-depth input; for hostile depth its scan covers every reachable node, validates metadata/name order through the installed dependency's accessors, contains malformed-access panic, and refuses shared-byte object explosions once visits exceed the value-byte bound (`wyrd-queue/src/variant.rs:210-250,648-806,876-959`). Canonical scale-zero `i64::MAX + 1..=u64::MAX` is the only admitted Decimal16 domain. | **PASS** |
| Native IPC preflight and decode | `material_plan` bounds schema depth/node/source counts, validates V5 framing, body ranges, validity counts, buffer cardinality and offsets, and projected material before root admission. Its ancestor-Struct mask permits a required child null only where a nullable enclosing Struct may mask it; `StreamDecoder::new().with_require_alignment(true)` retains Arrow's ordinary full array validation, so exact child/ancestor bitmap containment is checked before the table validator (`scribe/material_plan.rs:156-195,353-486,850-930`; `scribe/preprocess.rs:184-205,305-381`). This structural preflight does not replace or bypass semantic Struct/Variant admission. | **PASS** |
| Fixed IPC and WAL boundary | Only validated/stamped rows reach `prepare_slice`. `FixedIpcPlan::count` and `encode` share `visit_nodes`; it rejects slices, type drift, unmasked required-child nulls, list-position-space mistakes, and any count/write divergence before producing the exact-capacity WAL payload (`scribe/preprocess.rs:573-630,777-855`; `scribe/fixed_ipc.rs:77-127,193-423,1252-1301`). Thus nullable-child persistence support does not weaken fixed IPC, and no semantic refusal is discovered after a WAL append. | **PASS** |
| WAL, visible insertion, and ACK order | Shard work begins only after the complete prepared set is validated. `ShardOwner::process_group` writes and syncs slices, appends and syncs batch COMMIT records/control fences, inserts committed rows into active authority, and only then calls `acknowledge_visible`; failures notify the waiter or retain ambiguous committed ownership without reporting success (`scribe/shards.rs:3071-3157,3570-3695`). This matches the acknowledged boundary in the Bifrost reliability authority. | **PASS** |
| Refusal cleanup and continuation | Any contract failure before `AdmittedAppend` construction returns directly and drops the reservation/memory owners; a failure while native sources are materialized notifies the waiter and drops the complete local slice set before shard dispatch (`scribe/ingress.rs:574-616`; `scribe/preprocess.rs:662-713`). The recorded real-server journey `verification_runtime::builtin_variant_columns_are_refused_before_ack` sends malformed/deep/shared-object/noncanonical-decimal and partial-metric frames, then successfully writes valid trace/span/metric frames and verifies only the accepted identities are readable. `typed_builtin_payloads_are_queryable` similarly refuses a partial Eval summary before the intact write. | **PASS** |

## Findings

### Critical

None.

### Important

None.

### Suggestions

None. The existing validator seam is the smallest correct owner; another
validated-batch wrapper or downstream guard would duplicate it.

## Verification limits

No Cargo, `mise`, code-generation, or database-backed tests were started in
this review because the shared-checkout coordinator reserved those jobs. I
reviewed the round-5 implementation record showing the focused queue/table
tests, Scribe/Oracle tests, both named real-server journeys, cross-language
journeys, formatting, lints, code generation, documentation, and
`git diff --check` passing. Gateway `resolved_model` remains unit-level because
the only writer is the in-process gateway capture principal; source tracing
shows that its batch still enters the same `decode_rows` validator seam.

The cumulative and remediation-range `git diff --check` commands were run
read-only and passed. The immutable candidate and tree were rechecked after
report creation.

## Overall status

**PASS** — every reachable built-in raw Arrow/canonical path crosses the one
table-owned validation seam; partial semantic Structs and invalid Variant
states receive their catalogued pre-WAL/pre-ACK refusals in the required order;
native material planning and fixed IPC preserve rather than bypass that
boundary; and recorded continuation proof shows a refusal neither persists a
row nor poisons the following valid write.
