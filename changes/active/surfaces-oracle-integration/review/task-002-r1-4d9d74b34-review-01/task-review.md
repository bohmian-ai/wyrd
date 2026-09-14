# TASK-002 Remediation Implementation Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Review worktree: `/tmp/wyrd-task-002-r1-review-4d9d74b34`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `4d9d74b34803b3f27d55f740a5b40ffbe968b306`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior review: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/`
- Remediation task: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/TASK-002-R1-close-review-findings.md`
- Reviewed range: the complete cumulative `861f8d86c..4d9d74b34` range, not only the remediation commits.
- Candidate identity remained `4d9d74b34803b3f27d55f740a5b40ffbe968b306` and the candidate worktree remained clean before and after review.

## Review findings

### Critical

None.

### Important

None.

### Suggestions

None. Optional improvements, duplicate coverage, and unrelated debt were excluded.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-004–REQ-009, INV-001: preserve composite Card registration/loading, reference and `CardRef` contracts, Eval/Drift semantics, the 16-kind catalog, and Audit Card semantics | The Surfaces Card, loader, storage, hydration, saga, and `WyrdState` implementations are composed under `crates/shared/wyrd-client/src/{cards,state,storage}`; contract owners in `wyrd-spec` retain the approved shapes | Recorded Card unit/integration, CLI, Rust SDK, Python, TypeScript, HTTP, MCP, and `WyrdState` journeys all pass | PASS |
| REQ-016–REQ-018, REQ-040, REQ-041, INV-004, INV-013, INV-024: one shared client implementation and three thin SDK roots | `wyrd-client` owns auth, transport, Cards, storage, state, and Bifrost; `wyrd-sdk-rust` is a direct re-export; Python and TypeScript contain only foreign-runtime projections; retired shared client-owner crates are removed | Recorded `check:client-tier`, language builds/typechecks/journeys, and dependency-tree evidence pass | PASS |
| REQ-019, REQ-061, INV-005: `Bifrost` is the sole public query/write/lifecycle facade | `bifrost/facade.rs` exposes table management, buffered/direct writes, flush/shutdown, SQL, raw query, bounded collection, streaming, running/status/cancel, and description; `QueryClient` and `RawQueryStream` remain behind private `query`; production CLI/Python/TypeScript/support callers use `Bifrost` | Recorded compile-fail doctests and `bifrost_owns_the_complete_query_lifecycle` pass; source search finds no external production `query_client()` caller | PASS |
| REQ-019A: one explicit shared credential field | `ClientConfig::credential` is the shared configuration field; Card/Bifrost/Python/TypeScript constructors route through the shared resolution chain; no per-client `api_key` alias was restored | Recorded shared-client, Python, TypeScript, and configuration tests pass | PASS |
| REQ-020: preserve public Python authoring, conversion, iteration, lifecycle, and SDK modules | The relocated package remains under `sdks/wyrd-sdk-python`; public Cards/data/model/prompt/state/Bifrost modules delegate to Rust owners; sync/async, typed-row, PyArrow, Polars, pandas, and IPC surfaces remain | Recorded Python unit, typecheck, Cards/state, and Bifrost integration lanes pass | PASS |
| REQ-021: preserve `@wyrd/sdk` declarations, Arrow/model/typed-result ergonomics, lifecycle, and structured errors | `sdks/wyrd-sdk-ts` wraps `wyrd_client::Bifrost`, Cards, and state through N-API; no independent HTTP/gRPC implementation exists | Recorded TypeScript build, typecheck, unit, 17-test integration, and N-API declaration checks pass | PASS |
| REQ-056: schema-once, batch, Arrow EOS, identity, row-count, exactly-one-terminal, broken/drop settlement, and bounded Arrow ownership | `RawQueryStream` retains encoded wire frames and decodes one frame at delivery; `QueryResultStream::finish_at_clean_eof` validates every terminal including failure; settlement remains deadline-bounded and exactly once | Independently passed `failed_terminal_requires_clean_eof` and `coalesced_chunk_decodes_one_batch_per_delivery`; recorded stream and language journeys pass | PASS |
| REQ-057, AC-019: `/mcp` exposes exactly the three approved bounded Bifrost tools | MCP catalog remains `bifrost.list_tables`, `bifrost.describe_table`, and `bifrost.query`; bounded collection uses the shared facade | Recorded exact MCP discovery and deadline boundary tests pass | PASS |
| REQ-058, AC-019: CLI query behavior remains intact through `Bifrost` | CLI uses `Bifrost::query_only`; `4d9d74b34` compiles `query_server_journey.rs` into the explicit `cli` test target despite `autotests = false` | Recorded seeded-query and denial/audit journeys plus `test:cli:journey` pass | PASS |
| REQ-059, AC-002: complete generated table/query/lifecycle/error authority | All seven Bifrost OpenAPI operations publish typed `WyrdProblem` common/default and route-specific refusals; table/query/lifecycle routes and deadline schema are generated from source | Independently passed `http::openapi::tests::bifrost_operations_publish_typed_problem_refusals`; recorded `codegen:check` and proto drift checks pass | PASS |
| REQ-022–REQ-025: one derive-backed Rust/Python/TypeScript error authority | Shared `WyrdClientError -> WyrdError` projection is reused by Cards and Bifrost; Python exposes one eight-field `WyrdError` at root and `wyrd.errors`; TypeScript native outcomes construct runtime `WyrdError` and use the generated code union | Independently passed `cards_client_failures_keep_client_catalog_identity`; recorded Python identity/error and TypeScript connect/describe/error tests pass | PASS |
| REQ-060, INV-006: preserve Oracle client/queue ownership, settlement, backpressure, drain, credentials, TLS, transport, and error reconstruction without a heavy client cone | Oracle `wyrd-client`/`wyrd-queue` behavior remains the shared owner; client-tier normal dependencies exclude SQL/cloud/DataFusion/Delta/Iceberg/server crates; test-only dependencies remain dev-only | Recorded client/queue tests, SDK dependency trees, `check:client-tier`, and language Bifrost journeys pass | PASS |
| AC-003: preserved Card and `WyrdState` workflows are proven through affected first-class surfaces | Rust/Python/TypeScript SDK journeys exercise registration, retrieval, hydration, offline state, denial, and replay across the real server boundary | Recorded Rust SDK, TypeScript Cards/state, Python Cards/state, HTTP/CLI/MCP and Postgres-backed lanes pass | PASS |
| AC-004: real first-class SDK Bifrost journeys cover lifecycle, ingest, durability, query, typed collection, streams, description, and failures | All three language surfaces call the shared `Bifrost` facade and retain their idiomatic conversion layer only | Recorded Rust/Python/TypeScript journey lanes pass; the prohibited aggregate was not substituted for focused proof | PASS |
| AC-010, INV-010, INV-019: no retired typed-read, audit verification, bootstrap, stale aliases, parallel client authority, or live UI entered the range | Source and generated-contract inspection finds only canonical SQL reads and the approved SDK/error surfaces | Recorded source/boundary/codegen/CLI checks pass | PASS |
| AC-021: Oracle client/queue behavior and non-Bifrost Surfaces workflows coexist without textual blending | Cumulative diff retains the Oracle Bifrost owners and reconnects Cards/storage/state consumers under `wyrd-client`; no `vala-sdk` sibling client remains | Recorded focused client/queue and all affected language/Card/state journeys pass | PASS |
| Postgres lifecycle correction: five named lanes own isolated repository-managed Postgres | `mise.toml` uses `scripts/postgres/with-test-postgres.sh` outer/inner tasks and module pytest invocation; deleted shared setup tasks were not restored | Recorded five lane passes and `test:postgres:inventory` pass | PASS |
| Constraint: only the Python SDK activates PyO3 and retained owner Python features | `wyrd-sdk-python` has one optional `python` feature; PyO3 and owner `/python` activations are behind it; `testing` includes `python`; extension items are cfg-gated; maturin explicitly enables the feature | Independent default inverse tree found no `pyo3`; recorded enabled tree, `check:pyo3-scope`, production-wheel, setup, lint, and unit checks pass | PASS |
| Constraint: insert racing shutdown is refused or included in the terminal drain | Producer admission checks closure while holding the producer-map mutex; shutdown closes and snapshots under the same mutex | Independently passed `shutdown_and_producer_admission_share_the_pool_lock`; source ordering proves no producer can be returned or created behind the snapshot | PASS |
| Constraint: public deadline domain is exactly `1..=u32::MAX` across all projections | `BifrostQueryRequest.deadline_ms` is wide signed input with centralized closed-range validation and generated min/max; gRPC, Python, TypeScript, and MCP convert losslessly or return the shared structured validation failure | Independently passed `query_request_deadline_range_is_closed_and_published`; recorded Python/TypeScript/MCP boundary tests pass | PASS |
| Constraints/non-goals: no new parallel abstraction, transport, queue, decoder, loader, exception/feature hierarchy, dependency, compatibility API, merge, push, release, deploy, or Bifrost aggregate | Remediation reuses the existing shared owners, mutex, frame decoder, stream converter, IPC decoder, docs loader, and language error projector; generated artifacts were regenerated | Complete cumulative diff and remediation diff inspection; recorded boundary and generation checks | PASS |
| Completion and verification scope | Format/lint/language/codegen/docs/client-tier/PyO3/wheel/Postgres gates and exact focused proofs are recorded; `git diff --check` is clean | Independent `git diff --check` and five exact Rust focused tests pass; full recorded lane set is green after one Python integration rerun | PASS |

## Prior-finding closure

| Stable finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` | One shared `WyrdClientError -> WyrdError` conversion is used by Cards and Bifrost; exact code/status proof passes | CLOSED |
| `FIND-TASK-002-2` | Native Cards/Bifrost connect and table-description failures return structured metadata consumed by `projectedError`; four public TypeScript tests pass | CLOSED |
| `FIND-TASK-002-3` | Seven Bifrost route annotations and generated OpenAPI carry typed common/default and route-specific refusal bodies; focused OpenAPI proof passes | CLOSED |
| `FIND-TASK-002-4` | Existing `CodeFromFile` resolver includes `sdks`; all five stale embeds point to relocated examples; docs checks pass | CLOSED |
| `FIND-TASK-002-5` | `wyrd.errors` re-exports only the root extension class and identity is tested | CLOSED |
| `FIND-TASK-002-6` | One optional `python` feature gates PyO3, owner Python activations, and extension code; default inverse tree excludes PyO3 | CLOSED |
| `FIND-TASK-002-7` | Added/materially relocated Rust items in scope received intent, invariant, error/panic, and cancellation documentation; lints/docs checks pass | CLOSED |
| `FIND-TASK-002-8` | Cited relocated signatures use module-top imports and bare names; formatting/lints pass | CLOSED |
| `FIND-TASK-002-9` | Lifecycle/raw query methods live on `Bifrost`; sibling public exports/accessors are gone; production callers route through the facade | CLOSED |
| `FIND-TASK-002-10` | Admission and close/snapshot share the existing producer-map lock; deterministic focused proof passes | CLOSED |
| `FIND-TASK-002-11` | Failed terminals use the existing deadline-bounded clean-EOF proof before retention and settlement; trailing-frame cases pass | CLOSED |
| `FIND-TASK-002-12` | Pending storage holds encoded frames and performs Arrow conversion only for the next delivery; coalesced-chunk proof passes | CLOSED |
| `FIND-TASK-002-13` | Shared request validation and generated schema enforce `1..=u32::MAX`; all language/wire projections agree and focused boundary proofs pass | CLOSED |

