# OTLP and built-in table domain review

## Review Findings

### Critical

None.

### Important

- **OTLP-TABLES-001 — `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs:41-45,111-112` — The nullable-child repair for `resolved_model` also weakens required `requested_model`.** The one `model_ref_type()` is reused by both columns, so changing its children to nullable changes the published schema of the non-null `requested_model` as well. That contradicts the source contract: `GatewayCallPayloadV1::requested_model` is a required `ModelRef`, whose `provider` and `model` are required typed identifiers (`crates/wyrd-spec/src/gateway/record.rs:542-545`; `crates/wyrd-spec/src/gateway/mod.rs:96-111`). `describe_table` and the persisted fingerprint now advertise that either requested-model child may be null even though no such `ModelRef` exists. The new capture test proves only that an absent `resolved_model` has null children and the table schema test asserts only the two parents' nullability (`capture.rs:1426-1447`; `calls.rs:161-211`), so neither catches the requested-model regression. **Recommended fix:** keep non-null children for `requested_model` and use a distinct nullable-child `ModelRef` data type only for `resolved_model`, within `CallsTable`; extend the existing table contract test to assert both nested layouts and keep the current absent-resolved producer test.

- **OTLP-TABLES-002 — `crates/vala/vala-bifrost-redux/src/tables/metrics/projection.rs:277-335` — The canonical Arrow write boundary does not enforce the new nullable bucket-child invariant.** Revision 12's follow-on decision requires an absent `positive_buckets`/`negative_buckets` Struct to have null children and a present Struct to carry both `offset` and `bucket_counts`; the OTLP projector now does that correctly in `push_buckets` (`projection.rs:1208-1230`). But `validate_metric_points`, which accepts public canonical Arrow batches, validates only the top-level kind-specific column and never inspects those children. Because the declared children are now nullable (`tables/metrics/points.rs:33-62`), a caller can submit a schema-valid exponential-histogram row with a present bucket Struct missing either child, or a null parent retaining child values. That state has no equivalent OTLP `Buckets` value and violates the module's complete-kind-shape contract; hidden child values can also be exposed by hot-path `get_field`, the exact invariant this remediation is meant to close. Existing journeys prove valid present OTLP buckets survive publication and absent gauge buckets read null after publication (`metrics_export.rs:263-480,791-855`), but they do not drive malformed canonical Arrow through the server boundary. **Recommended fix:** extend the existing `validate_metric_points` owner to require both bucket children valid when the parent is valid and both children null when it is null, returning the existing `WYRD_VALA_400_SCHEMA_PARSE` (`BifrostError::SchemaParse`) with row and column context. Add a focused validator test and one public Rust canonical-Arrow write rejection proving the exact code and no retained row; no new validator or error type is needed.

### Suggestions

None.

## Open Questions

None. Both corrections stay within the approved nullable-child decision and existing table owners.

## Reviewed Boundary and Evidence

- Immutable subject: base `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`, candidate `0e37748f3a27d3bcec4713e6210e97328e045886`, candidate tree `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`.
- Authorities: approved `SPEC-bifrost-variant` revision 12; `TASK-001`; round 1-4 verdicts, validation ledgers, domain reports, and remediation tasks; `AGENTS.md`; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; telemetry, OLAP serving, Arrow analytical interop, and DataFusion references.
- Traced metrics from OTLP gauge and exponential-histogram producers through `PointColumns`, `push_buckets`, `bucket_struct`, canonical schema validation, Scribe publication, SQL `get_field`, and Arrow result rendering. Present OTLP bucket Structs remain intact; absent projector-owned Structs now carry null children. The remaining gap is reachable only through the supported canonical Arrow producer.
- Traced gateway `GatewayCallPayloadV1` through validation, Arrow JSON decoding, `CallsTable`, Scribe, and query rendering. The capture producer correctly nulls an absent `resolved_model`; the shared schema helper unnecessarily changes `requested_model`.
- Traced verification `DriftReport` and `EvalWorkflowSummary` through `ResultPayloadBuilder`, nullable child arrays, hot and published `get_field`, whole-Struct JSON, and Variant JSON rendering. Both absent and present rows are covered and no additional defect was found.
- Inspected sibling built-in nullable Structs: verification summaries, metric exponential buckets, and gateway resolved model. List-of-Struct elements (`resource_entity_refs`, span events/links, exemplars, usage, object refs, quantiles) do not share the nullable-parent invariant.
- Public error catalog checked: the metrics semantic-shape owner already uses `BifrostError::SchemaParse`, code `WYRD_VALA_400_SCHEMA_PARSE`; no Variant error code applies to missing typed Struct children.

## Verification Notes

- Passed: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::unresolved_call_nulls_resolved_model_children)'`.
- Passed: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=tables::tests::variant_contract_and_builtin_schemas_are_stable)'`.
- Reviewed recorded V2 verification journey coverage for hot and published absent/present verification summary children and whole-Struct JSON.
- Reviewed recorded OTLP metrics journey coverage for all present exponential bucket values across gRPC, protobuf HTTP, and JSON HTTP, plus published absent gauge bucket-child queries.
- Missing proof: no test rejects an incomplete or parent/child-inconsistent exponential bucket Struct through the canonical Arrow write surface; no test asserts that `requested_model` retains non-null children after the `resolved_model` repair.

## Overall Result

**FAIL** — two bounded correctness findings remain. The candidate and tree identities matched before this report was written.
