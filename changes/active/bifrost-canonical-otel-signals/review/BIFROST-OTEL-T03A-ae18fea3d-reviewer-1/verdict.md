---
task: BIFROST-OTEL-T03A
verdict: FIX_REQUIRED
base: 63ad8cbb8a03c7a83cf357a23fed794521ecbbb1
candidate: ae18fea3dd39173763596ba825fdfe8ba3eeae76
reviewer: reviewer-1
---

# BIFROST-OTEL-T03A task review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/bifrost-canonical-otel-signals/spec.md`, revision 9
- Original task: `changes/active/bifrost-canonical-otel-signals/tasks/03a-unified-bifrost-client.md`
- Approved interface reference: `changes/active/bifrost-canonical-otel-signals/unified-bifrost-client-draft.md`
- Base: `63ad8cbb8a03c7a83cf357a23fed794521ecbbb1`, the parent of the first implementation commit
- Candidate: `ae18fea3dd39173763596ba825fdfe8ba3eeae76`
- Reviewed range: `63ad8cbb8..ae18fea3d` (52 files, 4,552 insertions, 2,287 deletions)
- Candidate stability: `HEAD` remained `ae18fea3dd39173763596ba825fdfe8ba3eeae76` throughout review. The tree was clean before review artifacts were written.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One Rust-owned async `Bifrost` owns registration, active-table writes, unbound reads, streaming, flush, and shutdown. | `crates/vala/vala-sdk/src/bifrost.rs:27-432`; private `WriterPool` in `handle.rs`. | Candidate `cargo check -p vala-sdk` passes; supplied SDK journey lane reports 10 passing tests. | PASS |
| Rust exposes the approved `TableConfig` model/schema construction paths. | `table.rs:77-105` provides Arrow and JSON Schema, but the approved `from_model<T: schemars::JsonSchema>` constructor is absent. | No Rust compile fixture exercises the missing constructor. | FAIL (`FIND-BIFROST-OTEL-T03A-1`) |
| The blocking facade exposes the approved unified workflow, including advanced-query access. | `blocking.rs:26-166` delegates lifecycle, writes, SQL, and stream, but exposes neither `query()` nor an equivalent advanced-query handle. | Candidate compile succeeds because no fixture requires the missing surface. | FAIL (`FIND-BIFROST-OTEL-T03A-1`) |
| TypeScript `QueryResult` exposes the approved Arrow and byte conversions. | `typescript/wyrd/src/index.ts:600-623` exposes batches, terminal, and row count only; `toArrow()` and `toBytes()` are absent from source and declarations. | Independent `mise run ts:typecheck` passes, showing the typing fixture does not cover these required methods. | FAIL (`FIND-BIFROST-OTEL-T03A-1`) |
| Python and TypeScript are thin, idiomatic projections of the Rust owner. | PyO3 and N-API call `vala_sdk::Bifrost`, `TableConfig`, and the Rust stream owner; language layers perform boundary conversion. | Supplied Python and TypeScript journey lanes report 24 and 8 passing tests; `py:typecheck`, `ts:typecheck`, client-tier, and PyO3-scope checks pass. | PASS |
| Registration resolves server-issued identity, is idempotent for equal schema, and returns stable conflict for unequal schema. | `bifrost.rs:177-195` delegates registration and records the response; Python and TypeScript equal-schema journeys re-register successfully. The response is applied to whichever table is active after the await rather than the table whose request was sent. | Equal-schema behavior is covered; no unified-client conflict journey or concurrent-swap check exists. | FAIL (`FIND-BIFROST-OTEL-T03A-2`, `FIND-BIFROST-OTEL-T03A-3`) |
| Omitted transport resolves once; an explicit credential overrides ambient sources; API keys exchange and opaque tokens pass through. | `bifrost.rs:536-572`, `config.rs:67-118`, and `credential.rs:75-101`. | Supplied Python environment/missing-credential journeys and `wyrd-client` credential tests pass. | PASS |
| No-active-table, bounded backpressure, and optional correlation preserve stable behavior. | `bifrost.rs:240-272`; `Correlation` at `table.rs:283-294`; optional queue row at `wyrd-queue/src/queue.rs`; queue-full propagation in `handle.rs`. | Existing SDK unit coverage and Python/TypeScript no-active-table plus Python uncorrelated-row journeys cover behavior. | PASS, subject to catalog violation below |
| No-active-table uses the derive-backed public error catalog. | `query.rs:55-57, 131, 145, 160, 181-183` invents the public code/status/title/remediation locally; `wyrd-spec::vala::error::BifrostError` has no variant for it. | Language tests prove the local projection, not catalog ownership or generated metadata. | FAIL (`FIND-BIFROST-OTEL-T03A-4`) |
| Switching tables retains both producers; flush and shutdown drain all owned producers and preserve first failure evidence. | `handle.rs:184-257`; `bifrost.rs:274-323`. | SDK unit tests cover swap retention and all-producer drain; Python and TypeScript journeys read both targets. | PASS |
| `sql` and `stream` share terminal-safe behavior and never return partial rows as success. | `bifrost.rs:343-386`; `query.rs:867-1087`; Python/TypeScript streams wrap the Rust owner. | Supplied query journeys cover success, missing terminal, cancellation, and denial. | PASS |
| The Rust, Python, and TypeScript journey owners prove the complete unified workflow through a real server, with required refusal coverage at the nearest existing owner. | Python `test_register_insert_flush_read_and_swap` and TypeScript `bifrost-write.test.ts` cover the full workflow. `pg_bifrost_e2e.rs` contains no unified-client `register()` call and its new swap/backpressure use is mock-sink based. The reviewed language lanes contain no unequal-schema registration conflict and no non-SELECT or oversized-query case through the unified client. | Supplied lane counts do not prove the missing scenarios. | FAIL (`FIND-BIFROST-OTEL-T03A-2`) |
| Superseded split roots, per-row table/schema arguments, former public write-pool name, and compatibility aliases are absent; `observe::record` remains explicit-table/drop-on-full. | Old Python/TypeScript roots are removed; `WriterPool` is private; insert binds through `TableConfig`; `observe.rs` retains the separate path. | Source search plus client-tier/type checks. | PASS |
| Server remains authoritative for table UID, fingerprint, durable state, authorization, correlation, and terminals; mappings are named rather than positional. | `TableConfig` only stores identities returned by register/describe; `wyrd-queue` mapping owners are reused. | Supplied codegen and boundary checks pass. | PASS, except the concurrent response misbinding in `FIND-BIFROST-OTEL-T03A-3` |
| REQ-001, REQ-004, REQ-009, REQ-017, REQ-021, and REQ-022 remain satisfied within this task's client scope. | Existing server authorities are reused; no alternate durable path or compatibility storage was added; typed trace/GenAI reads remain on `QueryClient`; correlation is optional. | Supplied Scribe, Oracle, server, MCP, and language journeys pass. | PASS |
| INV-003, INV-006, INV-007, INV-008, INV-010, and INV-011 remain preserved. | Named schema converters, authenticated server paths, durable producer ACKs, existing harnesses, and optional Card correlation are reused. | Supplied boundary and journey evidence; no relevant regression found in the cumulative diff. | PASS |
| No root multi-service client, multi-table insert, client fingerprint, context manager, global config, new harness/runtime/framework/schema mapper, migration, or compatibility path is added. | Cumulative diff contains none of the prohibited additions. | Source and manifest inspection. | PASS |
| New/materially modified Rust follows repository import and rustdoc rules. | Function-local imports occur at `credential.rs:94` and `query.rs:239,249,259,279,291,304`; new `client_from_env` at `bifrost.rs:536` has no rustdoc because its intended docs are attached to `register_outcome_name`. | `cargo check` and lints do not enforce these repository-only requirements. | FAIL (`FIND-BIFROST-OTEL-T03A-5`) |
| The `ClientConfig::api_key` rename leaves no stale public instruction. | The field is renamed in code, but `wyrd-client/src/error.rs:57-63` still tells callers to use the removed `ClientConfig::api_key`. | Existing tests assert only the code, not the remediation text. | FAIL (`FIND-BIFROST-OTEL-T03A-6`) |
| Generated surfaces and required verification are complete. | Python stubs and N-API declarations are present. | Supplied `fmt`, all-feature lints, codegen, boundary, Python/TypeScript typechecks, and three language lanes pass. No successful candidate evidence was supplied for `ts:napi:check`, `verify:bifrost`, or `gate`; reported failures in broader lanes were reproduced as pre-existing and are not implementation findings. | FAIL because required focused behavior and candidate-wide proof remain incomplete |

