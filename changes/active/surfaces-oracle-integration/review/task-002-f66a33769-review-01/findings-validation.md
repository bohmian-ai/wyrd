# TASK-002 Findings Validation

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f66a337698940920dca20b126c1c6c28a6390191`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Wave 1 inputs: `task-review.md`, `standards-review.md`, `domain-review-security-rbac-tenancy.md`, `domain-review-stream-lifecycle-durability.md`, and `domain-review-public-sdk-contracts.md` in this directory.

The candidate was `f66a337698940920dca20b126c1c6c28a6390191` before and after validation. The only worktree changes are untracked review artifacts in this review directory.

## Wave 1 disposition

| Source ID | Validation | Final finding | Reason |
|---|---|---|---|
| `TASK-REV-001` | CONFIRMED | `FIND-TASK-002-1` | The public Cards constructor and all Cards operations route `WyrdClientError` through the divergent mapping. |
| `SEC-001` | CONFIRMED | `FIND-TASK-002-1` | Duplicate of `TASK-REV-001`; the security consequence and correction boundary agree. |
| `TASK-REV-002` | CONFIRMED | `FIND-TASK-002-2` | TypeScript Cards and Bifrost connection failures cross napi as display-only errors. |
| `SEC-002` | REVISED | `FIND-TASK-002-2` | Duplicate of `TASK-REV-002`; `TableConfig.describe` is retained in the same correction because it uses the identical public failure path. |
| `TASK-REV-003` | REVISED | `FIND-TASK-002-3` | The omission is real, but REQ-059 covers all public Bifrost table, query, and lifecycle operations, not only the three newly added table paths. |
| `SEC-003` | REVISED | `FIND-TASK-002-3` | Duplicate of the structured OpenAPI finding; explicit 401/403 publication remains part of the retained correction. |
| `SDK-CONTRACT-02` | CONFIRMED | `FIND-TASK-002-3` | Correctly identifies the complete Bifrost OpenAPI boundary and existing Card-route mechanism. |
| `TASK-REV-004` | REVISED | `FIND-TASK-002-4` | The stale paths are real, and the shared `CodeFromFile` glob/path matcher must also admit `sdks`; changing call sites alone cannot work. |
| `STD-006` | REVISED | `FIND-TASK-002-4` | Duplicate of the docs regression, with its proposed path-only correction expanded to the actual shared resolver boundary. |
| `STD-001` | CONFIRMED | `FIND-TASK-002-5` | Repository authority explicitly preserves `wyrd.errors`; only the obsolete aliases were authorized for deletion. |
| `STD-002` | CONFIRMED | `FIND-TASK-002-6` | The relocated extension aggregator has no `python` boundary feature and activates PyO3 plus owner Python features unconditionally. |
| `STD-003` | CONFIRMED | `FIND-TASK-002-7` | The cited newly added or materially relocated private fields and helpers are undocumented despite the explicit all-items rule. |
| `STD-004` | CONFIRMED | `FIND-TASK-002-8` | The cited materially relocated signatures use fully qualified paths contrary to the top-level-import rule. |
| `STD-005` | REJECTED | — | `wyrd-sdk-rust` is a transparent `pub use wyrd_client::*`; the real-server `wyrd-client` journey already exercises list and the transport test exercises delete. The new Rust SDK journey proves the re-export boundary through register/get/hydrate/state. Repeating every re-exported method does not prove distinct runtime wiring and is not required by TASK-002's Card registration/loading journey criterion. |
| `SDK-CONTRACT-03` | REJECTED | — | Duplicate of `STD-005`; no operation-specific Rust projection exists between `wyrd_sdk::cards::Cards` and the already-tested owner. |
| `STD-007` | REJECTED | — | The task record omitted the commands, but independent immutable verification ran `mise run py:format:check` and `mise run py:lints`; both passed. No implementation correction remains. |
| `STREAM-001` | CONFIRMED | `FIND-TASK-002-9` | `QueryClient` and `RawQueryStream` are publicly nameable, and production CLI/Python/TypeScript callers use the sibling lifecycle owner. |
| `STREAM-002` | CONFIRMED | `FIND-TASK-002-10` | Close and producer lookup/creation are separate transitions, leaving the described accepted-but-undrained interleaving reachable. |
| `STREAM-003` | CONFIRMED | `FIND-TASK-002-11` | The failed-terminal branch settles immediately and uniquely skips the clean-EOF proof used by success terminals. |
| `STREAM-004` | REVISED | `FIND-TASK-002-12` | A coalesced body item causes all contained Arrow batches to be decoded into `pending`. The correction is narrowed to lazy one-frame/one-batch decoding; no new queue limit or transport rejection policy is needed. |
| `SDK-CONTRACT-01` | REVISED | `FIND-TASK-002-13` | The cross-surface range mismatch is confirmed. Retaining a wide signed wire field and validating the closed range centrally avoids turning ordinary out-of-range HTTP/Python values into binding/deserialization errors. |

Wave 1 supplied no Suggestions or Open Questions requiring separate disposition. Positive controls and verification-limit observations that do not allege a task defect remain non-findings.

## Validated finding ledger

### FIND-TASK-002-1 — CONFIRMED — INCORRECT: Cards misclassifies shared client failures

- **Wave 1 sources:** `TASK-REV-001`, `SEC-001`.
- **Violated obligation:** REQ-023, REQ-024A, AC-002, and TASK-002's requirement that converged public failures preserve catalog identity.
- **Exact location:** `crates/shared/wyrd-client/src/cards/error.rs:31-50`; reached by `Cards::new` at `crates/shared/wyrd-client/src/cards/handle.rs:153-160`, later Cards transport operations throughout `handle.rs`, and the public Python Cards constructor at `sdks/wyrd-sdk-python/src/state/mod.rs:1464-1492`.
- **Evidence:** `RegistryEngineError::Client(Config)` and `NoCredentials` become `WyrdError::Internal`; `TransportDown` becomes `RegistryUnavailable`. The exact `ClientConfigInvalid`, `ClientNoCredentials`, and `ClientTransportDown` catalog variants exist in `crates/wyrd-spec/src/error.rs`. Bifrost separately maps the same three variants correctly in `crates/shared/wyrd-client/src/bifrost/query.rs:151-161`.
- **Observable consequence:** Cards can return `WYRD_INTERNAL_500` for missing credentials and a registry error for a client transport outage, while Bifrost returns the promised `WYRD_CLIENT_*` codes for the same shared-client failures.
- **Decision-complete minimum correction:** make `crates/shared/wyrd-client/src/error.rs` the single owner of `WyrdClientError` to `WyrdError` conversion, mapping the three variants to their existing `WYRD_CLIENT_*` catalog entries; delete both local projections and reuse the canonical conversion from Cards and Bifrost. Preserve server-originated registry errors unchanged.
- **Focused closure proof:** environment-isolated Rust assertions for Cards missing credentials, invalid explicit URL, and representative transport failure, plus a public Python Cards missing-credential assertion, checking exact code/status without message parsing.

### FIND-TASK-002-2 — CONFIRMED — INCORRECT: TypeScript connection failures bypass runtime `WyrdError`

- **Wave 1 sources:** `TASK-REV-002`, `SEC-002`.
- **Violated obligation:** REQ-021, REQ-023, REQ-025, AC-002, and the TypeScript structured-error acceptance criterion.
- **Exact location:** `sdks/wyrd-sdk-ts/native/src/cards.rs:34-47`; `sdks/wyrd-sdk-ts/native/src/lib.rs:338-353,403-423,990-1001`; public callers at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:651-666,965-981`.
- **Evidence:** `connect_cards`, `connect_bifrost`, and `describe_table_config` use `napi_error`, retaining only display text. Their public TypeScript wrappers return the rejection directly. Operation methods instead use the existing metadata fields consumed by `projectedError`/`lifecycleValue` at `index.ts:227-271`.
- **Observable consequence:** missing credentials, invalid configuration, and connection/describe failures are generic JavaScript errors with no stable code, status, title, remediation, or details.
- **Decision-complete minimum correction:** project the three public construction/description paths through the same closed success-or-catalog-metadata convention already used by native query startup and lifecycle results, then have the TypeScript wrappers call `projectedError` and throw the public `WyrdError`. Do not encode or parse metadata in display strings; keep intrinsic napi serialization failures as napi errors.
- **Focused closure proof:** public TypeScript tests for `Cards.connect`, `Bifrost.connect`, and `TableConfig.describe` with absent credentials, asserting `instanceof WyrdError`, `WYRD_CLIENT_401_NO_CREDENTIALS`, status 401, and nonempty title/remediation; one representative invalid-config or transport case proves the shared path.

