---
id: BIFROST-OTEL-T03A-R2
title: Complete schema-aware SQL results and remediation hygiene
kind: remediation
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
original_task: changes/active/bifrost-canonical-otel-signals/tasks/03a-unified-bifrost-client.md
base: 63ad8cbb8a03c7a83cf357a23fed794521ecbbb1
prior_candidate: ae18fea3dd39173763596ba825fdfe8ba3eeae76
candidate: 5f4d1ea180762299ac20797cfbf9122bca5bc9d7
findings: [FIND-BIFROST-OTEL-T03A-1, FIND-BIFROST-OTEL-T03A-8, FIND-BIFROST-OTEL-T03A-9, FIND-BIFROST-OTEL-T03A-10]
---

# Complete schema-aware SQL results and remediation hygiene

Required execution skill: `$wyrd-implement`.

## Authority and scope

The user explicitly approved the candidate's SQL-only typed-observation deviation. Preserve deletion of the former typed observation routes, RPCs, SDK methods, contracts, projections, and tests. The same instruction adds one client capability: `sql` may accept a language-native result schema/model and, when supplied, return rows validated and constructed through that mechanism.

This remediation covers the cumulative candidate `63ad8cbb8..5f4d1ea18`, the original task, the prior verdict and R1 task under `review/BIFROST-OTEL-T03A-ae18fea3d-reviewer-1/`, and the current verdict beside this file.

## Issue diagnosis

### FIND-BIFROST-OTEL-T03A-1 — TypeScript zero-row results discard the authoritative schema

`typescript/wyrd/src/index.ts::Bifrost.sql` drains batches and constructs `QueryResult` from only those batches and the terminal. `QueryResult.toArrow()` constructs an Arrow `Table` from the batches. A successful zero-row query yields no batch, so both `toArrow()` and `toBytes()` expose an empty schema rather than the schema the Rust `QueryResultStream` received. The existing TypeScript conversion journey contains rows and cannot detect this.

The result must retain the server-supplied Arrow schema even when it has no batches. Reuse the current Rust query stream and Arrow IPC/schema owners; do not infer fields from SQL or add a TypeScript schema mapper.

### FIND-BIFROST-OTEL-T03A-8 — `query_client` documentation advertises removed operations

`crates/vala/vala-sdk/src/bifrost.rs::Bifrost::query_client` still says it provides typed trace and GenAI reads. Those operations were intentionally removed. The adjacent blocking-facade documentation already names the remaining lifecycle/raw-query surface accurately.

The public async documentation must describe only the retained query controls and raw request access.

### FIND-BIFROST-OTEL-T03A-9 — cumulative whitespace validation fails

`git diff --check 63ad8cbb8..5f4d1ea18` reports an extra blank line at EOF in the prior T03A verdict and in `typescript/wyrd/src/index.ts`. The recorded worktree-only check inspected no committed patch.

Remove only the reported whitespace and prove the cumulative range.

### FIND-BIFROST-OTEL-T03A-10 — `sql` has no language-native typed-row result

Rust, Python, and TypeScript currently accept only a query and always return the Arrow-backed `QueryResult`. The user wants an optional result schema/model supplied with the SQL call. When present, each row must be validated and returned as that language's schema object; when absent, the existing `QueryResult` behavior must remain unchanged.

This is a client-side ergonomic projection after successful terminal validation, not a typed observation route or server contract. It must not change SQL, authorization, result limits, cancellation, Arrow decoding, or query terminals.

## Intended correction outcome

- Every successful TypeScript `QueryResult`, including a zero-row result, preserves its authoritative Arrow schema through `toArrow()` and `toBytes()`.
- Raw `sql(query)` remains source-compatible and returns `QueryResult` in Rust, Python, and TypeScript.
- Supplying a language-native result schema returns validated row objects: Pydantic instances in Python, Zod-style parsed values in TypeScript, and deserialized typed structs through the idiomatic Rust typed-query form.
- Validation failures are explicit client errors and never turn partial typed rows into success.
- Public documentation and cumulative whitespace checks match the resulting surface.

## Decision-complete recommendation

