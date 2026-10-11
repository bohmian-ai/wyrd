# TASK-001 r6 persisted-schema domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation authority: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

The candidate and tree matched the supplied identities before review. CodeGraph
was used first for the results, metrics, gateway, `WHOLE_STRUCTS`, producer,
and fingerprint paths. I then inspected the cumulative base-to-candidate
changes, the r1-r5 persisted-schema/Variant findings and verdicts, the r5
remediation and recorded evidence, and the current owners and tests. I did not
read another r6 report and made no source change.

## Reviewed boundary and authority

This review covers the durable Arrow/Parquet/Iceberg shape of the built-in
tables changed by TASK-001, especially:

- `vala.verification.results` `drift_report` and `eval_summary`;
- `vala.metrics.points` `positive_buckets` and `negative_buckets`;
- `vala.gateway.calls` `requested_model` and `resolved_model`;
- table-owned complete-present/null-absent validation through
  `DomainTable::WHOLE_STRUCTS` and `refuse_partial_structs`;
- producer validity bitmaps, logical fingerprints, Iceberg optionality,
  Arrow-to-Parquet-to-Arrow null preservation, sibling built-ins, and the
  unreleased/no-migration premise.

Governing authority was `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/bifrost-design.md`,
`architecture/references/domain/arrow-analytical-interop.md`, specification
revision 13, TASK-001, and TASK-001-R5. The locked schema layouts are in
`spec.md:237-276`; REQ-006 through REQ-011 are in `spec.md:656-751`; the Arrow
authority requires field names, logical types, nesting, and nullability to
survive every projection.

## Source coverage and result

| Boundary | Source and evidence | Result |
|---|---|---|
| Verification Struct declarations | `tables/verification/results.rs:38-112` declares both parents nullable and every child nullable in the locked order; `WHOLE_STRUCTS` names both columns. `tables/mod.rs:273-334` runs common Variant validation and then complete-present/null-absent validation. | **PASS** |
| Verification producers | `wyrd-server/src/verification/results.rs:691-746` fills every child for a present report/summary and emits child nulls plus a null parent when absent. The scored/unscored/Eval journey queries every child hot and after publication (`verification_runtime.rs:1045-1184`). | **PASS** |
| Metrics bucket declarations and producer | `tables/metrics/points.rs:30-64,100-147` makes both bucket children nullable only because their parent is nullable. `tables/metrics/projection.rs:1210-1255` fills both children when present and nulls both when absent. `validate_metric_points` invokes `refuse_partial_structs` for both bucket columns (`:277-335`). | **PASS** |
| Metrics persisted null behavior | The recorded OTLP journey queries all four child leaves after flush and requires SQL null for an absent gauge bucket (`metrics_export.rs:834-855`). The r6 candidate adds a real-server partial-bucket refusal and a following valid write in `verification_runtime.rs:1621-1669,1933-1969`. | **PASS** |
| Gateway requested/resolved schema | `tables/gateway/calls.rs:94-120` now expresses the real distinction: required `requested_model` has required children; optional `resolved_model` has nullable children. The exact nested layouts are asserted at `:162-238`. This closes r5 `PERSIST-R5-001` / `FIND-TASK-001-22`. | **PASS** |
| Gateway producer and semantic validator | Serde Arrow builds the two `ModelRef` values from the typed payload in `capture.rs:476-511`; `unresolved_call_nulls_resolved_model_children` proves an absent resolved model nulls both children (`:1426-1447`). `CallsTable::WHOLE_STRUCTS` routes `resolved_model` through the common validator. | **PASS in source; incomplete boundary proof — `PERSIST-R6-001`** |
| `WHOLE_STRUCTS` mechanism | `tables/mod.rs:296-334,474-485` is one small shared validation rule, configured only by Results and Calls; Metrics reuses the helper from its already-specialized validator. It rejects both partial states and adds no read repair, public type, compatibility schema, or second persistence path. | **PASS** |
| Arrow -> Parquet -> Arrow nullability | `iceberg_schema_for` preserves the declared Arrow nested optionality; catalog reconciliation compares names, nested types, and nullability (`catalog/bifrost_catalog.rs:996-1107,1509-1528`). Results and metrics have published child-query proof. The same schema mechanism applies to gateway, but gateway itself lacks the required published child proof. | **PASS for implementation; gateway proof limit below** |
| Logical fingerprints | Predeclared built-ins derive the catalog identity from `arrow_fields`; nested child nullability is contained in the Struct `DataType`, so both the verification nullable-child repair and the requested/resolved gateway split change the logical identity. Canonical metrics also carries the full physical fingerprint, including nested nullability. Exact schema tests pin Results and gateway layouts; the all-built-in test recomputes every fingerprint and rejects zero. | **PASS** |
| Sibling built-ins | The only other Structs are non-null list elements (`resource_entity_refs`, span events/links, exemplars, quantiles, gateway usage/object refs) or the three nullable parents already covered. They do not share the absent-parent leaf problem. Eval observation/result-item, agent-trace, audit, trace, log, metric, and gateway open payloads remain Variant with their required nullability and sensitivity. | **PASS** |
| No-migration premise | `spec.md:32,148-154,609-616` says Bifrost is unreleased and forbids migration. Deployment authority says no Wyrd image has been published. Fresh creation is Iceberg v3 (`bifrost_catalog.rs:996-1058`); an existing non-v3 or nested-schema mismatch fails closed (`:1062-1107`). The intentional fingerprint/schema changes therefore need no migration or compatibility reader. | **PASS** |

