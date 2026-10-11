# TASK-001 Invariant Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Candidate stability: `HEAD` resolved to the candidate before and after source inspection.
- CodeGraph: unavailable because the repository has no `.codegraph/` directory.

## Navigation and state-flow coverage

| State or value | Producer / owner | Sinks and sibling consumers inspected | Proof considered |
|---|---|---|---|
| Variant logical identity, limits, and public failures | `wyrd-spec/src/vala/{api,error}.rs`; `wyrd-queue/src/variant.rs`; `vala-bifrost-redux/src/tables/{fields,mod}.rs` | Queue row building, server Arrow conversion, table fingerprints, OTLP projection, Oracle UDFs, Rust/Python/TypeScript/CLI/MCP JSON terminals | V1, V2, V4-V9, V14-V16 from the task evidence; source tests in `variant.rs`, `tables/mod.rs`, and `oracle/variant_sql.rs` |
| Built-in open values and persisted Structs | Canonical signal ledgers/projectors; Eval observation row projection; verification result builder; gateway capture; audit projection | Scribe canonical validation, built-in table schemas, Oracle field/Variant reads, typed SDK terminals | V1-V8 and the named consumer journey reruns |
| Oracle Variant registry and stable failures | `OracleVariantSql`; `OracleExecution::session_state`; analytical worker builder; planning session; error mapping | Leader planning, admitted execution, follower decode, analytical stages, distributed error return, client problem reconstruction | V9 and `variant_operators_and_functions_follow_the_contract` |
| Iceberg format and hidden row lineage | Catalog creation/validation; pinned Iceberg writer; compaction fork | Scribe promotion, Forge repeated rewrite, manifest maintenance, snapshot expiry and orphan cleanup, Oracle logical schema | V10, V12, V13 |
| Bloom capacity | Shared Parquet writer recipe | Scribe writer and Forge rewrite properties | V11 |
| Public description of the new durable model | Task/spec documentation obligations | `architecture/bifrost-design.md`, `docs/`, examples | No candidate diff exists for these consumers |

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003, REQ-004, REQ-019: one Variant type, exact limits/error details, type-preserving conversion, and stable refusal | `crates/shared/wyrd-queue/src/variant.rs:43-179`; `crates/vala/vala-bifrost-redux/src/tables/mod.rs:627-720`; catalog errors in `crates/wyrd-spec/src/vala/error.rs` | V1; local Variant contract tests; V9 invalid/try JSON checks | PASS |
| REQ-006-REQ-008, REQ-011, INV-002, INV-005, AC-001: canonical OTLP Variant/Struct projection, final-key-wins, promotions equal their source, and canonical Arrow equivalence | `crates/vala/vala-bifrost-redux/src/tables/signal.rs:90-203` and the changed traces/logs/metrics ledgers and projectors | V3-V4, V14-V15 and the signal consumer reruns | PASS |
| REQ-009, REQ-010, INV-001, AC-003: verification, Eval, gateway, agent-trace, and audit payloads no longer retain the replaced opaque form | Changed table definitions and producer projections in `tables/{verification,eval,gateway,dev,audit}`; `wyrd-server/src/verification/results.rs`; `wyrd-client/src/observe/eval.rs`; gateway capture | V2, V5-V8 and the named Rust/Python/TypeScript/MCP consumer reruns | PASS |
| INV-004, AC-008: sensitivity of Variant/Struct roots is enforced before provider IO | Variant and Struct remain classified at their logical root; Oracle authorizes referenced logical columns before provider construction | V9 explicitly checks the under-privileged workload path before follower leases advance | PASS |
| REQ-017: Struct remains `get_field`; Variant operators/functions have one semantic registry in every production session | `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:73-180`; shared admitted/follower registration at `resources.rs:4235-4255`; analytical worker registration at `oracle/analytical.rs:525-542`; planning registration in `oracle/mod.rs` | V9; `variant_operators_and_functions_follow_the_contract` checks plan shape, chaining, dynamic paths, parsing, and null-parent residual behavior | PASS |
| REQ-019 distributed/client closure: Variant failures keep their catalog identity | Typed-chain recovery and bounded remote reconstruction at `oracle/mod.rs:4250-4350`; exact HTTP/gRPC reconstruction in `wyrd-client/src/error.rs` | V9 exercises invalid JSON on interactive and analytical paths; focused mapping tests cover all Variant error variants | PASS |
| REQ-001: every created table is Iceberg v3 and non-v3 state is refused | `catalog/bifrost_catalog.rs:1016-1058` creates v3; `:1062-1100` rejects other versions | V10 creates built-in and user tables; V12 verifies the pinned Iceberg projection | PASS |
| REQ-002, INV-003, AC-002: compaction preserves `_row_id` and `_last_updated_sequence_number`, hidden from the logical schema and five-field handoff | Pinned compaction fork carries the hidden columns; `forge/publication.rs:455` validates output before commit and `:1456-1491` requires complete non-null lineage metrics | V10 compares both hidden values through two rewrites, injects missing lineage, asserts no commit, and runs v3 GC; V13 tests the fork | PASS |
| INV-006: tenant derivation and tripwires are unchanged across new storage/query paths | Variant changes reuse existing authenticated table binding, footer proof, follower assignment, and logical-column authorization; no tenant selector or alternate reader was added | V2, V4-V10 exercise real tenant-scoped server paths; existing tenant checks remain in the same owners | PASS |
| INV-007: Variant processing stays bounded | Fixed 64-level/8 MiB checks are centralized in `EncodedVariant`; OTLP conversion routes its finished encoding through the same validator; existing admitted Arrow bounds remain unchanged | V1 checks exact limits and V4 exercises OTLP projections | PASS |
| REQ-005, AC-009: both writers size Bloom filters from row-group row capacity while retaining FPP and Parquet folding | `parquet/writer_properties.rs:31-41,130-152`; both Scribe and Forge start from that recipe | V11 | PASS |
| Public terminal parity: Rust/Python/TypeScript typed rows and HTTP/MCP/CLI JSON render native Variant values while Arrow terminals retain storage | Shared `VariantJsonEncoderFactory`; Rust `QueryResult`; Python recursive schema walk; TypeScript recursive schema walk/native decoder; MCP/CLI encoder factories | V5-V8, V14-V15 and consumer journey reruns | PASS |
| Required consumer closure includes examples and supported-type documentation | The task explicitly includes these consumers at `TASK-001...md:38-43` and repeats `docs/examples` in the expected write set at `:172-180`; the spec requires the Bifrost authority update at `spec.md:1040-1042` | `git diff --name-only base..candidate -- architecture docs examples` returns no paths; `architecture/bifrost-design.md` contains no `Variant`, `Iceberg v3`, query-function, or duplicate-key contract | FAIL |
| Prohibited changes: no shredding policy, user-model inference, second reader/model, `datafusion-variant`, signing, migration, compatibility alias, or DataFusion repin | Complete diff and manifests show only the approved Arrow Variant crates in their owners and the pinned Iceberg/compaction revisions; DataFusion source is unchanged | Cargo lock/source inspection and V16 | PASS |
| Standing standard-practice direction: reject mechanisms/checks/settings absent both repository precedent and comparable projects | The new mechanics use Arrow extension metadata/encoder factories, standard Parquet Variant crates, Iceberg v3 lineage, DataFusion UDF/planner registration, and Parquet row-group Bloom geometry. No unsupported bespoke option, compatibility mechanism, or standalone check was found. | Source and manifest inspection | PASS |

