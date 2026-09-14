# Public SDK and Contract Domain Review

## Reviewed boundary

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f66a337698940920dca20b126c1c6c28a6390191`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Scope: the sole `wyrd-client` owner; Rust, Python, and TypeScript projections; Cards, `WyrdState`, and Bifrost capabilities; Python PyO3/error/export/stub behavior; TypeScript N-API/declarations/error-code generation; and HTTP/OpenAPI/protobuf/MCP/CLI contract agreement.

The candidate commit remained `f66a337698940920dca20b126c1c6c28a6390191` throughout this review. Review artifacts created beside this report are outside the immutable committed subject.

## Authority and source coverage

| Boundary | Governing authority | Source and consumer coverage |
|---|---|---|
| Shared client and SDK ownership | `AGENTS.md` §§2–3, 5, 9; `architecture/agent-rules.md`; `architecture/wyrd-design.md` client-language and Bifrost sections; `architecture/wyrd-doctrine.mdx`; spec REQ-016–REQ-021, INV-004–INV-006, INV-013, INV-024 | Workspace/manifests, `crates/shared/wyrd-client`, deleted predecessor crates, `sdks/wyrd-sdk-{rust,python,ts}`, CLI and test consumers. The Rust SDK is a direct re-export; Python and N-API bindings delegate to `wyrd-client`; removed domain client crates are absent. `QueryClient::new` remains crate-private and its public operations are reached through `Bifrost::query_client`, so it is not independently constructible as a sibling owner. |
| Cards and `WyrdState` projections | `AGENTS.md` §§8–9, 11; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{python-api-and-stubs,typescript-guide,testing-workflows}.md`; spec REQ-004–REQ-009, REQ-017–REQ-021, AC-002–AC-004 | `wyrd-client::{cards,state,storage}`, Python SDK state/cards aggregation and stubs, TypeScript `NativeCards`/`NativeWyrdState` and public wrappers, Rust/Python/TypeScript journeys, server Card routes and CLI journeys. |
| Stable errors and generated contracts | `AGENTS.md` §§4, 8–9; `architecture/references/languages/{errors,pyo3-boundaries,python-api-and-stubs,typescript-guide,agent-harness}.md`; spec REQ-022–REQ-025, REQ-059, AC-002 | `wyrd-error-derive`, `wyrd-spec` error catalog/schema generator, shared Python projector, Python runtime exports/stubs/tests, TypeScript native envelopes/runtime `WyrdError`/generated `WyrdErrorCode`, server route annotations, `openapi.yaml`, and protobuf/query conversions. |
| Bifrost query contract and surface parity | `architecture/bifrost-design.md` §§Read audit and terminal contract, Public surface; `architecture/wyrd-design.md` §Bifrost; spec REQ-019–REQ-021, REQ-056–REQ-059 and externally observable behavior | `BifrostQueryRequest`, client validation/query stream, server query routes/service, Oracle validation, Python and TypeScript query boundaries, MCP deadline schema/parser, CLI query caller, OpenAPI and language journeys. |

## Verification limits

- Reviewed the complete base-to-candidate diff and the task-recorded passing results for Rust formatting/lints and Card/CLI/`WyrdState` lanes; Python unit, integration, typing, Cards, and state lanes; TypeScript build/type/unit/integration/N-API lanes; codegen, client-tier, PyO3-scope, wheel, response-mapper, protobuf, examples/docs, Postgres inventory, and focused Rust SDK/CLI/MCP tests.
- This reviewer did not rerun the recorded suites. The task intentionally excludes the Bifrost aggregate lane.
- Existing green codegen proves the generated files match their current annotations; it does not prove that the annotations describe every required failure response or the architecture-defined deadline range.

## Material findings

### SDK-CONTRACT-01 — INCORRECT: the shared query contract permits deadlines above the public `u32` range