## Material proposed finding

### PERSIST-R6-001 — MISSING: gateway complete-Struct closure lacks the required real-server and published proof

- **Violated obligation:** TASK-001-R5 requires the correction for
  `FIND-TASK-001-14` to drive raw Arrow through a real server for verification,
  metrics, **and gateway**, asserting the exact code, no retained row, a
  successful following valid write, and unchanged hot/published reads
  (`TASK-001-R5:88-93`). Its gateway-specific proof also requires raw
  admission to refuse a partial present `resolved_model` (`:109-112`). The
  acceptance section requires every focused negative journey to prove the
  exact catalog details, no ACK/no row, and continued availability
  (`:245-255`).
- **Exact locations:**
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:2538-2661`;
  `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:1426-1447`;
  `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:1045-1223,1272-1705`;
  implementation-evidence claim in
  `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md:292-313`.
- **Evidence:** The table unit test calls the resolved definition's validator
  directly and proves one partial `resolved_model` is refused. The capture unit
  test proves the producer's absent child slots. The real-server negative
  journeys added by the candidate cover a partial `eval_summary` and a partial
  `positive_buckets`, but no gateway frame. The valid gateway journey flushes
  a resolved call and queries only `request_payload` and `response_payload`; it
  never queries either model child, and it does not cover an absent
  `resolved_model`. The implementation record explicitly downgrades gateway to
  unit-only because public clients cannot write the table, despite the approved
  remediation requiring the server-owned path.
- **Observable consequence:** Source inspection supports the intended behavior,
  but the acceptance evidence does not prove that the internal gateway capture
  ingress actually applies the validator before ACK/WAL, that the exact
  `WYRD_VALA_400_SCHEMA_PARSE` detail crosses that boundary, or that an absent
  resolved model's child nulls survive publication. A regression in the
  gateway-only peer/capture wiring or its persisted projection can pass every
  cited test while violating the finding's closure conditions.
- **Required testable correction:** Use the existing server-owned gateway
  capture/peer ingest test boundary; do not expose a public write or add a
  harness. Submit a valid calls batch modified to contain a partial present
  `resolved_model`, require the exact schema-parse problem before ACK and no
  retained row, then submit the ordinary valid capture and prove continued
  availability. Also flush an unresolved capture and query both
  `resolved_model` children as SQL null. Reuse the current `CallsTable`
  declaration, `CallCapture::calls_batch`, and common validator; no production
  change is indicated by this review.

## Verification limits

Per orchestrator coordination, I started no Cargo, mise, codegen, Postgres, or
object-store job in this shared checkout. I inspected the recorded r5 command
results, including the focused table tests, gateway producer test, real-server
verification/metrics journeys, OTLP metrics publication journey, cross-language
queries, codegen, lints, and `git diff --check`. Those recorded green results
support the passing rows above but do not supply the missing gateway boundary
and publication scenario.

## Overall result

**FAIL**

The cumulative persisted schema is coherent: Results, metrics, and gateway now
declare the intended nullable leaves; producers emit only whole values; the
shared validator rejects partial states; Arrow/Iceberg reconciliation preserves
nested nullability; sibling built-ins are unaffected; fingerprints change with
the durable nested shapes; and the unreleased product correctly adds no
migration. One bounded acceptance-evidence gap remains: gateway
`resolved_model` does not have the explicitly required real-server refusal and
published-null proof.