### FIND-TASK-002-3 — REVISED — MISSING: Bifrost OpenAPI operations omit structured refusal bodies

- **Wave 1 sources:** `TASK-REV-003`, `SEC-003`, `SDK-CONTRACT-02`.
- **Violated obligation:** REQ-059 and AC-002 require complete table, query, lifecycle, permission, and structured-error authority.
- **Exact location:** `crates/wyrd/wyrd-server/src/bifrost/routes.rs:21-77`; `crates/wyrd/wyrd-server/src/query/routes.rs:90-199`; generated `openapi.yaml`; path-only test at `crates/wyrd/wyrd-server/src/http/openapi.rs:56-72`.
- **Evidence:** table register/list publish only 200 and describe has a body-less 404. Query running publishes only 200; status/cancel have body-less 404; query has a body-less 503. The handlers return `WyrdErrorResponse`/problem JSON, and Card routes already attach `WyrdProblem` to explicit non-success responses.
- **Observable consequence:** generated clients and agents cannot type authentication, permission, validation, conflict, not-found, availability, or other pre-stream Bifrost refusals even though runtime callers receive structured Wyrd problems.
- **Decision-complete minimum correction:** reuse the Card-route utoipa pattern across all seven Bifrost operations. Import `WyrdProblem`; attach it to every existing non-2xx response; explicitly publish 400/401/403 on each applicable operation, 409 on register and lifecycle-owner conflicts, 404 on describe/status/cancel, and 503 on availability paths; retain a typed default `WyrdProblem` response for other catalog-backed pre-stream failures rather than duplicating the whole error catalog in every annotation. Runtime terminal frames remain the query body's terminal contract, not HTTP problem responses.
- **Focused closure proof:** extend the existing OpenAPI unit test to iterate all seven operations and assert their 401/403 and default problem content reference `#/components/schemas/WyrdProblem`, with route-specific 404/409/503 assertions; regenerate and pass `mise run codegen:check`.