## Proposed findings

### INV-REV-001 — MISSING — Required documentation and example consumer closure is absent

- **Violated obligation:** The task makes generated contracts, examples, and supported-type documentation affected consumers (`TASK-001-variant-storage-and-query.md:38-43`) and includes docs/examples in its expected write set (`:172-180`). Revision 10 also requires `architecture/bifrost-design.md` to describe Iceberg v3, Variant/query semantics, and the duplicate-key rule before completion (`spec.md:1040-1042`).
- **Exact location:** `architecture/bifrost-design.md:1-13,39-110,681-808` remains the pre-Variant authority. It describes the general Bifrost model and Forge publication but contains no Variant contract or Iceberg-v3 lineage model. There are no changed paths under `architecture/`, `docs/`, or examples in the base-to-candidate diff.
- **Evidence:** The runtime now creates only v3 tables (`catalog/bifrost_catalog.rs:1047-1052`), rejects non-v3 tables (`:1095-1099`), exposes a public Variant SQL surface (`oracle/variant_sql.rs:76-134`), and changes durable built-in schemas. The active design authority and supported-use documentation do not communicate any of those shipped contracts.
- **Observable consequence:** Maintainers and users following the repository's designated Bifrost authority cannot discover the new storage version, hidden-lineage requirement, Variant duplicate-key semantics, SQL operators/functions, or supported public type. The repository therefore has conflicting operational truth: source and generated schemas ship the behavior while the required human-facing consumers still describe only the older model.
- **Required testable correction:** Update the existing Bifrost architecture authority and the existing supported-type/example documentation surfaces to describe only the TASK-001 behavior that now ships: Iceberg v3 with hidden lineage preserved by Forge, the public Variant type and duplicate-key rule, and the Oracle `->`/`->>`/`parse_json`/`try_parse_json`/`to_json` contract. Reuse the current documentation structure; add no new documentation framework, checker, option, compatibility section, or speculative TASK-002/TASK-003 behavior. Run the repository's existing documentation/example checks applicable to the touched files in addition to the task-local contract checks.

## Prior-finding hypotheses

The task names BVR-FRESH-001, BVR-FRESH-003, BVR-FRESH-005, BVR-RR2-002, and BVR-RR3-003 as remediation inputs. Their substantive runtime hypotheses are closed in the cumulative candidate: typed built-in persistence, all-session Oracle registration, null-parent Variant masking, v3 hidden-lineage rewrite/GC, and row-group Bloom geometry are each present at their producer and shared consumer boundaries and have focused journey evidence. The missing documentation closure is a separate candidate-completion gap, not a reopening of those runtime findings.

## Verification assessment

- Reviewed the complete `base..candidate` diff and all changed-path inventory.
- Independently confirmed `git diff --check` succeeds through the orchestrator's shared evidence; no additional commands were needed to establish the source-level finding.
- The task records all 17 focused commands as passing on the final candidate, including Rust/Python/TypeScript/MCP journeys, Oracle distributed coverage, repeated Forge rewrite/GC, both pinned fork tests, code generation, formatting, and lints.
- I did not rerun the expensive database/fork suites. Their evidence is credible for the runtime obligations but cannot prove an absent documentation consumer.

## Overall result

**FAIL**

The runtime and persisted-state invariants inspected are implemented and covered by focused evidence, but the candidate does not satisfy the task exactly because its explicitly required architecture/documentation/example consumer closure is absent.