## Material findings

### FIND-BIFROST-OTEL-T03A-1 — MISSING: approved public client surface is incomplete

- Violated obligation: preserve the approved `TableConfig`, blocking `Bifrost`, and TypeScript `QueryResult` surfaces.
- Evidence: `table.rs:64-140` has no `from_model`; `blocking.rs:26-166` has no advanced-query accessor; `typescript/wyrd/src/index.ts:600-623` has neither `toArrow()` nor `toBytes()`.
- Observable consequence: documented Rust model-first usage does not compile, blocking Rust callers cannot reach lifecycle/typed advanced reads without consuming the facade, and TypeScript callers cannot use the approved collected-result conversions.
- Required correction: add only those approved projections, delegating to the existing JSON-Schema conversion, async `QueryClient`, and installed Apache Arrow implementation, with compile/type fixtures that call them.

### FIND-BIFROST-OTEL-T03A-2 — MISSING: required unified-client journey proof is incomplete

- Violated obligation: each first-class language journey owner proves the complete unified workflow, with schema-conflict and query-floor refusals at the nearest credible existing owners.
- Evidence: `pg_bifrost_e2e.rs` has no `Bifrost::register()` call and uses mock sinks for the newly adapted swap/backpressure cases; the reviewed Rust/Python/TypeScript language journeys contain no unequal-schema registration conflict and no non-SELECT or oversized query through the unified client.
- Observable consequence: the Rust public registration/swap/read seam and required stable refusals can regress while every reported language lane remains green.
- Required correction: extend the existing Rust and language journey owners—without a new target or harness—to cover the missing real-server workflow and refusal paths.