### FIND-TASK-002-4 — REVISED — REGRESSION: relocated Python examples break two docs routes

- **Wave 1 sources:** `TASK-REV-004`, `STD-006`.
- **Violated obligation:** INV-001, the SDK-root move, and repository completion rules requiring valid current docs paths.
- **Exact location:** `docs/src/content/docs/how-to/build-an-agent.svx:18,52`; `docs/src/content/docs/how-to/build-a-workflow.svx:18,74,92`; `docs/src/lib/components/CodeFromFile.svelte:20-28,45-50`.
- **Evidence:** all five embeds name deleted `python/py-wyrd/examples/**` paths. The files moved to `sdks/wyrd-sdk-python/examples/**`, while the component's eager glob and `repoTail` matcher admit only `crates`, `examples`, and `python`. The independent Wave 1 docs build reached deterministic prerender 500s for both pages.
- **Observable consequence:** `/wyrd/how-to/build-an-agent/` and `/wyrd/how-to/build-a-workflow/` render as error pages instead of containing their Python examples.
- **Decision-complete minimum correction:** add `sdks` to the existing `CodeFromFile` glob and `repoTail` top-level matcher, then retarget only the five stale embeds to `sdks/wyrd-sdk-python/examples/**`. Reuse the moved examples; add no copies or new docs loader.
- **Focused closure proof:** `mise run docs:check` plus a focused build assertion that both routes have no prerender 500 and their generated output contains an embedded Python example.

