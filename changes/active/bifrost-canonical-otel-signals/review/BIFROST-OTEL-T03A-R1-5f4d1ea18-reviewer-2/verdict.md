---
task: BIFROST-OTEL-T03A
verdict: FIX_REQUIRED
base: 63ad8cbb8a03c7a83cf357a23fed794521ecbbb1
prior_candidate: ae18fea3dd39173763596ba825fdfe8ba3eeae76
candidate: 5f4d1ea180762299ac20797cfbf9122bca5bc9d7
reviewer: reviewer-2
---

# BIFROST-OTEL-T03A remediation review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Current approved specification: `changes/active/bifrost-canonical-otel-signals/spec.md`, revision 10
- Original task: `changes/active/bifrost-canonical-otel-signals/tasks/03a-unified-bifrost-client.md`, pinned to revision 9
- Approved interface reference: `changes/active/bifrost-canonical-otel-signals/unified-bifrost-client-draft.md`
- Prior verdict: `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T03A-ae18fea3d-reviewer-1/verdict.md`
- Remediation task: `changes/active/bifrost-canonical-otel-signals/review/BIFROST-OTEL-T03A-ae18fea3d-reviewer-1/BIFROST-OTEL-T03A-R1-complete-unified-client.md`
- Base: `63ad8cbb8a03c7a83cf357a23fed794521ecbbb1`
- Prior candidate: `ae18fea3dd39173763596ba825fdfe8ba3eeae76`
- Candidate: `5f4d1ea180762299ac20797cfbf9122bca5bc9d7`
- Reviewed range: `63ad8cbb8..5f4d1ea18` (75 files, 5,998 insertions, 9,106 deletions)
- Review-time authority: the user explicitly approved the revision 10 SQL-only typed-read deviation and additionally required an optional language-native row schema/model on `sql`.
- Candidate stability: `HEAD` remained `5f4d1ea180762299ac20797cfbf9122bca5bc9d7`; the worktree was clean before this verdict was written.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One Rust-owned async `Bifrost` and thin blocking facade expose registration, table selection, bound writes, unbound SQL/streaming, flush, shutdown, and advanced query access. | `crates/vala/vala-sdk/src/bifrost.rs:35-465`; `blocking.rs:19-185`; private `WriterPool` remains the shared write owner. | Independent `cargo check --locked -p vala-sdk --all-features` passed; supplied SDK journey reports 12 passing tests. | PASS |
| `TableConfig` supports Rust models, Python models/Arrow schemas, TypeScript JSON Schema/Zod, and describe-by-name without a second mapper or client fingerprint. | `table.rs:75-242`; Python `bifrost/__init__.py:159-254`; TypeScript `index.ts:408-520`; every path delegates to the existing `wyrd-queue` conversion. | Independent focused Rust model test and `ts:typecheck` passed; supplied language journeys exercise Pydantic, Arrow, Zod, and describe. | PASS |
| Registration is equal-schema idempotent, rejects unequal schemas, and cannot stamp an in-flight response onto another active declaration. | `bifrost.rs:181-206` compares the still-active declaration with the exact request sent before applying the response. | Supplied Rust deterministic race test and Python conflict journey pass. | PASS |
| Omitted and explicit transport values use the existing credential/endpoint chain once; public guidance names `ClientConfig::credential`. | `bifrost.rs:560-616`; `wyrd-client/transport/credential.rs`; `wyrd-client/error.rs`. | Independent exact `no_credentials_guidance_names_the_credential_field` test passed; supplied credential tests and journeys pass. | PASS |
| No-active-table metadata has one derive-backed owner. | `wyrd-spec/src/vala/error.rs:437-451`; `vala-sdk/src/query.rs` delegates code, status, title, and remediation. | Supplied exact SDK test and language journeys pass; independent `codegen:check` passed. | PASS |
| Switching tables retains producers; flush/shutdown drain them; no per-row table/schema argument or compatibility write root remains. | `bifrost.rs:209-335`; `handle.rs`; old public split roots are absent. | Supplied Rust/Python/TypeScript journeys write and read both targets. | PASS |
| `sql` and `stream` require a success terminal and collected `QueryResult` conversions preserve the authoritative Arrow result. | Rust collection retains the stream schema (`bifrost.rs:355-373`); Python uses Rust IPC. TypeScript retains only yielded batches (`index.ts:285-370, 529-573, 686-700`), and its native stream wrapper drops `QueryResultStream::schema` (`wyrd-node/src/lib.rs:738-748`). | Supplied conversion test covers a non-empty result only. Independent Apache Arrow check confirmed `new Table([])` and its IPC round trip have zero fields. | FAIL (`FIND-BIFROST-OTEL-T03A-1`) |
| SQL optionally accepts a language-native result schema/model and returns validated instances of that schema rather than a raw `QueryResult`. | Rust, Python, and TypeScript `sql` currently accept only the SQL query and always return `QueryResult`; no typed-row projection exists. | Existing analytical journeys manually convert Arrow columns and do not exercise Pydantic, Zod, or Rust-struct result rows. | FAIL (`FIND-BIFROST-OTEL-T03A-10`) |
| The approved SQL-only deviation is implemented without a replacement typed observation route. | The candidate deletes the nine typed route/RPC/SDK stacks and retains canonical SQL, table description, and query lifecycle controls. | Source search confirms the removed typed observation symbols and routes are absent. The user explicitly approved this deviation for the reviewed candidate. | PASS |
| Materially modified Rust documentation accurately describes its public surface. | `Bifrost::query_client` still promises typed trace and GenAI reads at `bifrost.rs:400-401`, but those methods were deleted by revision 10 work. | Rust compilation cannot detect semantic rustdoc drift. | FAIL (`FIND-BIFROST-OTEL-T03A-8`) |
| Existing journey owners cover the unified workflow and required negative behavior without a new harness or target. | Existing SDK, Python, and TypeScript journey files were extended; the analytical journeys reuse `WyrdTestServer` and current language targets. | Supplied results: SDK 12, Python 27, TypeScript 9 passing. No journey proves the empty-result TypeScript schema case above. | FAIL (`FIND-BIFROST-OTEL-T03A-1`) |
| Generated surfaces and repository patch hygiene are clean. | Generated Python/TypeScript/protobuf surfaces match their current owners. The cumulative patch has extra blank EOF lines in the prior verdict and `typescript/wyrd/src/index.ts`. | Independent `ts:typecheck`, `ts:napi:check`, and `codegen:check` passed; `git diff --check 63ad8cbb8..5f4d1ea18` failed at those two files. | FAIL (`FIND-BIFROST-OTEL-T03A-9`) |
| Ponytail minimalism: delete obsolete behavior, reuse current owners/dependencies, and add no speculative framework. | The revision 10 refactor deletes the typed stack and reuses canonical SQL, `QueryClient`, Arrow, Zod's native schema export, the existing producer pool, and existing journeys. No new harness, runtime, mapper, or production dependency was added. | Diff inspection and manifests. | PASS, subject to the unresolved contract conflict. |

