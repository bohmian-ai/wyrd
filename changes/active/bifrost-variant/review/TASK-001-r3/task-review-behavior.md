# TASK-001 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation tasks: `TASK-001-R1-close-variant-contract-gaps.md` and `TASK-001-R2-exact-integers-and-late-errors.md`

The candidate remained at `555308ba14058ddc56102d2f925298ef43858175` throughout this review. I reviewed the complete base-to-candidate range and used the prior findings only as hypotheses. The binding human decisions govern the apparent tensions in older task prose: standard Iceberg v3 lineage is the sole lineage mechanism; no duplicate-ID or optional-metrics gate is required; `serde_json` arbitrary precision remains disabled; integers outside `i64`/`u64` are refused; every late terminal carries the complete catalog problem; and Python/TypeScript Interactive-only late-failure journeys are an accepted limit.

## Navigation and caller-to-result coverage

- Variant admission: `wyrd_queue::variant::EncodedVariant`, `batch_builder`, schema conversion, Scribe `decode_rows`, `DomainDefinition::validate_variants`, and Gate's typed gRPC projection.
- Built-in producers and consumers: canonical trace/log/metric projections, verification results, eval observations/items, gateway capture, agent traces, audit projection, Rust/Python/TypeScript/MCP JSON terminals, and generated contracts/docs.
- Query: `OracleVariantSql` registration, local and Analytical session constructors, `map_datafusion_error`, `build_frames`, terminal construction, protobuf conversion, HTTP/gRPC projection, shared-client collection/error reconstruction, and Python/TypeScript wrappers.
- Storage: v3 catalog creation and validation, hidden metadata-column projection through the pinned Iceberg/compaction forks, Forge repeated rewrite and GC journey, and current fork pins.
- Bloom sizing: the shared Scribe/Forge writer recipe and parquet-rs native NDV/folding behavior.
- Tests: task V1-V17 sources, R1/R2 focused tests, and the recorded final-candidate evidence.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, REQ-002, INV-003, INV-006, AC-002: every Bifrost table is v3; repeated Forge rewrites and v3 GC preserve both hidden lineage values without exposing them | `catalog/bifrost_catalog.rs:1047-1055,1062-1100` creates and validates only v3. The pinned compaction revision is `b68d9a9ff6c23bdc2d2e8c9d704b0aefe3c564d9`. `managed_rewrite.rs:1102-1260,1450-1562` observes each logical-value/hidden-lineage tuple across two rewrite outputs and GC. The rejected duplicate-ID scan and optional-metrics gate are absent. | Recorded V10 and V13 passed on the pinned revisions; V12 passed for the pinned Iceberg fork. The revised V10 proof follows standard Iceberg v3 behavior and the binding human decision. | PASS |
| REQ-003, REQ-004, REQ-019, INV-001, INV-002, INV-007: one canonical Variant representation, limits, exact numeric meaning, duplicate-key rule, stable errors, and logical fingerprint | `wyrd-queue/src/variant.rs:135-183,650-680` validates canonical bytes and classifies raw JSON tokens without `f64` for integers: `i64`, then `u64`/scale-zero decimal, else the catalog range refusal. `CanonicalType::Variant`, `DataTypeSpec::Variant`, tag `0x0d`, and the Arrow extension flow through the cumulative diff. `arbitrary_precision` is not enabled. | Focused review rerun: `variant::tests::json_text_classifies_integers_from_their_tokens` and `json_converts_under_the_variant_contract` both passed. Recorded V1, V5-V8, and the Oracle exact-integer test cover the public terminals, including exact `u64::MAX` and both refused bounds. | PASS |
| REQ-006-REQ-011, INV-001, INV-004, INV-005, AC-001, AC-003: every named built-in uses the required Variant/Struct shape, promotions, sensitivity, and producer/consumer closure | The table ledgers declare the required signal Variants, `resource_entity_refs`, promoted columns, typed `drift_report`/`eval_summary`, renamed gateway payloads, and the remaining open payloads. Producers in the table projections, verification results, eval, gateway, agent trace, and audit owners emit those shapes. Scribe `decode_rows` calls recursive `validate_variants` before acknowledgement/WAL. | Recorded V1-V8 and V14-V16 passed, including the typed-built-in journey, all three OTLP journeys, Rust/Python/TypeScript/MCP journeys, canonical signal equivalence, and generated-contract checks. | PASS |
| REQ-017, INV-004, INV-006, AC-001, AC-005, AC-008: one Oracle Variant SQL owner is installed before planning/codec/execution in every production session; Struct remains `get_field`; sensitivity precedes IO | `oracle/variant_sql.rs` owns `variant_get`, text conversion, parse/try-parse/to-json and the expression planner; constructors in `oracle/mod.rs` and `oracle/analytical.rs` install the shared owner, while codec/peer fingerprints bind `ORACLE_VARIANT_SQL_VERSION`. The existing authorization and tenant cut precede provider IO. | Recorded V9 passed across leader, admission, follower, analytical, and worker sessions, including function/operator semantics, plan round-trip, Struct separation, and sensitive-expression refusal. | PASS |
| REQ-019 and R2 FIND-12: pre-stream and late failures carry the same full catalog problem in Interactive and Analytical execution, every SDK reconstructs it, and bounded terminals return no partial result | `wyrd-spec/src/vala/api.rs:809-845,848-915` requires a problem on failed terminals. `oracle/mod.rs:3664-3699,4240-4279` maps the execution error before building the terminal. `oracle/query_stream.rs:450-510` applies that path after emitted batches. Protobuf uses `error_problem_json`; the shared client reconstructs through `from_problem` and rejects a failed terminal after clean EOF. No fixed terminal code list or prose parser remains. | Focused review rerun: `late_catalog_error_keeps_its_identity`, `late_failure_terminal_is_closed_and_non_success`, `failed_terminal_problem_round_trips`, and `failed_terminal_problem_rebuilds_its_catalog_error` all passed. Recorded V5/V9 real-server journeys prove a delivered batch followed by exact Variant failure, generic unrelated failure, Interactive and Analytical paths, and no successful partial collection. Recorded Python/TypeScript Interactive journeys plus the accepted shared-client/Rust V9 limit are credible. | PASS |
| REQ-005, AC-009 task-owned portion: both writers use native row-group Bloom capacity while keeping FPP/folding | `parquet/writer_properties.rs:127-150` sets only Bloom enablement/FPP; parquet-rs derives NDV from `DEFAULT_MAX_ROW_GROUP_ROW_COUNT`. Scribe and Forge use the same recipe. No Wyrd NDV duplicate remains. | Recorded V11 passed; `writer_properties.rs:316-332` checks both writer recipes. Hot-path `IN` probing remains TASK-003/REQ-027 scope rather than being duplicated here. | PASS |
| R1 FIND-2: malformed, wrong-extension, too-deep, and too-large Variant input is refused for every TASK-001 built-in before ACK/WAL with typed details | Scribe's common `decode_rows` invokes table-owned recursive Variant validation, converts it to `ScribeError::ContractViolation`, and Gate carries the canonical problem in `wyrd-error-bin`. This is the shared producer boundary, not per-producer guards. | Recorded `builtin_variant_columns_are_refused_before_ack` and Gate error tests passed, covering top-level and nested inputs and no persisted row. | PASS |
| Architecture/docs and public-surface parity required by the task and prior findings | `architecture/bifrost-design.md` and `docs/.../bifrost/schema.svx` describe v3, standard lineage, Variant SQL, exact integer bounds, and built-in layouts. Rust, Python, TypeScript, HTTP/gRPC, and MCP consume the shared wire/client behavior. | Recorded `docs:check`, `check:docs`, `codegen:check`, Python/TypeScript type/lint lanes, and client/boundary checks passed. | PASS |
| Prohibited scope: no migration, shredding, user-model inference, second Variant model/reader, DataFusion repin, compatibility alias, duplicate-lineage mechanism, metrics gate, `arbitrary_precision`, or new public knob | Cumulative diff and manifests retain these exclusions. Iceberg/compaction pins are the approved narrow forks; TASK-002/003 behavior was not pulled into runtime code. Reserved protobuf numbers follow the established protobuf standard and are not a compatibility field or alternate wire model. | Source/diff inspection; recorded format, lint, codegen, docs, boundary, and diff checks passed. | PASS |

