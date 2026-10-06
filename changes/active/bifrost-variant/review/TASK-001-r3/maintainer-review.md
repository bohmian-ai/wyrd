# TASK-001 R3 Maintainer Review

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation tasks: `TASK-001-R1-close-variant-contract-gaps.md` and `TASK-001-R2-exact-integers-and-late-errors.md`

The candidate remained at the stated commit before and after review.

## Changed-surface coverage

| Surface | Symbols and owner flow inspected | Callers, tests, and declaration parity | Result |
|---|---|---|---|
| Variant contract and queue encoding | `DataTypeSpec::Variant`, `VariantViolation`, `EncodedVariant`, raw-token number classification, Variant Arrow storage/builders, schema conversion, batch admission | Built-in projections, `parse_json`, queue errors, contract/schema goldens, exact-integer tests | PASS |
| Built-in persisted schemas and producers | table ledgers and projections for traces, logs, metrics, verification, eval, gateway, agent traces, and audit; canonical signal conversion | OTLP projections, verification/eval/gateway/audit producers, server and OTLP journeys | PASS |
| Oracle SQL and query lifecycle | `OracleVariantSql`, operator planner/UDFs, session installation, stream terminal construction, distributed error reconstruction, query IPC conversion | leader/follower/analytical/worker construction, Rust Oracle journeys, terminal unit tests | PASS |
| Public late-error contract | `QueryTerminalFrame.error`, protobuf `error_problem_json`, tonic conversion, `wyrd-client` reconstruction, server HTTP/gRPC/MCP adapters | generated proto binary, Rust/Python/TypeScript query journeys and TypeScript unit coverage | PASS |
| Iceberg v3, Forge, and Bloom writers | v3 creation/validation, compaction handoff, hidden lineage projection/copy, GC, shared writer properties | repeated-rewrite journey, fork pins/tests, Scribe/Forge property test | PASS under the binding decision that standard Iceberg v3 lineage handling is the sole mechanism |
| Rust/Python/TypeScript result projection | Rust `QueryResult`, Python nested Variant conversion, napi Variant conversion, TypeScript nested Variant conversion | Python annotations/import surface, generated napi declarations, TypeScript public types and typed-row journeys | PASS |
| Documentation and generated contracts | `architecture/bifrost-design.md`, Bifrost schema guide, JSON schemas, proto source/binary, TypeScript declarations/error codes | compared to revision 11 and current source behavior | FAIL — `MAINT-001` |
| Repository maintainer shape | materially changed Rust items, owner/method placement, rustdoc, module imports, signature types, test placement and naming | compared with `AGENTS.md`, `architecture/agent-rules.md`, and maintainer style | FAIL — `MAINT-002` |

## Material findings

### MAINT-001 — Active architecture documents the superseded wide-decimal integer policy

- **Changed location:** `architecture/bifrost-design.md:149-152`
- **Governing principle:** the active design is the authoritative contract, and maintainer documentation must describe the same typed behavior as the implementation and public guide.
- **Evidence:** revision 11 REQ-004 and `EncodedVariant::from_json_text` accept signed `i64`, encode only the remaining `u64` range as scale-zero decimal, and refuse every integer outside those two ranges. `docs/src/content/docs/bifrost/schema.svx:31-42` states that exact rule. The newly added architecture paragraph instead says any integer that fits a Variant decimal becomes a decimal and only an integer no Variant numeric type holds is refused. That wording includes values beyond `u64` which the candidate intentionally rejects.
- **Concrete maintenance cost:** the primary Bifrost authority sends a maintainer implementing another producer or SQL path toward a wider accepted domain than the shared encoder and approved contract, making future source/docs parity unsafe.
- **Smallest testable correction:** replace the two broad decimal sentences in the existing architecture paragraph with the revision-11 wording: signed-64 integers remain integers, values above `i64::MAX` through `u64::MAX` become scale-zero decimals, and every other integer is refused. Reuse the current source and schema-guide language; add no mechanism, setting, file, or check.

### MAINT-002 — The cumulative Rust diff still scatters imports and uses qualified signature types prohibited by the repository style

- **Changed locations:** `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:1654,1678,1714`; `crates/shared/wyrd-client/src/observe/eval.rs:188`; `crates/wyrd/wyrd-server/src/query/service.rs:372`; `crates/wyrd/wyrd-server/src/verification/results.rs:593-595,758-761`; `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1302-1307`
- **Governing rule:** `architecture/agent-rules.md` requires all ordinary imports at module scope and bare imported names in fields, parameters, return types, and trait bounds. R1 FIND-TASK-001-10 and its acceptance criterion explicitly require the cumulative diff to contain neither form.
- **Evidence:** three query-stream tests import `DataFusionError` inside their function bodies. Materially changed/new declarations still spell `serde::Serialize`, `serde_json::Value`, `wyrd_spec::error::WyrdProblem`, and `std::fmt::Display` directly in bounds, fields, parameters, or return types. These are part of the candidate diff, not untouched surrounding debt.
- **Concrete maintenance cost:** the module header no longer exposes the real dependency surface, while equivalent domain types appear under inconsistent spellings across the same change. The failed R1 closure also leaves reviewers unable to rely on the task's claimed style audit.
- **Smallest testable correction:** move `DataFusionError` to the existing test module import block and import `Serialize`, `Value`, `WyrdProblem`, and `Display` in each owning module, then use their bare names in the changed declarations. Preserve the allowed local `use std::fmt::Write as _` trait-enablement case. Add no checker or allow attribute.

## Uncertain preferences

None. No optional refactor, abstraction, compatibility layer, extra test harness, or nonstandard lineage check is recommended.

## Verification assessment

- Reviewed the candidate's recorded focused and broader evidence in the original task and both remediation tasks, including the revision-11 exact-integer and late-terminal journeys.
- The accepted limit remains: Python and TypeScript prove late failure through Interactive; the Rust multi-pod journey proves distributed execution through the shared client reconstruction.
- Confirmed `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..555308ba14058ddc56102d2f925298ef43858175` passes.
- No build or runtime lane was rerun for this static maintainer pass. Neither retained finding needs a new harness: documentation comparison and source inspection directly prove them.

## Overall result

**FAIL**

The implementation shape is otherwise cohesive and its public declarations align across Rust, Python, TypeScript, HTTP/gRPC, and generated artifacts, but the active authority mismatch and the explicit unresolved repository-style obligation are material maintainer defects.
