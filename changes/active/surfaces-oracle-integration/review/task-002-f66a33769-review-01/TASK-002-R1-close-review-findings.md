---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-surfaces-oracle-integration
spec_revision: 7
requirements: [REQ-019, REQ-021, REQ-023, REQ-024A, REQ-024B, REQ-025, REQ-056, REQ-059, INV-001, INV-005, AC-002]
depends_on: [TASK-002]
parent_task: TASK-002
remediates: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3, FIND-TASK-002-4, FIND-TASK-002-5, FIND-TASK-002-6, FIND-TASK-002-7, FIND-TASK-002-8, FIND-TASK-002-9, FIND-TASK-002-10, FIND-TASK-002-11, FIND-TASK-002-12, FIND-TASK-002-13]
---

# Close TASK-002 review findings

## Authority and candidate

- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Review: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Reviewed candidate: `f66a337698940920dca20b126c1c6c28a6390191`
- Implementation route: `$wyrd-implement`

## Outcome

Finish the approved convergence without changing its architecture: `wyrd-client` remains the sole Rust client owner, `Bifrost` becomes its complete public query/lifecycle facade, all language and generated surfaces preserve the same catalog errors and input domains, and bounded stream/write ownership survives races and transport chunking. Restore the small SDK-relocation surfaces required by repository authority and make the affected docs usable.

## Issue diagnoses and required corrections

### `FIND-TASK-002-1` — Cards shared-client errors

Cards maps `Config` and `NoCredentials` to `WYRD_INTERNAL_500` and `TransportDown` to a registry error in `crates/shared/wyrd-client/src/cards/error.rs:31-50`, although matching `WYRD_CLIENT_*` catalog variants already exist and Bifrost maps them correctly. This is reachable from `Cards::new`, later transport operations, and Python Cards construction, so equivalent shared-client failures disagree by facade.

Make the shared client error owner provide the single catalog conversion and reuse it from Cards and Bifrost. Preserve server-originated registry errors. Prove missing credentials, invalid explicit URL, and a transport failure preserve exact code/status in Rust, with the missing-credential case also proven through public Python Cards.

### `FIND-TASK-002-2` — TypeScript connect and describe errors

`connect_cards`, `connect_bifrost`, and table-config description in `sdks/wyrd-sdk-ts/native` collapse expected catalog failures into display-only napi errors; the public wrappers therefore cannot construct `WyrdError`. Existing operation paths already carry closed catalog metadata.

Reuse that existing native-result metadata projection for these three paths and the existing TypeScript `projectedError` conversion. Keep intrinsic napi serialization failures as napi errors. Prove public Cards connect, Bifrost connect, and table description return `WyrdError` with the exact no-credentials metadata, plus one representative invalid-config or transport case.

### `FIND-TASK-002-3` — Bifrost OpenAPI refusals

The seven Bifrost table/query/lifecycle operations in the server route annotations publish incomplete or body-less non-success responses even though handlers return problem JSON. Generated clients cannot type common authentication, authorization, validation, conflict, not-found, or availability refusals.

Reuse the existing Card-route `WyrdProblem` annotation pattern across all seven operations. Publish applicable 400/401/403 responses, register/lifecycle conflicts, describe/status/cancel not-found responses, availability responses, and a typed default for remaining pre-stream failures. Keep query terminal frames as the in-body terminal contract. Extend the OpenAPI test across all seven operations and regenerate contracts.

### `FIND-TASK-002-4` — broken Python example embeds

Five embeds in the two agent/workflow how-to pages still point to deleted `python/py-wyrd/examples` paths. Their shared loader also excludes the new `sdks` root, producing deterministic prerender 500s.

Teach the existing `CodeFromFile` resolver about the `sdks` root and retarget only those five embeds to the moved examples. Do not copy examples or introduce another loader. Prove both routes build without prerender errors and contain their embedded Python examples.

### `FIND-TASK-002-5` — missing `wyrd.errors`

The relocation deleted the authority-defined `wyrd.errors` module together with obsolete aliases. Root-level `wyrd.WyrdError` still exists, but `from wyrd.errors import WyrdError` fails.

Restore a single Python module that re-exports only the existing extension `WyrdError`. Do not restore `Cfg*` aliases, subclasses, metadata logic, or hand-edited generated stubs. Prove module and root imports are the identical class.

### `FIND-TASK-002-6` — unconditional PyO3 boundary

