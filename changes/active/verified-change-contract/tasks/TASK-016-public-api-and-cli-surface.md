---
id: TASK-016
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 64
requirements: [REQ-188, REQ-189, REQ-190, REQ-191, REQ-193, REQ-194, REQ-195, REQ-196, REQ-197, REQ-198, REQ-199, REQ-200, REQ-201, REQ-202, REQ-203, REQ-205, REQ-206, AC-045, AC-046, AC-047, AC-049, AC-050, AC-051, AC-052, AC-053, AC-054, AC-055, AC-056, AC-058]
depends_on: []
---

# Ship the verification-era public API and in-process CLI

## Outcome and Value

The server and first-class Rust, Python, and TypeScript SDKs expose the revision-64 public workflow without test workarounds: a Run view judges through `observe.verify`, Cards read back as typed envelopes, aliases remain aliases, CLI commands are callable as functions, callers can obtain their current access token, Bifrost queries carry bind parameters, built-in tables exist from tenant creation onward, stock OTel exporters authenticate with API keys, and Python authoring is typed with Rust and TypeScript parity where those SDKs expose the same capability. The removed `Verification` SDK handle has no compatibility surface, while its server HTTP and MCP operations remain (REQ-188..191, REQ-193..203, REQ-205..206; AC-045..047, AC-049..056, AC-058).

Python exception hierarchy and catalog-completeness behavior remain owned by `changes/active/py-error-refactor/spec.md`; this task only consumes that contract and changes the exports required by REQ-189.

## Owners, Scope, Consumers, and Prohibited Changes

- `wyrd-spec` owns changed wire contracts, typed `Judgment`/Card/status projections, query parameters, constructors, schemas, and catalogued local refusals. It remains IO-free, async-free, and PyO3-free.
- `wyrd-client` owns the shared Run/Observe, Cards, Bifrost query, token, artifact-manifest, and transport behavior consumed by all SDKs. Language SDKs remain thin projections.
- `wyrd-cli` owns one command implementation per CLI operation and typed JSON-equivalent results. The executable and in-process functions must call that same implementation; no SDK reimplements command behavior.
- `wyrd-server` owns Operator registration/delivery, tenant provisioning/startup reconciliation, OTLP API-key authentication, and query-wire forwarding. Existing HTTP/MCP binding and run operations remain server-owned.
- `wyrd-testing` owns exactly the three public test controls fixed by REQ-195. Test-suite usage and cleanup belong to TASK-017.
- Existing Rust authoring owners and `sdks/wyrd-sdk-python` own Python Card/runtime authoring; TypeScript and Rust gain parity only where they already expose the same Card kind.
- Prohibited: changes to Bifrost schemas, column types, Iceberg metadata, storage format, Scribe, Forge, Oracle execution/planning internals, or the SQL engine; an SDK verification-history API; an SDK key/credential mutation API separate from CLI functions; an OTLP helper; a principal-id accessor; a blocking/retrying observation emit; a `Verification` alias or compatibility module; re-planning Python error behavior.
- REQ-200 is limited here to the typed public query request, shared clients, SDK projections, and server query forwarding seam. If Oracle needs execution changes beyond consuming the forwarded bind values, that work stays in `SPEC-bifrost-variant`.
- REQ-201 calls the existing `BifrostCatalog::ensure_builtin` and canonical `builtin_tables()` inventory from tenant provisioning and server startup. It must not alter either implementation or any table definition.
- REQ-206 is a negative public-contract obligation: retain ordinary parameterized Bifrost SQL as the only SDK history read and add no history handle.

## Approach

1. Put shared wire values and stable errors in their current owners, then project them through `wyrd-client` and the three SDK boundaries; regenerate rather than hand-edit generated schemas, Python stubs, or N-API declarations.
2. Move direct judgment onto the existing dependency-owning Run/Observe owners, resolving a bound Verifier from the hydrated graph before the existing no-retry verification HTTP call.
3. Remove the SDK-only Verification handle and exports, rename Run view identity to `alias`, and preserve server HTTP/MCP binding/run contracts.
4. Reuse current Card registration, CLI command, auth-token, query, catalog, Operator connection, and OTLP authentication owners rather than adding parallel services or helpers.
5. Complete typed Card/runtime authoring at the foreign-runtime boundary while keeping validation and durable behavior in Rust-native owners.
6. Update the active architecture authorities for `observe.verify`, eager built-in provisioning, query parameters, and API-key OTLP authentication without importing `SPEC-bifrost-variant` storage decisions.