- **Violated obligation:** `architecture/bifrost-design.md:451-453` and specification externally observable behavior at `spec.md:660-662` require the identical public range `1..=u32::MAX` across Rust, HTTP, gRPC, Python, TypeScript, and MCP.
- **Location:** `crates/wyrd-spec/src/vala/api.rs:537-553`; projected schema at `openapi.yaml:893-907`; Python boundary at `sdks/wyrd-sdk-python/src/bifrost/mod.rs:422-435`; TypeScript N-API boundary at `sdks/wyrd-sdk-ts/native/src/lib.rs:20-30`.
- **Evidence:** `BifrostQueryRequest.deadline_ms` is `Option<u64>` and `validate` rejects only zero. OpenAPI therefore advertises an `int64` with `minimum: 0` and no maximum. Python accepts the same `u64` input, while TypeScript and MCP use `u32`, producing different accepted domains. Oracle projects any nonzero `u64` into a duration (`vala-bifrost-redux/src/oracle/mod.rs:4072-4077`) until a later time conversion happens to overflow.
- **Observable consequence:** Rust, HTTP, and Python accept values that the TypeScript and MCP surfaces cannot represent, and generated clients are told that zero and values above `u32::MAX` belong to the contract. Invalid Python/Node numeric values may also fail during foreign-runtime extraction rather than as the same catalog-backed validation error. The public contract is not coherent across first-class languages.
- **Required testable correction:** enforce `1..=u32::MAX` in the shared `BifrostQueryRequest` contract and generated schema, and make each foreign boundary pass invalid numeric input through the structured Wyrd validation path rather than relying on unsigned/narrow binding extraction. Add one focused contract check for zero, `u32::MAX`, and `u32::MAX + 1`, plus public Python and TypeScript assertions that the out-of-range case raises their runtime `WyrdError` with the same stable validation code.

### SDK-CONTRACT-02 — MISSING: OpenAPI publishes Bifrost success shapes but omits its structured failure bodies

- **Violated obligation:** specification REQ-059 (`spec.md:385-389`) requires generated OpenAPI authority to include Bifrost table/query/lifecycle operations and structured errors; AC-002 requires one coherent error and HTTP surface. Repository error guidance requires public HTTP errors to use the generated `WyrdProblem` shape.
- **Location:** `crates/wyrd/wyrd-server/src/bifrost/routes.rs:22-76`; generated result at `openapi.yaml:12-89`. The same omission remains on query lifecycle annotations in `crates/wyrd/wyrd-server/src/query/routes.rs:90-199`.
- **Evidence:** table registration and listing declare only `200`; describe declares a body-less `404`; query/lifecycle annotations similarly describe failures without a `WyrdProblem` body. The handlers actually return `WyrdErrorResponse` as `application/problem+json`, and Card route annotations already demonstrate the repository mechanism by attaching `body = WyrdProblem` to each public error response.
- **Observable consequence:** a client generated from the authoritative OpenAPI document cannot discover or type the structured Wyrd failure returned by Bifrost routes, despite Rust, Python, TypeScript, MCP, and CLI consumers relying on that metadata. Green codegen simply reproduces the incomplete source annotations.
- **Required testable correction:** use the existing Card-route annotation pattern to declare `WyrdProblem` bodies for the reachable non-success responses of every public Bifrost table, query, and lifecycle operation, regenerate OpenAPI, and extend the focused OpenAPI test to assert that each Bifrost operation exposes an `application/problem+json` response referencing `WyrdProblem`.

### SDK-CONTRACT-03 — MISSING: the new Rust SDK journey does not exercise its public Card-list capability

- **Violated obligation:** `AGENTS.md` §11 and `architecture/references/languages/testing-workflows.md` require a real client → server → client journey for every user-facing capability; AC-003 requires focused evidence through each affected public language. The candidate establishes `wyrd_sdk` as a new first-class public package and re-exports `Cards::list` through it.
- **Location:** `sdks/wyrd-sdk-rust/tests/cards_state.rs:132-214`.
- **Evidence:** the journey registers, replays, gets, tests a write denial, hydrates, and loads offline state, but never calls `Cards::list`. The TypeScript journey does call `cards.list` (`sdks/wyrd-sdk-ts/wyrd/tests/integration/cards-state.test.ts:134-135`), and lower-level `wyrd-client`/server tests cannot prove the new `wyrd_sdk` package path for this operation.
- **Observable consequence:** the required first-class Rust package has no journey proof that its metadata-list request and response compile and cross the real server boundary, leaving a language-parity gap in the exact surface TASK-002 introduces.
- **Required testable correction:** extend the existing Rust SDK Card/`WyrdState` journey with one `wyrd_sdk::cards::Cards::list` call after registration and assert the returned metadata identifies the registered Card; run the same exact focused journey command already recorded for `cards_state`.

## Overall result

**FAIL**

The ownership convergence, Python error root, generated TypeScript error-code union, Cards/`WyrdState` delegation, and removal of parallel typed-read/client authority are supported by source and recorded verification. The three findings above prevent this public SDK/contracts boundary from satisfying TASK-002 exactly.