## Prior-finding closure

| Finding | Behavior disposition |
|---|---|
| `FIND-TASK-001-1` | Closed: raw-token integer classification preserves all accepted integers exactly and refuses values outside `i64`/`u64`; `arbitrary_precision` remains off. |
| `FIND-TASK-001-2` | Closed: the common Scribe admission boundary recursively validates every built-in Variant before ACK/WAL and retains catalog details. |
| `FIND-TASK-001-3` | Closed under the binding decision: production and tests rely only on standard Iceberg v3 lineage handling; duplicate-ID and optional-metrics gates are absent. |
| `FIND-TASK-001-4` | Closed: active Bifrost authority and the existing schema guide match the shipped contract. |
| `FIND-TASK-001-5` | Closed: distributed Variant execution uses the typed tagged carrier already present in the DataFusion error channel; unrelated text remains generic. |
| `FIND-TASK-001-6` | Closed: invalid `EncodedVariant` construction is private and the unused emptiness surface is absent. |
| `FIND-TASK-001-7` | Closed: `QueryResult` documentation is attached to the exported TypeScript class. |
| `FIND-TASK-001-8` | Closed: parquet-rs native row-group geometry owns Bloom NDV. |
| `FIND-TASK-001-9`, `FIND-TASK-001-10` | Closed in the behavior paths inspected; recorded lints and cumulative source audit cover documentation/import rules. |
| `FIND-TASK-001-11` | Closed: values beyond `u64` or below `i64` are rejected, and `u64::MAX` is exact through each required public terminal. |
| `FIND-TASK-001-12` | Closed: the terminal now carries the complete RFC problem and the shared client reconstructs it for all public projections. |

## Proposed findings

None. I found no reachable missing, incorrect, drifted, violating, or regressed TASK-001 behavior after applying the binding decisions. The cumulative change uses existing standard/native mechanisms; I found no mechanism, check, option, file, or setting that is unsupported both by repository precedent and comparable widely used projects.

## Verification performed in this review

- Passed: `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::json_text_classifies_integers_from_their_tokens) | test(=variant::tests::json_converts_under_the_variant_contract)'` (2 tests).
- Passed: focused `vala-bifrost-redux` tests `late_catalog_error_keeps_its_identity`, `parse_json_keeps_exact_integers_and_refuses_the_rest`, and `late_failure_terminal_is_closed_and_non_success` (3 tests).
- Passed: `wyrd-tonic` `failed_terminal_problem_round_trips` and `wyrd-client` `failed_terminal_problem_rebuilds_its_catalog_error` (1 test each).
- Reviewed but did not rerun: the repository-managed Postgres, multi-pod Analytical, OTLP, Forge, fork, Python, TypeScript, MCP, docs, codegen, format, lint, and boundary lanes recorded against this exact candidate. Their commands, pins, and source assertions are consistent with the candidate. The accepted Python/TypeScript Interactive-only late-failure limit is recorded rather than treated as a verification gap.

## Overall result

**PASS**