## Prior-finding closure

| Prior finding | Result | Evidence |
|---|---|---|
| `FIND-BIFROST-OTEL-T03A-1` | OPEN, narrowed | Rust `from_model`, blocking advanced access, and TypeScript conversion methods now exist. TypeScript conversions are still incorrect for a successful zero-row result because the authoritative schema is discarded. |
| `FIND-BIFROST-OTEL-T03A-2` | CLOSED | Existing real-server journey owners now exercise the Rust unified workflow, unequal-schema conflict, and non-SELECT/oversized refusal. |
| `FIND-BIFROST-OTEL-T03A-3` | CLOSED | Registration applies a response only to the active declaration that produced the identical request, with deterministic race coverage. |
| `FIND-BIFROST-OTEL-T03A-4` | CLOSED | `BifrostError::NoActiveTable` is the derive-backed metadata owner and SDK projections delegate to it. |
| `FIND-BIFROST-OTEL-T03A-5` | CLOSED | The identified imports are module-scoped and `client_from_env` has accurate rustdoc. `FIND-BIFROST-OTEL-T03A-8` is separate documentation drift introduced by the later typed-read deletion. |
| `FIND-BIFROST-OTEL-T03A-6` | CLOSED | Public no-credential guidance and its regression test name only `ClientConfig::credential`. |

## Material findings

### FIND-BIFROST-OTEL-T03A-1 — INCORRECT: TypeScript zero-row results lose their schema