Keep `vala_sdk::Bifrost`, `QueryResult`, and the existing terminal-safe query stream as the only query owners. Typed-row conversion begins only after `sql` has produced a complete successful `QueryResult`.

For Python, add an optional model argument to sync and async `sql`. Convert the completed Arrow table to row mappings and call the supplied Pydantic-compatible model's existing `model_validate` for every row. Return the current `QueryResult` when omitted and a list of model instances when supplied.

For TypeScript, add an optional structural schema argument with a `parse(value)` method, which Zod already supplies. Convert each completed Arrow row to a plain object and return `schema.parse(row)` values. Express overloads/generics so omission returns `QueryResult` and a supplied schema returns its inferred row array, without making Zod a production dependency.

For Rust, use the idiomatic typed equivalent rather than simulating optional runtime arguments: retain `sql(&str) -> QueryResult` and add a typed `sql_as<T: serde::de::DeserializeOwned>(&str) -> Vec<T>` projection on both async and blocking facades. Reuse the installed Arrow JSON/Serde path over the completed `QueryResult`; do not add a new conversion dependency or public mapper abstraction.

At the N-API stream boundary, retain or expose the Rust stream's authoritative schema so TypeScript collection does not reconstruct it from batches. Use the already-installed Apache Arrow implementation for table and IPC construction. Update the stale async `query_client` rustdoc from the blocking facade's accurate wording, and remove the two reported blank EOF lines.

## Constraints and preserved behavior

- Preserve the user-approved SQL-only observation read surface; do not restore any removed typed route, RPC, request/response contract, SDK method, or compatibility alias.
- Preserve one Rust-owned query execution, successful-terminal requirement, cancellation, limits, authorization, tenant isolation, and structured errors.
- Preserve existing `sql(query)` return values and all `stream` behavior.
- Typed conversion is local and post-query. It must not send schema information to the server, alter SQL planning, or claim the supplied model is the durable table schema.
- Validate every row through the supplied language mechanism. If any row fails, return one failure and no successful typed result.
- Reuse PyArrow/Pydantic behavior already available to Python, Apache Arrow and structural `parse` in TypeScript, and installed Arrow/Serde support in Rust.
- No new server API, schema mapper, harness, test target, runtime, framework, production dependency, or typed observation surface.
- Do not touch the unrelated modified `changes/active/bifrost-forge-oracle-integration/spec.md` worktree file.

## Explicit non-goals

- No streaming typed-row iterator in this remediation; `stream` remains Arrow batches.
- No automatic SQL generation from the supplied result model.
- No coercion that bypasses Pydantic/Zod/Serde validation.
- No persistence, registration, fingerprint, or table-layout semantics for result schemas.
- No restoration or client-side reimplementation of trace, GenAI, eval, drift, metric, log, or agent-trace readers.

## Acceptance criteria

| Finding | Required observable result |
|---|---|
| `FIND-BIFROST-OTEL-T03A-1` | A successful zero-row TypeScript query retains the selected field names and types in `toArrow()` and after decoding `toBytes()`. |
| `FIND-BIFROST-OTEL-T03A-8` | Rust API documentation for `query_client` names only operations that exist after the approved typed-read deletion. |
| `FIND-BIFROST-OTEL-T03A-9` | `git diff --check 63ad8cbb8..<new-candidate>` passes. |
| `FIND-BIFROST-OTEL-T03A-10` | Existing language journey owners prove raw `sql(query)` is unchanged; Python sync/async SQL returns Pydantic instances when a model is supplied; TypeScript SQL returns Zod-parsed typed values when a schema is supplied; Rust async/blocking typed SQL returns deserialized structs; and each language surfaces one invalid-row validation failure without returning partial typed rows. |

## Verification

Run exact focused tests for each new Rust scenario through `mise exec -- cargo nextest run --locked` and the focused Python/TypeScript commands selected by their existing owners. Then run:

```bash
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run fmt
mise run lints
git diff --check 63ad8cbb8..<new-candidate>
```

DO NOT run full repo level tests. Just run the focused tests above. The candidate is a SQL-only observation change; no other test targets are affected.

Record baseline-identical unrelated failures separately. Do not weaken or suppress a gate or test to obtain a pass.