### FIND-TASK-002-5 — CONFIRMED — VIOLATION: the canonical `wyrd.errors` projection was deleted

- **Wave 1 source:** `STD-001`.
- **Violated obligation:** `architecture/references/languages/python-api-and-stubs.md` Package Layout and AGENTS.md §8 require the public structured-exception module and matching runtime/type surface; REQ-024B prohibits the old aliases, not the module.
- **Exact location:** deleted `python/py-wyrd/python/wyrd/errors.py`; absent `sdks/wyrd-sdk-python/python/wyrd/errors.py`.
- **Evidence:** the base module exported `WyrdError` and three fake `Cfg*` aliases. Candidate root imports still expose `wyrd.WyrdError`, but `from wyrd.errors import WyrdError` now fails and no `errors` module exists at the authority-defined SDK layout.
- **Observable consequence:** the documented module import breaks even though the underlying single exception type remains available.
- **Decision-complete minimum correction:** add one `sdks/wyrd-sdk-python/python/wyrd/errors.py` projection that imports and exports only `WyrdError` from `._wyrd`. Do not restore `Cfg*` aliases, subclasses, or a second metadata projector. The typed Python source itself is sufficient; do not hand-edit generated stubs merely to duplicate the re-export.
- **Focused closure proof:** one public import/identity assertion proving `wyrd.errors.WyrdError is wyrd.WyrdError`, followed by Python unit/typecheck and codegen checks.

### FIND-TASK-002-6 — CONFIRMED — VIOLATION: the relocated PyO3 boundary is unconditional

- **Wave 1 source:** `STD-002`.
- **Violated obligation:** AGENTS.md §7 and `architecture/references/languages/pyo3-boundaries.md` require relocated PyO3 under the Python SDK's optional `python` boundary feature; Cargo features must be explicit.
- **Exact location:** `sdks/wyrd-sdk-python/Cargo.toml:17-47` and the ungated extension root at `sdks/wyrd-sdk-python/src/lib.rs`.
- **Evidence:** the manifest has only `testing`; `pyo3` is non-optional, and ten retained owner-crate `python` features activate in normal dependencies. Merely selecting `wyrd-sdk-python` therefore selects the foreign runtime cone.
- **Observable consequence:** the SDK has no Python-free Rust/default mode and violates the required optional foreign-runtime boundary.
- **Decision-complete minimum correction:** add the single `python` feature prescribed by repository authority; make `pyo3` optional; move every retained owner-crate `/python` activation under that feature; gate the extension modules/items so the default crate compiles without PyO3; make `testing` depend on `python` and the test-only harness; and make maturin/Python setup and wheel lanes explicitly enable `python`. Add no additional feature hierarchy.
- **Focused closure proof:** default-feature and `--features python` dependency trees showing PyO3 absent/present respectively, plus `lint:wyrd-sdk-python`, `check:pyo3-scope`, production-wheel, Python build, and Python unit checks.

### FIND-TASK-002-7 — CONFIRMED — VIOLATION: materially relocated Rust items lack required rustdoc