The approved `QueryResult.toArrow()` and `toBytes()` conversions must represent the collected Arrow result. `Bifrost.sql` collects only yielded `RecordBatch` values and constructs `QueryResult(batches, terminal)` (`typescript/wyrd/src/index.ts:686-700`). `QueryResult.toArrow()` then calls `new Table([...batches])` (`560-562`). For a successful query with no rows, no batch crosses the language boundary, so the table and IPC stream have an empty schema even though the server supplied a result schema. The Rust collector explicitly retains that schema; the N-API stream wrapper currently discards it.

Observable consequence: `SELECT id, value FROM table WHERE false` succeeds but TypeScript callers cannot inspect `id` or `value`, and `toBytes()` sends a schema-less result to downstream Arrow readers. Preserve the Rust stream's authoritative schema in the TypeScript collected result and add one existing-owner journey or focused runtime check asserting the fields before and after IPC conversion for a successful zero-row query. Reuse the existing Rust/Arrow stream schema; do not create a mapper or fabricate fields in TypeScript.

### FIND-BIFROST-OTEL-T03A-8 — VIOLATION: public `query_client` rustdoc promises deleted methods

`crates/vala/vala-sdk/src/bifrost.rs:400-401` says the public accessor exposes typed trace and GenAI reads. Those methods and their contracts were deleted. A caller following the generated Rust API documentation reaches methods that do not exist. Once the read contract is resolved, make this public rustdoc describe only the surface that actually remains; the blocking facade at `blocking.rs:170-179` already contains the minimal accurate wording for the SQL-only choice.

### FIND-BIFROST-OTEL-T03A-9 — VIOLATION: the cumulative patch fails whitespace validation

`git diff --check 63ad8cbb8..5f4d1ea18` reports extra blank lines at EOF in the prior `verdict.md:105` and `typescript/wyrd/src/index.ts:770`. A clean worktree makes plain `git diff --check` inspect nothing, so the recorded pass did not verify the immutable patch. Remove only the reported blank EOF lines and rerun the range check.

### FIND-BIFROST-OTEL-T03A-10 — MISSING: `sql` cannot return language-native schema objects

The user requires an optional result schema/model alongside the SQL argument. When omitted, `sql` must keep returning the current Arrow-backed `QueryResult`. When supplied, it must validate each returned row and return instances/values produced by that language's schema mechanism: Pydantic model instances in Python, Zod-parsed typed values in TypeScript, and deserialized Rust structs through the idiomatic Rust typed form. The current methods accept only the query and expose no such row projection.

Add this at the public language facades, after the shared Rust query has completed successfully, so it cannot bypass terminal validation or become a second server read path. Reuse `QueryResult`'s Arrow data and installed language-native validation: Pydantic `model_validate`, structural Zod-style `parse`, and the installed Arrow/Serde conversion for Rust. Do not add a mapper framework, server route, durable contract, or dependency. Focused language tests must prove successful typed rows, validation failure, and unchanged raw `sql(query)` behavior.

## Verification limits

- Independently passed: `mise run ts:typecheck`, `mise run ts:napi:check`, `mise run codegen:check`, `cargo check --locked -p vala-sdk --all-features`, the exact Rust model-schema test, and the exact credential-guidance test.
- Independently demonstrated the zero-batch Arrow behavior with the installed `apache-arrow`: a `Table` constructed from no batches and its IPC round trip both report zero fields.
- The supplied implementation evidence reports the SDK, Python, and TypeScript journey lanes passing with 12, 27, and 9 tests respectively; those environment-owning journeys were not repeated during this review.
- The supplied `verify:bifrost` failure remains the pre-existing tenant-isolation check, and `gate` was not completed. Those unrelated limits do not explain the findings above.
- The cumulative range check fails as recorded in `FIND-BIFROST-OTEL-T03A-9`; worktree-only `git diff --check` is not evidence for a committed candidate.

## Verdict

`FIX_REQUIRED`

The user has approved the SQL-only deviation, so it is not a review blocker. The remediation closes five prior findings and most of the sixth, and the added refactor follows the Ponytail delete/reuse path. The remaining bounded work is packaged in `BIFROST-OTEL-T03A-R2-complete-sql-results.md`.