## Open questions

None. No correction requires a product, public API, architecture, security, compatibility, concurrency, resource-ownership, or persistent-data decision.

## Verification notes

- Reviewed the complete cumulative diff, current candidate source, manifests, generated OpenAPI/schema/declarations, original task, prior verdict and reports, remediation task, applicable architecture, and recorded implementation evidence.
- Independently passed:
  - `git diff --check 861f8d86c..4d9d74b34`
  - `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=cards::error::tests::cards_client_failures_keep_client_catalog_identity)'`
  - `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::handle::tests::shutdown_and_producer_admission_share_the_pool_lock) | test(=bifrost::query::tests::failed_terminal_requires_clean_eof) | test(=bifrost::query::tests::coalesced_chunk_decodes_one_batch_per_delivery)'`
  - `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::api::query_terminal_tests::query_request_deadline_range_is_closed_and_published)'`
  - `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::openapi::tests::bifrost_operations_publish_typed_problem_refusals)'`
  - Default `wyrd-sdk-python` normal dependency inverse tree reports no `pyo3` package, which is the expected negative result.
- Recorded green evidence additionally covers Rust format/lints and Card/CLI/state lanes; Python format/lints/unit/type/integration/Card/state and wheel checks; TypeScript build/type/unit/integration/N-API checks; docs/codegen/proto/client-tier/PyO3/Postgres inventory; and focused Rust SDK, CLI, MCP, Python, and TypeScript scenarios.
- `mise run py:test:integration` reported one multipart-completion conflict on its first run, then passed 55/55 immediately; the same state journey passed in its focused lane. The remediation does not change the server multipart completion owner, source inspection did not establish a reproducible candidate defect, and the rerun plus focused proof are retained as credible evidence. This remains a verification limit, not a finding.
- The task correctly did not run the prohibited Bifrost aggregate lane.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-002 obligations, all thirteen validated prior findings are closed, the non-goals remain excluded, and no new material finding survives source and focused-proof validation.