- **Wave 1 source:** `STD-003`.
- **Violated obligation:** AGENTS.md §16 and `architecture/agent-rules.md` require intent-level rustdoc on every new or materially modified Rust item, including private fields/helpers, and `# Errors` on fallible functions.
- **Exact location:** representative direct violations at `sdks/wyrd-sdk-python/src/lib.rs:105`; `sdks/wyrd-sdk-python/src/state/registry.rs:11-13`; `sdks/wyrd-sdk-python/src/state/mod.rs:1098-1107,1169-1197,1873-1909,2671-2852`; and `crates/shared/wyrd-client/src/cards/handle.rs:140-144`.
- **Evidence:** `register_submodule`, `PythonCardRegistry.registry`, `Cards.engine`, `PyVersionBump.native`/`as_native`, the four option `values` fields, and the cited fallible boundary helpers have no rustdoc/required error sections. Diff inspection shows these are added or materially relocated in the candidate. Their callers are the SDK module registration, all Python Card registry variants, option constructors, holder stamping/selection, and loader registration path; they are live production boundary code, not dormant helpers.
- **Observable consequence:** the candidate violates an explicit `BLOCK_BEFORE_MERGE` repository rule and leaves maintainers without the invariants/side effects required to modify these boundary workflows safely.
- **Decision-complete minimum correction:** document the cited items and every other undocumented item added or materially relocated in the same touched Rust modules, describing workflow role, invariants/side effects, and required `# Errors`, `# Panics`, or cancellation behavior. Change documentation only; do not extract helpers or add a permanent grep gate.
- **Focused closure proof:** Rust format/lints/doc generation plus review of the added/relocated private-item diff confirming no undocumented item remains.

### FIND-TASK-002-8 — CONFIRMED — VIOLATION: relocated Rust signatures hide dependencies in qualified paths

- **Wave 1 source:** `STD-004`.
- **Violated obligation:** `architecture/agent-rules.md` requires module-top imports and bare type names in fields, parameters, returns, bounds, and `where` clauses.
- **Exact location:** `crates/shared/wyrd-client/src/state.rs:507,586,614,1181`; `sdks/wyrd-sdk-python/src/state/mod.rs:2708,2796`; `sdks/wyrd-sdk-python/src/bifrost/mod.rs:576,588,609,1002`; `sdks/wyrd-sdk-ts/native/src/lib.rs:247,272,283,969`.
- **Evidence:** these materially relocated signatures use qualified `wyrd_spec`, `skald_spec`, `wyrd_client`, `arrow`, and `arrow_schema` types while their modules already maintain top-level import blocks.
- **Observable consequence:** the changed modules fail an explicit repository structural rule and obscure their dependency manifest.
- **Decision-complete minimum correction:** add the named types to each existing module-top `use` block and replace only the cited signature paths with bare names. Do not introduce aliases unless two imported types actually collide.
- **Focused closure proof:** source search over the touched signatures followed by `mise run fmt` and `mise run lints`.

### FIND-TASK-002-9 — CONFIRMED — VIOLATION: query lifecycle remains a sibling public client surface

- **Wave 1 source:** `STREAM-001`.
- **Violated obligation:** REQ-019, INV-005, TASK-002's explicit non-exposure constraint, and `architecture/bifrost-design.md` require `Bifrost` to own public query streaming and lifecycle while `QueryClient` remains internal.
- **Exact location:** public exports at `crates/shared/wyrd-client/src/bifrost/mod.rs:47-50`; escape hatch at `crates/shared/wyrd-client/src/bifrost/facade.rs:475-483`; blocking escape hatch at `crates/shared/wyrd-client/src/bifrost/blocking.rs:188-193`; production callers in `crates/wyrd/wyrd-cli/src/query/mod.rs:104-106`, `sdks/wyrd-sdk-python/src/bifrost/mod.rs:438-529`, and `sdks/wyrd-sdk-ts/native/src/lib.rs:607-704`.
- **Evidence:** `QueryClient` and `RawQueryStream` are publicly nameable. `Bifrost` lacks the raw request, running, status, cancel, and bounded-collection methods, so CLI and both foreign boundaries call `query_client()` directly. `QueryClient::new` being crate-private does not make the returned public type internal.
- **Observable consequence:** public lifecycle authority is split and Rust callers cannot discover the required capability set solely on `Bifrost`.
- **Decision-complete minimum correction:** put `query(&BifrostQueryRequest)`, `collect_bounded`, `running`, `status`, and `cancel` inherent methods on async `Bifrost`, with matching blocking methods where applicable; retain existing `stream(&str)`/`sql`/`describe`; route CLI, Python, TypeScript, and repository test/support callers through those methods; make `query_client` crate-private; and remove public exports of `QueryClient` and `RawQueryStream`. Keep `QueryResultStream` public because it is the facade's returned stream, and keep the internal `QueryClient` as the focused transport/settlement mechanic rather than moving its logic.
- **Focused closure proof:** a Rust public-surface compile test naming every required operation through `Bifrost`, source/API checks proving the sibling types are not publicly nameable, and the existing CLI/Python/TypeScript lifecycle journeys.