## Ordered Implementation Scenarios

### Scenario 1 — A Run view verifies a bound Verifier and returns a typed Judgment

**Behavior.** Rust `run.observe().verify`, Python and TypeScript `run.observe.verify`, and Python's positional `state.run("alias")` resolve the named bound Verifier locally, accept the Verifier-kind-specific input, call the existing direct verification route once, and return the typed Eval or Drift `Judgment`. Failed verdicts return normally. Unknown bindings and wrong input shape fail locally with the exact REQ-188 codes before transport; baseline-not-ready preserves the server error (REQ-188, AC-045, AC-049).

**RED.** Add the shared client regression `observe::tests::verify_resolves_bound_verifiers_and_refuses_invalid_inputs_locally`; run `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::verify_resolves_bound_verifiers_and_refuses_invalid_inputs_locally)'`. It initially fails because Observe has no verification operation or graph binding index.

**GREEN.** Add the minimum cohesive Run/Observe behavior and foreign-runtime projections, reusing the current direct-execution transport and hydrated graph. Preserve judge-only behavior: no observation, run, dispatch, Bifrost write, or Bifrost startup dependency.

**REFACTOR.** Delete the direct-execution duplication left in SDK Verification wrappers and keep one Rust-native input-to-request conversion per Verifier kind.

### Scenario 2 — Removed verification surfaces stay removed and Run views expose aliases

**Behavior.** No SDK exports `Verification`; Python has no importable `wyrd.verification`; Rust exports `WyrdError` at `wyrd_sdk::WyrdError`; Run views expose the opening alias and no `card_ref`/`cardRef`, while `state.card_ref(alias)` remains typed (REQ-189, REQ-194, AC-046, AC-050, AC-052).

**RED.** Replace the Rust SDK root projection check with `tests::sdk_root_exposes_the_supported_surface`; run `mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --lib -E 'test(=tests::sdk_root_exposes_the_supported_surface)'`. It initially fails while the Verification handle remains and `WyrdError` is not a root export.

**GREEN.** Remove SDK handle modules/exports and generated declarations, retain only the private shared transport needed by Observe, and carry the selected alias beside the exact subject reference inside the Run view.

**REFACTOR.** Remove dead verification SDK types and imports rather than retaining name bans or compatibility shims. Do not alter the server routes or the Python error model owned by `py-error-refactor`.

### Scenario 3 — Operator paths and artifact manifests are author-friendly

**Behavior.** An HTTP Operator with a named HTTP connection accepts a `/`-prefixed path template and resolves the stored origin on every attempt; the same path without a connection is refused at registration. Artifact manifest entries may omit digest and size, which the client computes before registration; supplied values are verified and a mismatch returns `RegistryManifestHashMismatch` (REQ-190, REQ-191, AC-047).

**RED.** Add `card::operator::tests::path_only_url_requires_a_named_http_connection`; run `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=card::operator::tests::path_only_url_requires_a_named_http_connection)'`. Add `cards::saga::hash_artifacts::tests::missing_metadata_is_computed_and_declared_mismatch_is_refused`; run `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=cards::saga::hash_artifacts::tests::missing_metadata_is_computed_and_declared_mismatch_is_refused)'`.

**GREEN.** Extend the existing Operator validation/delivery and artifact hash pass. Keep absolute-URL origin matching and SSRF screening unchanged, and finish all local artifact reads before the registration call.

**REFACTOR.** Keep origin composition and manifest hashing in their existing owners; add no alternate loader or connection resolver.

### Scenario 4 — Cards and authoring values are typed across SDKs

**Behavior.** `cards.get(ref)` returns typed envelope/spec/status values for exactly Data, Model, Prompt, Agent, Verifier, Service, Trigger, and Operator in all SDKs. Operator-connection requests and `CardRef` have typed constructors; TypeScript `TableConfig.fromJsonSchema` takes an options object and no public constructor takes a native binding type; local refusals carry catalog codes; trusted artifact hashes come from the registered Card (REQ-193, REQ-202, AC-049, AC-056).