The relocated Python SDK manifest makes PyO3 and retained owner-crate Python features normal dependencies, and the extension root is ungated. Selecting the crate therefore always selects the foreign-runtime cone, contrary to the prescribed optional boundary.

Add the one repository-prescribed `python` feature, make PyO3 and owner Python activations conditional on it, gate extension items, make testing include the Python boundary plus the test harness, and explicitly enable the feature in Python build/wheel lanes. Do not add a feature hierarchy. Prove PyO3 absent under defaults and present with `python`, then run the owning scope, wheel, build, lint, and unit checks.

pyo3-related code should never be included or activated unless the `python` feature is explicitly enabled. This needs to be proven

### `FIND-TASK-002-7` — missing rustdoc

Added or materially relocated items in the Python SDK state/extension modules and the shared Cards handle lack the intent, invariant, side-effect, and fallibility documentation mandated for all changed Rust items. Representative sites are enumerated in `findings-validation.md`; the violation is not limited to those examples.

Document every undocumented added or materially relocated item in the touched Rust modules, including required `# Errors`, panic, and cancellation/partial-progress behavior where applicable. This is documentation-only remediation: do not extract helpers or add a new permanent check. Prove with formatting, lints, docs, and direct review of the added/relocated item diff.

### `FIND-TASK-002-8` — qualified signature paths

Materially relocated signatures in shared state and the Python/TypeScript boundary modules use qualified crate paths despite the mandatory module-top import rule. Exact sites are listed in `findings-validation.md`.

Use the existing module-top import blocks and bare type names at those sites, introducing aliases only for real collisions. Prove no cited signature violation remains, then run Rust formatting and lints.

### `FIND-TASK-002-9` — sibling query lifecycle owner

`QueryClient`, `RawQueryStream`, and public `query_client()` escape hatches remain nameable, and CLI, Python, and TypeScript use them because `Bifrost` lacks the complete raw-query and lifecycle capability set. This violates the explicit single-facade outcome even though the internal query engine is not independently constructible.

Expose the approved raw query, bounded collection, running, status, and cancel capabilities through inherent async `Bifrost` operations and matching blocking operations where applicable. Route production and support callers through `Bifrost`; make the query-client accessor internal and remove public exports of `QueryClient` and `RawQueryStream`. Retain the focused internal engine and public facade-returned `QueryResultStream`. Prove the full capability set is discoverable through `Bifrost`, the sibling types are not public, and existing CLI/Python/TypeScript lifecycle journeys pass.

### `FIND-TASK-002-10` — insert/shutdown race

Producer lookup/creation checks closure before locking the producer map, while shutdown closes and snapshots separately. A racing insert can pass the check and enqueue into a producer created after the shutdown snapshot, allowing both calls to succeed with undrained buffered data.

Use the existing producer-map synchronization boundary so closure and producer admission/creation cannot cross the shutdown close/snapshot transition. Preserve producer-local rejection for enqueue racing its own drain; add no second registry or coordination abstraction. Prove deterministically that a racing insert is either refused or durably acknowledged before shutdown returns and no producer/dynamic ownership remains.

### `FIND-TASK-002-11` — failed terminal uniqueness

Successful/degraded terminals require clean EOF, but a failed terminal settles and returns immediately. A duplicate terminal or other post-failure frame is never observed, so exactly-one-terminal is not enforced.

Reuse the existing deadline-bounded clean-EOF validation for failed terminals before retention and settlement. Clean EOF returns the catalog failure; a post-terminal frame or broken/late EOF uses the existing broken-stream settlement. Prove duplicate-terminal and other post-failure-frame cases cannot retain a terminal or settle prematurely, while a clean failed terminal remains unchanged.

### `FIND-TASK-002-12` — eager multi-batch decode

One arbitrary HTTP body item can contain many frames, and the current pending queue eagerly turns all batch frames into owned `RecordBatch` values before yielding one. Transport coalescing therefore bypasses the declared one-batch ownership/backpressure unit.

Keep queued frames encoded and perform stateful conversion/Arrow decoding only as the next frame is delivered, reusing the current frame and query decoders. Do not add a configurable queue, dependency, or transport rejection policy. Prove a single body item containing schema, many batches, and terminal preserves order/accounting/terminal semantics while pending decoded-batch ownership never exceeds one.

### `FIND-TASK-002-13` — deadline range mismatch