### FIND-TASK-002-10 — CONFIRMED — INCORRECT: shutdown can miss a concurrently admitted producer

- **Wave 1 source:** `STREAM-002`.
- **Violated obligation:** TASK-002's flush/shutdown durability and preservation of Oracle drain/settlement behavior.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/handle.rs:170-183,267-289,307-336`; reached by `Bifrost::insert` and `Bifrost::shutdown` in `facade.rs:299-313,364-385`, then by Rust, Python, TypeScript, blocking, and observe callers.
- **Evidence:** `insert` reads `closed` before taking the producer-map lock. `shutdown` stores `closed`, snapshots under that lock, releases it, drains only the snapshot, then retains any non-drained entry. An insert can pass the first check, pause, and create/enqueue to a producer after shutdown's snapshot.
- **Observable consequence:** both insert and shutdown can report success while the accepted row remains buffered in a producer that shutdown never drained.
- **Decision-complete minimum correction:** make admission and producer lookup/creation observe closure under the existing producer-map mutex: recheck `closed` while holding that mutex before returning an existing producer or creating a new one, and take the shutdown close/snapshot transition under the same mutex. Reuse the existing lock and atomic flag; add no second registry or coordination type. Producer enqueue remains responsible for rejecting a row racing its own drain after lookup.
- **Focused closure proof:** one deterministic unit concurrency test that holds first-producer lookup across shutdown and proves either refusal or durable acknowledgement before shutdown returns, then asserts zero retained producers and dynamic ownership.

### FIND-TASK-002-11 — CONFIRMED — INCORRECT: failed terminals skip the exactly-one-terminal proof

- **Wave 1 source:** `STREAM-003`.
- **Violated obligation:** REQ-056 and the Bifrost terminal contract require exactly one terminal and reject every post-terminal frame.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/query.rs:750-834`; current incomplete proof at `query.rs:2622-2643`; consumed by facade collection, CLI, Python iteration, TypeScript iteration, and all direct `QueryResultStream::next_batch` callers.
- **Evidence:** success/degraded terminals call `finish_at_clean_eof`; a failed terminal is immediately retained, marks settlement `Settled`, and returns `FailedTerminal`. A following terminal, batch, or schema is never polled through the converter.
- **Observable consequence:** a malformed failed-terminal-plus-frame stream is reported as a validated server failure instead of an incomplete/protocol stream, so the client has not enforced exactly one terminal.
- **Decision-complete minimum correction:** route failed terminals through the existing server-deadline-bounded clean-EOF helper before retention/settlement. On clean EOF, retain and return `FailedTerminal`; on any following frame or broken/late EOF, use the existing broken-stream error and settlement path. Do not add a second EOF algorithm.
- **Focused closure proof:** one focused test covering a failed terminal followed by another frame (including a duplicate terminal case) and asserting protocol/incomplete failure, no premature retained terminal, and exactly-once settlement; keep the existing clean failed-terminal test green.

### FIND-TASK-002-12 — REVISED — VIOLATION: one body item eagerly decodes multiple owned Arrow batches

- **Wave 1 source:** `STREAM-004`.
- **Violated obligation:** TASK-002's preserved bounded Arrow ownership and `architecture/references/domain/arrow-analytical-interop.md`'s one-`RecordBatch` ownership/backpressure unit.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/query.rs:408-425,477-542`; `FrameDecoder::push` behavior at `crates/wyrd/wyrd-tonic/src/frame_codec.rs:69-110`; misleading proof at `query.rs:2690-2766`.
- **Evidence:** one arbitrary response-body item is expanded into every complete protobuf frame, and the loop immediately feeds every batch fragment through the stateful Arrow decoder before pushing all resulting `(frame, RecordBatch)` pairs into an unconstrained `VecDeque`. HTTP application frame boundaries are not transport chunk boundaries. `peak_pending_frame_bytes` measures fragment scratch, not decoded batches waiting in `pending`.
- **Observable consequence:** coalescing can retain many decoded Arrow batches before yielding one, moving backpressure from the declared batch unit to a transport-chunk-dependent group.
- **Decision-complete minimum correction:** keep pending protobuf frames encoded and run converter/Arrow decoding only when one frame is popped for delivery, so at most one decoded `RecordBatch` exists in the Wyrd-owned pending path. Reuse `FrameDecoder`, `QueryStreamConverter`, and `QueryIpcDecoder`; do not introduce a configurable queue, new dependency, or transport-size refusal. Preserve frame order, row accounting, terminal validation, and stateful IPC decoding.
- **Focused closure proof:** one stream test supplies schema, many batch frames, and terminal in a single body item, proves all rows/order/terminal behavior, and records that peak pending decoded-batch ownership never exceeds one.