**RED.** Add `cards::handle::tests::supported_kinds_project_typed_specs_and_status`; run `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=cards::handle::tests::supported_kinds_project_typed_specs_and_status)'`. Extend schema-source tests rather than generated outputs; run the exact owning check `mise run codegen:check` to expose Python/TypeScript drift.

**GREEN.** Generate the Python and TypeScript unions from `wyrd-spec`, project the existing Rust Card envelope, and add only the typed boundary constructors/options required by REQ-202. Do not expose typed specs for the seven deferred kinds.

**REFACTOR.** Remove raw-object and native-binding public doors superseded by typed projections; retain no duplicate hand-maintained kind union.

### Scenario 5 — Python authoring is typed end to end with parity

**Behavior.** Python supplies `DataCard.from_path`, `ModelCard.from_path`, typed splits, target columns, interface readback, Agent Card conversion, callback context, prompt response schema, config values, signature dimensions, query terminals, and a documented canned-response offline mock. Rust and TypeScript expose the same capability wherever their existing SDK surface has the same Card kind (REQ-203, AC-056).

**RED.** Add source-owner regression tests in the existing Python unit files and run them by exact path after the repository-owned build: `mise run py:setup && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q tests/unit/cards/data/test_datacard_save_load.py tests/unit/cards/model/test_modelcard_save_load.py tests/unit/runtime/agent tests/unit/runtime/test_callbacks.py tests/unit/config tests/unit/cards/prompt`. The failures must identify missing public typed operations, not test fixture setup.

**GREEN.** Complete the Rust-native owner APIs and thin PyO3/package projections, then add Rust/TypeScript parity only for already-exposed corresponding kinds. The mock remains offline, public, deterministic, and caller-configured.

**REFACTOR.** Reuse `PromptCard.from_path` and existing typed domain values; do not add Python-owned validation, a second runtime, or wrapper types that only mirror one Rust type.

### Scenario 6 — The CLI is one implementation callable from every SDK

**Behavior.** `plan`, `apply`, `get`, and `load`, plus key issuance and provider-credential commands used by journeys, are typed in-process functions in Rust, Python, and TypeScript. They return the same typed value the executable prints with JSON format and raise/return `WyrdError`, while the Python and TypeScript packages still install `wyrd` (REQ-196, REQ-199, AC-051, AC-053).

**RED.** Add `card::tests::in_process_commands_return_the_json_result_without_exiting`; run `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=card::tests::in_process_commands_return_the_json_result_without_exiting)'`. It initially fails because dispatch writes output and returns `ExitCode` rather than a typed result.

**GREEN.** Separate command execution from terminal rendering at the existing CLI owner, then project those functions through the optional Rust SDK `cli` feature and foreign-runtime bindings. Secret-bearing typed arguments must preserve existing redaction and secret-input rules.

**REFACTOR.** Make the executable a thin renderer over the same command functions; delete duplicated SDK command logic and subprocess-only paths.

### Scenario 7 — Clients expose current tokens and keep observation emits non-blocking

**Behavior.** Every SDK client returns a current bearer through `access_token()` for third-party clients; no principal-id accessor is added. Existing observation emits continue to refuse a full queue immediately with `WYRD_CLIENT_429_QUEUE_FULL`, without blocking or retrying (REQ-197, REQ-198, AC-053).

**RED.** Add `client::tests::access_token_uses_the_shared_refreshing_auth_path`; run `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=client::tests::access_token_uses_the_shared_refreshing_auth_path)'`. Keep the existing queue-full regression selected by `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(/^observe::tests::/)'` as verification-only proof for REQ-197 rather than manufacturing production churn.

**GREEN.** Expose the existing auth middleware's current bearer through the assembled client and project it. Preserve secrecy/redacted debug behavior and the current observation admission path.

**REFACTOR.** Keep token refresh in `AuthMiddleware`; no SDK cache, exporter helper, or credential copy gains independent lifecycle logic.

### Scenario 8 — Query parameters cross the public wire without interpolation

**Behavior.** Rust, Python, and TypeScript `sql(query, params)` preserve ordered typed bind values in the public request, and the server forwards them to the existing query seam. A value containing SQL text remains data. Verification history remains ordinary parameterized SQL with no SDK history API (REQ-200, REQ-206, AC-054).

**RED.** Add `vala::api::tests::query_bind_values_round_trip_in_order`; run `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::tests::query_bind_values_round_trip_in_order)'`. Add `bifrost::query::tests::sql_forwards_bind_values_without_interpolation`; run `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::sql_forwards_bind_values_without_interpolation)'`.

