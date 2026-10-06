# TASK-001 System-Resilience Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Remediation range: `bb6ae8070..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved authority: `SPEC-bifrost-variant` revision 12 and original `TASK-001`

The supplied candidate resolves to the supplied tree. The checkout was still at
that commit and tree after the focused review checks. This review used the
complete base-to-candidate change and then isolated the remediation range to
identify newly changed failure boundaries.

## Deployed-Path Evidence

| Changed runtime path | Deployed ownership and propagation | Affected capabilities |
|---|---|---|
| JSON-row Variant preparation | `BatchBuilder` retains each top-level field's raw JSON token so integer text is not first narrowed through `serde_json::Value`; the Variant column alone calls `EncodedVariant::from_json_text` (`crates/shared/wyrd-queue/src/batch_builder.rs:96-123`, `212-263`, `349-363`). `append_raw` bounds recursive construction at 64 containers and retains a depth refusal while checking eligible siblings for the higher-priority numeric refusal (`crates/shared/wyrd-queue/src/variant.rs:601-685`). A refusal remains inside the bounded client producer and no request reaches the server. | Rust/Python/TypeScript buffered row insertion and every producer using the shared queue. |
| Raw Arrow Variant admission | A declared Variant reaches the table-owned recursive validator, which reads its `metadata`/`value` children and calls `EncodedVariant::from_bytes` (`crates/vala/vala-bifrost-redux/src/tables/mod.rs:217-253`, `334-390`). Size is checked before the shallow depth walk; malformed shallow-access panics are contained and mapped to `InvalidJson`, and upstream full validation runs only after the value is known to be at most 64 containers deep (`crates/shared/wyrd-queue/src/variant.rs:167-197`, `752-790`). Scribe runs this work on its bounded ingress CPU lane before fixed IPC preparation, shard dispatch, WAL append, or ACK (`crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:521-627`). | Canonical Arrow ingestion for built-ins, including OTLP-equivalent signal writes, verification publication, gateway capture, agent traces, and audit publication. |
| Schema-first built-in refusal | `validate_predeclared` rejects undeclared columns, withholds Variant traversal when the complete declared shape differs, and lets the existing fingerprint fence reject that mismatch; only an exact shape reaches Variant validation (`crates/vala/vala-bifrost-redux/src/tables/mod.rs:256-313`). The shared `is_variant` owner requires the exact extension name and empty/absent extension metadata (`crates/shared/wyrd-queue/src/variant.rs:428-442`). | All eleven predeclared built-in tables at the Scribe trust boundary. |
| Nullable nested built-in values and WAL encoding | Verification summary/report children, metric exponential-bucket children, and gateway `resolved_model` children are nullable, and their producers emit actual null child slots beneath absent parents (`crates/vala/vala-bifrost-redux/src/tables/verification/results.rs:47-74`; `crates/wyrd/wyrd-server/src/verification/results.rs:692-744`; `crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs:31-60`; `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs:29-47`). `FixedIpcPlan` permits a required child null only when the union of enclosing Struct validity masks that row, and rejects an unmasked null before encoding (`crates/vala/vala-bifrost-redux/src/scribe/fixed_ipc.rs:270-360`). `prepare_slice` completes this checked fixed-capacity encoding before the append becomes WAL/shard work (`crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:772-835`). | Verification-result publication, OTLP metrics, gateway captures, Scribe WAL/replay, hot publication, and subsequent Oracle field projection. |
| Oracle Variant execution and late errors | Every Variant function masks empty parent placeholders and fully validates each present stored cell before upstream decoding (`crates/shared/wyrd-queue/src/variant.rs:339-391`; `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:1-677`). Catalogued local or worker failures continue through `QueryCatalogError`; the coordinator reconstructs their structured identity and only uncatalogued failures become `QueryExecutionFailed` (`crates/vala/vala-bifrost-redux/src/oracle/exec.rs:1208-1217`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4239-4306`). The terminal path still cancels and settles descendants before releasing read/admission ownership, and shared client collection returns no partial result on failure (`crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:647-805`; `crates/shared/wyrd-client/src/bifrost/query.rs:775-825`). | Interactive and distributed Oracle queries over HTTP, gRPC, MCP, CLI, scheduled verification reads, and all three SDKs. |
| Iceberg v3 and Forge | The remediation range does not alter the publication, lease, operation, output-generation, or reconciliation state machines. Cumulatively, table creation selects v3 and physical validation refuses any other format (`crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1045-1101`); the existing Forge journey exercises repeated lineage-preserving rewrites and v3 cleanup (`crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1470-1660`). | Every built-in and user table, compaction, manifest maintenance, snapshot expiry, and object cleanup. |
| Public result rendering | Row-as-JSON surfaces share `VariantJsonEncoderFactory`, which masks placeholders and validates stored cells before rendering (`crates/shared/wyrd-queue/src/variant.rs:280-337`). The compiled CLI journey now covers native object output, exact `u64::MAX`, and `3.0` spelling (`crates/wyrd/wyrd-cli/tests/query_server_journey.rs:78-192`). | CLI plus the shared HTTP/MCP/SDK JSON result behavior. |

## Failure-Path and Recovery Assessment

| Failure or interruption | Observed boundary and recovery behavior | Assessment |
|---|---|---|
| Hostile raw Variant nested far beyond the limit | The raw Arrow value is already bounded by the ingest envelope and the fixed 8 MiB Variant ceiling. `from_bytes` copies only that bounded value, stops its shallow walk at depth 65, contains malformed-access unwinds, and returns a catalogued request error. Scribe drops the admitted RAII reservations before shard handoff; there is no WAL frame, batch fence, ACK, process crash, or state requiring recovery. The journey evidence sends a 20,000-level compact encoding and then successfully writes a following batch, proving the process remains usable. | PASS |
| Malformed or foreign-extension raw Arrow | Foreign extension metadata is refused as `UnsupportedType`; malformed bytes become `VariantInvalidJson`. Both happen in table-owned validation before WAL/ACK. The failure is scoped to the request and does not poison the ingress worker, shard, or shared server. | PASS |
| Postgres unavailable during admission | Scribe obtains the catalog binding and database receipt instant before payload preprocessing and dispatch (`scribe/ingress.rs:408-450`, `453-482`). Dependency failure releases local owners and fails only the append; restoration plus retry under the stable batch identity resumes normal service. | PASS |
| Cancellation around Scribe dispatch | Before dispatch, reservations and prepared slices drop. After dispatch, the shard owns completion and the durable batch identity, preserving the existing uncertain-client-response retry/dedup boundary (`scribe/ingress.rs:535-627`). The remediation adds no detached work or new retry identity. | PASS |
| Null parent Struct during persistence or restart | Producers now preserve null at both parent and child. Fixed IPC validates the actual parent mask, so an absent parent round-trips while a child null under a present parent fails before WAL construction. The encoded schema and values use the existing WAL/staged-run owners; restart/replay gains no new side state or cleanup path. | PASS |
| Corrupt stored Variant encountered by Oracle | Placeholder masking performs full validation before `VariantArray`/JSON conversion. Corruption becomes a query execution failure, then a failed terminal; it does not panic the shared Oracle process. Distributed descendants are cancelled and joined and client collectors discard already received frames. Unrelated queries and Scribe/Forge remain available. | PASS |
| Oracle timeout, cancellation, peer loss, or pod replacement | Query ownership is process-local and one-attempt. The existing terminal path cancels the graph and releases active reads, memory, scratch, slots, and transports. A lost pod terminates only its in-flight queries; replacement sessions reinstall the one Variant SQL registry and reconstruct durable source state from Postgres/object storage. | PASS |
| Forge cancellation, lease loss, process loss, or ambiguous commit | TASK-001's v3 lineage values remain inside the existing five-field handoff and become visible only through the fenced catalog commit. Cancellation preserves possible-output identity, lease loss fences stale settlement, and process loss is recovered by the existing durable task/operation reconciliation. The remediation changes no publication authority or cleanup root. | PASS |
| Rolling overlap with the pre-remediation built-in schema | Revision 12 intentionally changes nullable child schema and the approved specification explicitly declares no shipped table/data migration or mixed-version state. Existing non-v3 or mismatched physical schemas fail closed, and deployment compatibility fingerprints must prevent unsupported peer/application overlap. No compatibility reader or dual writer is authorized or required for this pre-release task. | PASS within the approved deployment boundary |

## Recovery and Proof Assessment

Focused checks run on the supplied candidate:

- `wyrd-queue`: `raw_depth_is_bounded_before_full_validation`, `variant_extension_requires_empty_metadata`, and `malformed_stored_variant_is_an_error_not_a_panic` — 3/3 passed.
- `vala-bifrost-redux`: `masked_required_struct_child_null_roundtrips` and `malformed_stored_variant_fails_every_function_without_panicking` — 2/2 passed.
- `wyrd-server`: `unscored_drift_writes_only_the_summary` and `unresolved_call_nulls_resolved_model_children` — 2/2 passed.

The tracked implementation evidence additionally records the real server
journey `builtin_variant_columns_are_refused_before_ack`, which submits the
compact hostile depth input, asserts the typed refusal, and verifies a later
accepted batch; the hot/published verification-summary journey; the metrics
journey; the compiled CLI journey; the Oracle interactive/distributed registry
journey; and the repeated v3 Forge rewrite/GC journey. Those environment-backed
lanes were not all rerun by this reviewer.

Residual proof limits are non-blocking: no pod-kill test interrupts the exact
new shallow depth walk or nullable-child IPC encode, but both operations finish
before any durable authority transition. Existing Scribe crash/replay, Oracle
peer-loss/cancellation, and Forge reconciliation owners remain unchanged and
cover failures after those transitions.

## Material Proposed Findings

No material system-resilience findings.

The candidate confines malformed Variant and nested-null failures to the
request/query boundary, keeps preprocessing and IPC work bounded and pre-WAL,
preserves structured cancellation and no-partial-result terminals, introduces
no new durable authority or retry identity, and leaves restart/reconciliation
under the existing Scribe, Oracle, and Forge owners.

## Overall Verdict

**PASS**