The shared Rust/HTTP/Python request accepts `u64` values above `u32::MAX`, OpenAPI publishes an int64 minimum of zero without a maximum, and TypeScript/MCP accept `u32`. First-class surfaces therefore disagree on valid values and structured failure behavior.

Retain a wide signed shared input capable of representing ordinary underflow/overflow requests, but centralize synchronous validation of `1..=u32::MAX` and publish those bounds in generated schema. Python delegates signed values to shared validation; TypeScript accepts a JavaScript number and projects non-finite, fractional, and out-of-range cases through its existing structured startup result before lossless conversion; MCP remains lossless at `u32`. Update wire conversions without changing the valid range. Prove 0, 1, `u32::MAX`, and `u32::MAX + 1` in shared/schema tests and equivalent catalog errors through Python and TypeScript.

## Constraints and preserved behavior

- Keep `crates/shared/wyrd-client` as the sole SDK-facing implementation and keep durable behavior server-owned.
- Preserve Oracle credential, TLS, transport, queue, bounded Arrow, settlement, backpressure, drain, and catalog error behavior except where a confirmed defect is corrected above.
- Preserve Cards composite registration/loading, `WyrdState`, Eval/Drift, CLI, MCP, and all existing valid Bifrost query/write/lifecycle behavior.
- Keep one derive-backed public error catalog and one runtime `WyrdError` per language projection.
- Preserve tenant isolation, authorization ordering, audit behavior, secret handling, input validation, and failure-closed behavior.
- Keep PyO3 out of `wyrd-spec`; only the Python SDK may activate retained owner-crate Python features.
- Do not weaken, ignore, allowlist around, or delete a gate or test.
- Do not hand-edit generated OpenAPI, stubs, declarations, or schema artifacts.

## Non-goals

- No new client abstraction, transport, queue, decoder, docs loader, exception hierarchy, feature hierarchy, or dependency.
- No movement of durable logic into an SDK or foreign runtime.
- No broad rewrite of unchanged Python wrappers or unrelated documentation.
- No restoration of typed observation reads, legacy audit verification, bootstrap, compatibility aliases, or live UI behavior.
- No implementation, merge, push, release, or deployment outside this remediation task.
- Do not run the Bifrost aggregate lane; retain the original TASK-002 verification boundary.

## Acceptance criteria

| Criterion | Findings closed |
|---|---|
| Shared Cards/Bifrost client failures retain their exact catalog identity through Rust and Python. | `FIND-TASK-002-1` |
| TypeScript Cards/Bifrost connect and table-description failures are public `WyrdError` values with complete metadata. | `FIND-TASK-002-2` |
| All seven Bifrost OpenAPI operations publish typed common and route-specific refusal bodies, and generated artifacts match. | `FIND-TASK-002-3` |
| Both affected docs routes render their relocated Python examples without prerender failure. | `FIND-TASK-002-4` |
| `wyrd.errors.WyrdError` exists, is identical to `wyrd.WyrdError`, and no obsolete alias returns. | `FIND-TASK-002-5` |
| Default Python-SDK Rust dependencies exclude PyO3; the explicit Python feature and owning lanes include it. | `FIND-TASK-002-6` |
| Every added/materially relocated Rust item in scope meets documentation rules. | `FIND-TASK-002-7` |
| Every cited relocated signature uses module-top imports and bare type names. | `FIND-TASK-002-8` |
| Public query and lifecycle workflows are discoverable only through `Bifrost`; all production callers use it. | `FIND-TASK-002-9` |
| Insert racing shutdown cannot be accepted outside the drain set. | `FIND-TASK-002-10` |
| Failed terminals require clean EOF and all post-terminal frames are rejected with exactly-once settlement. | `FIND-TASK-002-11` |
| HTTP chunk coalescing cannot cause more than one decoded pending Arrow batch. | `FIND-TASK-002-12` |
| Rust, HTTP/OpenAPI, gRPC, Python, TypeScript, and MCP agree on `1..=u32::MAX` deadline semantics and catalog failures. | `FIND-TASK-002-13` |

## Required proof

Run and record the exact focused commands for every new or changed test named by the closure proofs above. Rust commands must use exact `mise exec -- cargo nextest run --locked` package/target/test expressions, with the repository Postgres wrapper when required; Python and TypeScript commands must name the exact repository-native path and selector.

Also run the narrowest existing lanes covering each touched surface:

- `mise run fmt`
- `mise run lints`
- `mise run py:format`
- `mise run py:lints`
- `mise run py:test:unit`
- `mise run py:typecheck`
- the affected Python Cards/Bifrost integration lanes
- `mise run ts:build`
- `mise run ts:typecheck`
- `mise run ts:test:unit`
- `mise run ts:test:integration`
- `mise run ts:napi:check`
- the nearest shared-client/Card/Bifrost focused and crate lanes required by the touched Rust modules
- `mise run codegen:check`
- `mise run docs:check`
- `mise run check:client-tier`
- `mise run check:pyo3-scope`
- `mise run check:py-wheel-no-testing`
- `git diff --check`

Re-run the original TASK-002 Card, CLI, `WyrdState`, MCP discovery, SDK dependency/feature, and affected language journey evidence so preserved behavior is directly established. Do not substitute aggregate success for any specifically named focused proof, and do not run the prohibited Bifrost aggregate.

## Implementation evidence

Candidate: `dfdc60699`, `b335fd7df`, `6c603d141`, `f61cf03ad`, `06783f749`, `da0c13a56`, `96b531c08`, `2d2e818cd`, `4d9d74b34` on `change/surfaces-oracle-integration`.

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| FIND-1 shared client catalog identity | `wyrd-client/src/error.rs` single `WyrdClientError`→`WyrdError` projection reused by `cards/error.rs`, `bifrost/query.rs`, `bifrost/scope.rs`; `python/wyrd` Cards raise it | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=cards::error::tests::cards_client_failures_keep_client_catalog_identity)'`; `cd sdks/wyrd-sdk-python && uv run python -m pytest "tests/unit/cards/test_registry_surface.py::test_cards_without_a_credential_raise_the_client_catalog_error"` | PASS |
| FIND-2 TS connect/describe `WyrdError` | `sdks/wyrd-sdk-ts/native/src/{cards,lib}.rs` structured connection results; `wyrd/src/index.ts` `projectedError`; regenerated `index.d.ts`/`index.d.cts` | `cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/unit/connect-errors.test.ts` (4 passed: Cards/Bifrost connect, describe no-credentials, describe transport) | PASS |
| FIND-3 typed Bifrost OpenAPI refusals | `WyrdProblem` responses on the seven Bifrost route annotations; regenerated `openapi.yaml` and schemas | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::openapi::tests::bifrost_operations_publish_typed_problem_refusals)'`; `mise run codegen:check` | PASS |
| FIND-4 docs Python embeds | `docs/src/lib/components/CodeFromFile.svelte` `sdks` root; five embeds retargeted in `how-to/build-a-workflow.svx`, `how-to/build-an-agent.svx` | `mise run docs:check` (build/prerender, links, a11y) | PASS |
| FIND-5 `wyrd.errors` | `sdks/wyrd-sdk-python/python/wyrd/errors.py` re-exports the extension `WyrdError` only | `cd sdks/wyrd-sdk-python && uv run python -m pytest "tests/test_error_contract.py::test_errors_module_projects_the_root_error_class"` | PASS |
| FIND-6 PyO3 only under `python` | `sdks/wyrd-sdk-python/Cargo.toml` optional `pyo3` and owner `/python` activations under `python`; `testing = ["python", "dep:wyrd-testing"]`; `src/lib.rs` gated; maturin `features = ["python", …]`; `scripts/checks/pyo3-scope.sh` default-tree assertion | `! mise exec -- cargo tree --locked -p wyrd-sdk-python -e normal -i pyo3` (no match under defaults); `mise exec -- cargo tree --locked -p wyrd-sdk-python -e normal --features python -i pyo3` (pyo3 v0.28.3 present); `mise exec -- cargo check --locked -p wyrd-sdk-python`; `mise run check:pyo3-scope`; `mise run check:py-wheel-no-testing`; `mise run py:setup` | PASS |
| FIND-7 rustdoc | wyrd-client cards/hydrate/saga/state/storage/upload items; Python `state/{mod,registry}.rs` items with `# Errors`/`# Panics` | `missing_docs_in_private_items` intersected with lines added since `861f8d86c`: 0 hits; `mise run lints`; `mise run docs:check` | PASS |
| FIND-8 bare imported names | module-top imports in `wyrd-client/src/state.rs`, Python `state/mod.rs`, `bifrost/mod.rs` (`NativeBifrost` alias for the pyclass collision), TS `native/src/lib.rs` | `mise run fmt`; `mise run lints`; cited sites inspected | PASS |
| FIND-9 `Bifrost` sole query lifecycle owner | `bifrost/facade.rs`/`blocking.rs` `query`, `collect_bounded`, `running`, `status`, `cancel`; `QueryClient`/`RawQueryStream` exports and `query_client()` removed; CLI, Python, TS, wyrd-testing callers routed through `Bifrost` | `mise exec -- cargo test --locked -p wyrd-client --doc bifrost` (2 `compile_fail` doctests); `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::sdk::bifrost_owns_the_complete_query_lifecycle)'`; CLI journey below | PASS |
| FIND-10 insert/shutdown race | `bifrost/handle.rs` closure checked under the producer-pool lock; shutdown closes and snapshots under the same lock | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::handle::tests::shutdown_and_producer_admission_share_the_pool_lock)'` | PASS |
| FIND-11 failed terminal clean EOF | `bifrost/query.rs` failed terminals settle through `finish_at_clean_eof` | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::failed_terminal_requires_clean_eof)'` | PASS |
| FIND-12 lazy frame decode | `bifrost/query.rs` pending queue holds encoded frames; `decode_frame` converts one per delivery | `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::coalesced_chunk_decodes_one_batch_per_delivery)'` | PASS |
| FIND-13 deadline `1..=u32::MAX` | `wyrd-spec/src/vala/api.rs` `Option<i64>` with validation and schema bounds; tonic/forwarding/MCP conversions; Python `i64`; TS `f64` projected through structured startup errors | `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::query_terminal_tests::query_request_deadline_range_is_closed_and_published)'`; `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/mcp::bifrost::tests::/)'` (3 passed); `mise run ts:test:integration` including `tests/integration/oracle-query.test.ts -t "rejects out-of-range deadlines with the shared validation error"`; `mise run py:test:integration` including `tests/integration/test_bifrost_query.py::test_bifrost_query_out_of_range_deadline_is_the_shared_validation_error` | PASS |
| Preserved CLI query journeys | `wyrd-cli/tests/cli.rs` now compiles `query_server_journey.rs` (orphaned by `autotests = false` since `f6512769f`) | `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=query_server_journey::query_command_reads_seeded_table) \| test(=query_server_journey::query_command_denial_preserves_problem_without_read_decision)"'` (2 passed); `mise exec -- cargo clippy --locked -p wyrd-cli --all-targets --all-features -- -D warnings` | PASS |
| Preserved TASK-002 Card/CLI/WyrdState/MCP evidence | unchanged owners | `mise run test:cards:unit`; `mise run test:cards:integration`; `mise run test:cli:journey`; `mise run test:wyrdstate:journey`; `mise run py:test:cards:integration` (13 passed); `mise run py:test:wyrdstate:integration` (7 passed); `WYRD_CLI_E2E=1 WYRD_REG_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo test --locked -p wyrd-cli --test cli -- card_lifecycle::pg_tests::card_lifecycle_cli_journey --exact'`; `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp --run-ignored all -E "test(=discovery::pg_tests::agent_discovers_only_authorized_tables_and_layout)"'` | PASS |
| Repository lanes | — | `mise run fmt`; `mise run lints`; `mise run py:format`; `mise run py:lints`; `mise run py:test:unit`; `mise run py:typecheck`; `mise run ts:build`; `mise run ts:typecheck`; `mise run ts:test:unit`; `mise run ts:test:integration` (17 passed); `mise run ts:napi:check`; `mise run codegen:check`; `mise run docs:check`; `mise run check:client-tier`; `mise run check:pyo3-scope`; `mise run check:py-wheel-no-testing`; `git diff --check` | PASS |

Non-goals held: no new abstraction, transport, queue, decoder, loader, exception or feature hierarchy, or dependency; generated artifacts were regenerated, not hand-edited; the Bifrost aggregate lane was not run.

Material limits:

- `mise run py:test:integration` failed once (`test_state_journey.py::test_service_bundle_hydrates_complete_python_runtime_offline`: server `mark upload completed affected 0 rows` in `wyrd-storage` multipart completion) and passed 55/55 on immediate rerun; the same test passed in `py:test:wyrdstate:integration`. That storage path is untouched by this remediation; the intermittent conflict is unresolved.
- `sdks/wyrd-sdk-python/Cargo.toml` gained an uncommitted `[profile.release]` block from outside this work during verification; it is excluded from the candidate.