**GREEN.** Extend the existing query request and SDK projections, validate supported scalar values at the contract edge, and forward them unchanged. Consume the Bifrost change's bind-execution seam when it merges; do not edit Oracle or DataFusion here.

**REFACTOR.** Keep `sql` and streaming on the same request type; remove formatted SQL from examples and public client code touched by this task.

### Scenario 9 — Built-in tables exist with every tenant and after restart

**Behavior.** Tenant creation ensures every current canonical built-in before the tenant becomes active. Server startup lists active tenants and idempotently ensures the same inventory, backfilling additions. An unwritten built-in is therefore queryable and empty; first-write and describe paths no longer own creation (REQ-201, AC-055).

**RED.** Add `platform_admin_e2e::new_and_existing_tenants_receive_every_builtin`; run `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=platform_admin_e2e::new_and_existing_tenants_receive_every_builtin)'"`.

**GREEN.** Compose tenant provisioning and boot reconciliation with the existing catalog and `builtin_tables()` list. Treat failure as provisioning/startup failure rather than exposing a partially ready tenant or ready server.

**REFACTOR.** Share the narrow server-owned ensure-all workflow between provisioning and startup. Do not move catalog or table-definition ownership into the server.

### Scenario 10 — Stock OTLP exporters authenticate directly with API keys

**Behavior.** HTTP and gRPC OTLP endpoints accept `x-wyrd-api-key`, validate it on every request through the existing API-key exchange/verification authority, and keep accepting it beyond any prior access-token lifetime. Existing access-token authentication remains valid; no SDK exporter helper is added (REQ-205, AC-058).

**RED.** Add `trace_export::stock_exporter_authenticates_with_api_key_after_token_lifetime`; run `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E 'test(=stock_exporter_authenticates_with_api_key_after_token_lifetime)'"`.

**GREEN.** Add an OTLP-specific authentication entrance that accepts either existing bearer auth or the API-key header and produces the same authenticated principal before decoding/admission. Preserve per-request validation, authorization, audit, limits, and tenant scope.

**REFACTOR.** Share API-key verification between HTTP and gRPC adapters without widening authentication on non-OTLP routes.

### Scenario 11 — The test harness exposes only the three sanctioned controls

**Behavior.** Rust, Python, and TypeScript `WyrdTestServer` publicly document `flush_bifrost`, `wait_for_baseline(verifier, timeout)`, and `make_binding_due(...)`, backed by production paths; other audit-discovered public hooks are not part of the SDK test contract (REQ-195).

**RED.** Add `server::tests::public_controls_are_the_three_sanctioned_operations`; run `mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=server::tests::public_controls_are_the_three_sanctioned_operations)'`. Run `mise run codegen:check` to expose Python stub drift.

**GREEN.** Provide the missing baseline wait and align the three language projections and docs. Runtime-only private harness facilities may remain for their owning Rust tiers but are not exported as client test controls.

**REFACTOR.** Remove duplicate public wrappers; keep one production-path implementation behind each control.

## Acceptance Criteria

- AC-045..047 and AC-049..056, AC-058 are implementable solely through the documented public surfaces; TASK-017 supplies the cross-language journey evidence.
- The SDK Verification handle/module is absent, while server HTTP/MCP binding and run routes remain.
- All local refusals introduced here use one exact `WyrdError` catalog code; Python error internals are unchanged except for consuming the separately approved error contract.
- Query parameters are never interpolated into SQL by clients or the Wyrd server seam.
- Built-in provisioning uses only the current catalog inventory and idempotent creation operation.
- `observe.verify` performs no observation, run creation, Operator dispatch, or Bifrost write.
- No prohibited Bifrost internal, storage, schema, or execution file changes.

## Expected Write Set and Consumer Closure

Production and owner proof only; TASK-017 owns all audited client-facing tests and shared fixtures.