### FIND-TASK-002-13 — REVISED — INCORRECT: public query deadline domains disagree

- **Wave 1 source:** `SDK-CONTRACT-01`.
- **Violated obligation:** the specification's externally observable deadline contract and `architecture/bifrost-design.md` require `1..=u32::MAX` milliseconds across Rust, HTTP, gRPC, Python, TypeScript, and MCP.
- **Exact location:** `crates/wyrd-spec/src/vala/api.rs:526-555`; generated `openapi.yaml:893-907`; Python input at `sdks/wyrd-sdk-python/src/bifrost/mod.rs:422-438`; TypeScript input at `sdks/wyrd-sdk-ts/native/src/lib.rs:20-30,593-607`; MCP input/projection at `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:254-334`.
- **Evidence:** the shared request and Python accept `u64` and reject only zero; OpenAPI publishes int64 with an incorrect zero minimum and no maximum. TypeScript/MCP accept `u32`. Thus `u32::MAX + 1` is accepted by Rust/HTTP/Python but cannot enter TypeScript/MCP's shared validation path.
- **Observable consequence:** first-class surfaces disagree on valid input and on whether an out-of-range deadline is a structured Wyrd validation failure or a foreign binding error.
- **Decision-complete minimum correction:** use one wide signed shared request field capable of representing ordinary below/above-range inputs, validate `1..=u32::MAX` synchronously in `BifrostQueryRequest::validate`, and publish the same minimum/maximum in generated schema. Python should accept that signed boundary value and delegate to shared validation. The TypeScript napi request should accept a JavaScript number, reject non-finite/non-integer/out-of-range values into the existing structured startup result, then cast; MCP's existing `u32` parser remains valid and projects losslessly. Update gRPC/wire conversions without changing the valid public range.
- **Focused closure proof:** shared contract and generated-schema assertions for 0, 1, `u32::MAX`, and `u32::MAX + 1`; public Python and TypeScript tests asserting the same catalog-backed validation code for the upper overflow; retain the MCP boundary tests.

## Validation result

The deduplicated ledger contains thirteen bounded implementation findings. None requires a new product, public API, architecture, security, compatibility, cross-service, concurrency-semantics, resource-ownership, or persistent-data decision: each correction is fixed by approved authority and an existing owner/mechanism. The appropriate review verdict is therefore `FIX_REQUIRED`, not `SPEC_REVISION_REQUIRED` or `BLOCKED`.

Independent verification added during Wave 2:

- `mise run py:format:check` — PASS (`121 files already formatted`)
- `mise run py:lints` — PASS (`All checks passed`)
- `git diff --check 861f8d86c..f66a33769` — PASS