### FIND-BIFROST-OTEL-T03A-3 — INCORRECT: a delayed registration response can resolve the wrong active table

- Violated obligation: registration resolves server-issued identity for the table that was registered, while active-table swaps remain safe.
- Evidence: `bifrost.rs:177-187` snapshots one table's request, awaits HTTP, then `bifrost.rs:188-193` unconditionally resolves the currently active `TableConfig`. `use_table` is synchronous and may run during that await.
- Observable consequence: registering table A while another caller switches to table B can stamp A's UID and fingerprint onto B, creating a client-side identity that the server never issued for B.
- Required correction: bind the response to the exact configuration/request that produced it and never mutate a different active binding; add one deterministic delayed-response swap check.

### FIND-BIFROST-OTEL-T03A-4 — VIOLATION: the new public no-active-table error bypasses the catalog

- Violated obligation: public cross-language errors use the derive-backed `wyrd-spec` catalog; metadata is not hand-written in SDK code.
- Evidence: `query.rs:55-57,131,145,160,181-183` locally defines `WYRD_VALA_412_NO_ACTIVE_TABLE` and its metadata. `crates/wyrd-spec/src/vala/error.rs` owns public Bifrost errors but contains no corresponding variant.
- Observable consequence: generated error metadata and SDK projections have separate authorities and can drift.
- Required correction: make the approved error a derive-backed Bifrost catalog variant and project the SDK refusal from that owner; regenerate and verify affected artifacts.

### FIND-BIFROST-OTEL-T03A-5 — VIOLATION: changed Rust breaks mandatory import and rustdoc rules

- Violated obligation: all imports live at module top, and every new Rust item has intent-bearing rustdoc.
- Evidence: function-local imports at `credential.rs:94` and `query.rs:239,249,259,279,291,304`; `bifrost.rs:513-536` attaches the ambient-client documentation to `register_outcome_name`, leaving `client_from_env` undocumented.
- Observable consequence: the changed modules hide dependencies and fail the repository's hard documentation completion standard despite compiling.
- Required correction: move the aliases/trait import to module scope and attach accurate rustdoc to each function.

### FIND-BIFROST-OTEL-T03A-6 — REGRESSION: no-credentials remediation names a removed field

- Violated obligation: `ClientConfig::api_key` is fully replaced by `ClientConfig::credential`, with no compatibility vocabulary.
- Evidence: `crates/shared/wyrd-client/src/error.rs:57-63` still documents and emits `pass ClientConfig::api_key`.
- Observable consequence: callers receiving `WYRD_CLIENT_401_NO_CREDENTIALS` are instructed to configure a field that no longer exists.
- Required correction: update the public documentation and error text to the canonical `credential` field and pin the emitted remediation/message in the existing error test owner.

## Prior-finding closure

Not applicable. This is the first acceptance review of candidate `ae18fea3d` for BIFROST-OTEL-T03A.

## Verification limits

- Independently run during review: `mise exec -- cargo check --locked -p vala-sdk` — PASS; `mise run ts:typecheck` — PASS; `git diff --check` — PASS.
- Supplied successful evidence: Rust/Python/TypeScript Bifrost journey lanes, formatting, all-feature lints, codegen, client-tier, PyO3-scope, Python typecheck, TypeScript typecheck, and the named Scribe/Oracle/server/MCP/Forge-scale lanes.
- The supplied record does not contain a successful final `ts:napi:check`, `verify:bifrost`, or `gate`. Reported broad-lane failures reproduced outside this task's behavior are recorded as limits, not findings and not requested remediation.
- Passing compile/type/codegen checks do not cover the missing methods, concurrent registration swap, catalog ownership, or missing journey scenarios above.

## Verdict

`FIX_REQUIRED`