- Contracts and generated sources: `crates/wyrd-spec/src/{verification.rs,envelope.rs,vala/api.rs,registry/submission.rs,card/operator.rs,error.rs}`, schema-generator sources, and generator-produced `crates/wyrd-spec/schemas/`, Python `.pyi`, and TypeScript declarations/error-code projections.
- Shared client: `crates/shared/wyrd-client/src/{client.rs,lib.rs,observe/**,state.rs,verification.rs,cards/**,bifrost/**,operator_connections.rs}` and focused inline owner tests. The public Verification module is deleted or reduced to a private transport owner with no SDK export.
- CLI: `crates/wyrd/wyrd-cli/src/**`, its manifest, and executable wiring.
- Server seams: `crates/wyrd/wyrd-server/src/{components/platform/**,components/operators/**,http/**,grpc/otlp.rs,query/**,boot/mod.rs,state.rs}` plus existing server/OTLP focused targets.
- Test harness production surface: `crates/wyrd/wyrd-testing/src/{server.rs,verification.rs,lib.rs}` and Python/TypeScript harness bindings/stubs.
- Rust SDK: `sdks/wyrd-sdk-rust/{Cargo.toml,src/lib.rs}`.
- Python SDK production/package surface: `sdks/wyrd-sdk-python/{Cargo.toml,pyproject.toml,src/**,python/wyrd/**}` and the Rust-native authoring owners it already wraps under `crates/wyrd/wyrd-cards`, `crates/skald/**`, and existing configuration/runtime crates.
- TypeScript SDK production/package surface: `sdks/wyrd-sdk-ts/{native/**,wyrd/src/**,wyrd/package.json}` plus generated declarations.
- Architecture: `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and the narrow REQ-201 correction in `architecture/bifrost-design.md`.
- Manifests and `mise.toml` only where the Rust SDK CLI feature, packaged executables, generated declarations, or focused owner tests require registration.

## Dependency, Merge Order, and Bifrost Conflict Resolution

TASK-016 has no dependency on TASK-017. Merge TASK-016 first; TASK-017 then builds against the exact revision-64 public API and may not preserve an obsolete production surface to ease test migration. If both branches touch a test registration in `mise.toml`, preserve TASK-016's production/build requirements and TASK-017's final test membership in one lane.

`SPEC-bifrost-variant` merges after these tasks. Likely overlap is limited to `architecture/bifrost-design.md`, `crates/wyrd-spec/src/vala/api.rs`, `crates/shared/wyrd-client/src/bifrost/{facade.rs,query.rs,blocking.rs}`, `crates/wyrd/wyrd-server/src/query/{routes.rs,service.rs,scheduled.rs}`, `crates/wyrd/wyrd-server/src/boot/mod.rs`, and generated schemas/declarations. Resolve by retaining this task's public `sql(query, params)` and eager tenant/startup calls while taking the Bifrost branch's internal request consumption, Variant/Iceberg representation, and execution implementation. This task must not edit `crates/vala/vala-bifrost-redux/**`, `crates/vala/vala-sql/**`, Scribe, Forge, Oracle, migrations, table definitions, or storage metadata.

## Verification and Evidence

- Run each exact RED command above during its scenario, then rerun earlier green scenario commands after shared-owner changes.
- Use `mise run codegen:check` after contract/export work; never hand-edit generated outputs.
- Because this task crosses contracts, server, CLI, all SDKs, auth, and shared CI/build surfaces, final verification is `mise run gate` once, plus `git diff --check` and a prohibited-path diff audit. Do not separately rerun gate component lanes as final proof.
- Record that TASK-017 supplies the required client-facing Rust/Python/TypeScript journeys after this API merges, and that integrated AC-054 consumes the later Bifrost bind-execution seam without weakening the public request proof here.

## Material Stop Conditions

- The existing direct-verification route cannot return the fixed `Judgment` without changing its approved server semantics.
- Correct bind execution requires this task to choose or change a Bifrost storage, DataFusion, Oracle, or persistent-data contract rather than forwarding the revision-64 public parameters.
- Eager built-in creation cannot be composed from `builtin_tables()` and `BifrostCatalog::ensure_builtin` without changing table definitions, catalog semantics, or migrations.
- Python error behavior must diverge from the separately approved `py-error-refactor` specification.
- Packaging one `wyrd` executable from the Python or TypeScript SDK requires a new distribution contract not fixed by REQ-196.

## Authority Links

- [Approved spec revision 64](../spec.md): REQ-188..206 except REQ-192/204; AC-045..058 except AC-048/057.
- `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/bifrost-design.md`.
- `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/testing-workflows.md`.
- Python exception authority: `changes/active/py-error-refactor/spec.md`.
