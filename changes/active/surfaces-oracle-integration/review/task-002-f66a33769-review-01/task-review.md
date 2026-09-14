# TASK-002 Implementation Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f66a337698940920dca20b126c1c6c28a6390191`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Candidate identity was `f66a337698940920dca20b126c1c6c28a6390191` before and after review.

## Review Findings

### Critical

None.

### Important

#### TASK-REV-001 — INCORRECT: Cards collapses stable client failures into unrelated catalog errors

- Violated obligation: REQ-023 and the task requirement that public errors agree through the shared client; REQ-024A for the Python projection; the constraint that all failures be explicitly handled in the current execution context.
- Exact location: `crates/shared/wyrd-client/src/cards/error.rs:36-49`; reachable from `crates/shared/wyrd-client/src/cards/config.rs:15-33`, `crates/shared/wyrd-client/src/client.rs:79-82`, and `sdks/wyrd-sdk-python/src/state/mod.rs:1488-1491`.
- Evidence: `WyrdClientError::Config` becomes `WyrdError::Internal`, `NoCredentials` becomes `WyrdError::Internal`, and `TransportDown` becomes `RegistryUnavailable`. The catalog already has the exact `ClientConfigInvalid`, `ClientNoCredentials`, and `ClientTransportDown` variants at `crates/wyrd-spec/src/error.rs:3182-3208,3252-3264`; Bifrost already maps these three errors correctly at `crates/shared/wyrd-client/src/bifrost/query.rs:147-160`.
- Reachable consequence: `Cards::new(None, None)` with an empty credential chain returns `WYRD_INTERNAL_500` instead of `WYRD_CLIENT_401_NO_CREDENTIALS`; an empty explicit server URL returns an internal 500 instead of `WYRD_CLIENT_400_CONFIG_INVALID`. Rust callers and `wyrd.cards.Cards(...)` therefore observe different codes/statuses from Bifrost for the same shared-client failure, and Python loses the catalog-backed stable failure required by REQ-024A.
- Required testable correction: make the shared `wyrd-client` owner perform one canonical `WyrdClientError -> WyrdError` projection and reuse it from both Cards and Bifrost, preserving all three existing `WYRD_CLIENT_*` variants. Add one focused, environment-isolated test covering at least no-credentials and invalid configuration through `Cards::new`, and assert the exact code/status through the public Python `Cards` constructor.

#### TASK-REV-002 — INCORRECT: TypeScript connection failures bypass the runtime `WyrdError`

- Violated obligation: REQ-021, REQ-025, and the task acceptance criterion requiring TypeScript runtime structured errors through `@wyrd/sdk`.
- Exact location: `sdks/wyrd-sdk-ts/native/src/cards.rs:34-47`, `sdks/wyrd-sdk-ts/native/src/lib.rs:403-419,998-1000`, and `sdks/wyrd-sdk-ts/wyrd/src/index.ts:651-666,977-980`.
- Evidence: both native connection functions convert expected Wyrd-owned failures with `napi_error`, which retains only `Display` text in a generic `napi::Error`. The TypeScript `Bifrost.connect` and `Cards.connect` wrappers return that rejection directly and never run it through `projectedError`, although lifecycle methods use `NativeLifecycleResult` to preserve code, status, title, detail, remediation, and details. Existing TypeScript tests always supply credentials and do not cover connection refusal.
- Reachable consequence: a user with no configured credential, invalid client configuration, or failed Bifrost connection receives a generic JavaScript `Error`, not `instanceof WyrdError`, and cannot inspect the generated `WyrdErrorCode`, status, remediation, or details promised by the SDK contract.
- Required testable correction: return connection failures through the existing structured native metadata/result mechanism and construct the public `WyrdError` with `projectedError`; do not parse an error message. Add focused TypeScript tests proving missing-credential failure for `Cards.connect` and `Bifrost.connect` is `instanceof WyrdError` with `WYRD_CLIENT_401_NO_CREDENTIALS` and status 401.

#### TASK-REV-003 — MISSING: generated Bifrost table routes omit their structured failure contract

- Violated obligation: REQ-059 and AC-002, which require the generated OpenAPI authority to include Bifrost table routes and structured errors.
- Exact location: `crates/wyrd/wyrd-server/src/bifrost/routes.rs:22-27,47-51,69-77`; generated result `openapi.yaml:12-89`.
- Evidence: register and list declare only a 200 response. Describe declares a 404 description without a body. The handlers themselves return `WyrdErrorResponse` and their rustdoc names validation, authentication/authorization, conflict/not-found, and availability failures, but none of those responses is represented as `WyrdProblem`. Existing Card route annotations demonstrate the repository mechanism at `crates/wyrd/wyrd-server/src/components/cards/routes.rs:218-221,275-279`.
- Reachable consequence: generated clients and agents cannot discover or type the actual problem payload for table registration/list/description failures, so the published HTTP contract is incomplete even though runtime handlers return structured failures.
- Required testable correction: declare the reachable non-success responses with `WyrdProblem` bodies on all three table operations, regenerate `openapi.yaml`, and extend the focused OpenAPI test to assert the response statuses and `$ref` problem schema rather than only path presence.

#### TASK-REV-004 — REGRESSION: moving the Python SDK leaves two public docs pages rendering as HTTP 500

- Violated obligation: INV-001 and the task's SDK-root move, which must preserve the affected public Python authoring workflows rather than leave consumers pointing at the deleted package root.
- Exact location: `docs/src/content/docs/how-to/build-an-agent.svx:18,52`, `docs/src/content/docs/how-to/build-a-workflow.svx:18,74,92`, and `docs/src/lib/components/CodeFromFile.svelte:20,27`.
- Evidence: the five `CodeFromFile` references still name deleted `python/py-wyrd/examples/*` files; the files now live under `sdks/wyrd-sdk-python/examples/*`. The component's eager glob and `repoTail` matcher also exclude `sdks`, so changing only the call-site strings cannot resolve them. A focused `mise run docs:build` on the immutable candidate emitted `[500] GET /wyrd/how-to/build-an-agent/` and `[500] GET /wyrd/how-to/build-a-workflow/` with `CodeFromFile: file not found`, while exiting zero because `docs/svelte.config.js:196` treats prerender HTTP errors as warnings.
- Reachable consequence: both linked public how-to pages are generated as 500 error pages instead of documentation. The recorded `docs:check` pass is not credible proof for these pages because its build step accepts those prerender warnings.
- Required testable correction: teach the existing `CodeFromFile` component to resolve repository-relative `sdks/**` sources and update the five embeds to the actual SDK paths. As focused closure proof, build the docs and assert that neither route emits a prerender 500 and that both generated pages contain their embedded Python example; no new docs framework or duplicate examples are needed.

### Suggestions

None. Optional improvements and unrelated debt were excluded.

## Open Questions

None. All retained findings are resolved by the approved specification and existing repository mechanisms.

## Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-060, REQ-061, INV-024, AC-021: preserve Oracle bounded queue/client behavior and expose Bifrost through `wyrd_client::Bifrost` | Oracle Bifrost modules remain under `crates/shared/wyrd-client/src/bifrost`; `crates/shared/wyrd-client/src/bifrost/mod.rs` exposes the facade and queue-backed behavior; sibling `vala-sdk` is removed | Recorded Rust/Python/TypeScript Bifrost and focused CLI results; Bifrost aggregate intentionally excluded by task | PASS |
| REQ-004 through REQ-009 and INV-001: preserve Card registration, references, CardRef shape, Eval/Drift semantics, catalog, and Audit Card semantics | Card implementation moved into `wyrd_client::cards` with narrow import/path changes; Card/spec sources remain authoritative; Rust/Python/TS journeys exercise composite registration and hydration | Recorded `test:cards:unit`, `test:cards:integration`, CLI, Python, TypeScript, and Rust SDK journeys | PASS, except the separate docs regression in TASK-REV-004 |
| REQ-016 through REQ-018, INV-004: one shared implementation and thin SDK roots | `wyrd-client` owns Cards, storage, state, transport, auth, and Bifrost; old `wyrd-registry`, `wyrd-storage-client`, and shared `wyrd-sdk` crates are deleted; Rust SDK is a re-export and foreign runtimes delegate to `wyrd-client` | Recorded `check:client-tier`, SDK builds/typechecks, and dependency trees | PASS |
| REQ-019, INV-005: one public Bifrost facade, no sibling engine clients | `Bifrost` owns the public workflow; raw ingest transport is gated to test-support; SDK projections construct the facade | Recorded Rust/Python/TS Bifrost journeys and static surface checks | PASS |
| REQ-019A: one explicit shared `ClientConfig::credential` field | `crates/shared/wyrd-client/src/config.rs:40-50`; Card/Python/TS constructors use `credential` | Unit tests and recorded language integration journeys | PASS |
| REQ-020: preserve Python Card/data/model/prompt/state/Bifrost sync and async surfaces | Package is rooted at `sdks/wyrd-sdk-python`; public modules and PyO3 aggregation remain present | Recorded Python unit, integration, Card, WyrdState, and typecheck lanes | PASS |
| REQ-021: preserve TypeScript Bifrost/Card/WyrdState behavior and structured errors | `sdks/wyrd-sdk-ts` projects Bifrost, Cards, and state through N-API | Card/state and Bifrost journeys pass, but no constructor-refusal test exists and native connection errors bypass `WyrdError` | FAIL — TASK-REV-002 |
| REQ-056: schema-once/batches/EOS/one-terminal stream invariants and settlement | Shared query stream/terminal logic remains in `wyrd-client`; language iterators delegate to it | Recorded Python/TypeScript integration and unit stream results | PASS |
| REQ-057, AC-019 (MCP): exactly three bounded Bifrost tools | MCP implementation unchanged by this range; focused discovery names the three approved tools | Recorded exact MCP discovery command | PASS |
| REQ-058, AC-019 (CLI): preserve query input/output through Bifrost | CLI consumes `Bifrost::query_only`; query contract remains in the shared facade | Recorded CLI journey and exact focused Card lifecycle command | PASS |
| REQ-059, AC-002: complete generated HTTP/gRPC/client contract including structured errors | Table paths and schemas were added to OpenAPI, but table-operation failure responses are absent or untyped | `codegen:check` proves artifact drift only; source/generated inspection disproves completeness | FAIL — TASK-REV-003 |
| REQ-022: only `WYRD_REGISTRY_*` registry codes | Removed crates are replaced by `wyrd_client::cards`; no new compatibility prefix was introduced | Recorded codegen/error checks and source search | PASS |
| REQ-023: preserve general/storage/Bifrost catalog and reconstruction | Catalog and transport reconstruction remain, but Cards converts three `WyrdClientError` variants to unrelated errors | Existing Bifrost error tests do not cover Cards construction mapping | FAIL — TASK-REV-001 |
| REQ-024, REQ-024B: one Python eight-field `WyrdError`; prohibited aliases/classes absent | fake `wyrd.errors` aliases were deleted; SDK registers the shared PyO3 error and stubs expose eight fields | Recorded `py:test:unit` and `py:typecheck`; source search finds no prohibited Python classes | PASS |
| REQ-024A: every stable public Python failure preserves catalog metadata | Bifrost does; Python Cards delegates to the incorrect Cards mapping | No Python missing-credential Cards test; static reachable path yields internal 500 | FAIL — TASK-REV-001 |
| REQ-025: TypeScript runtime `WyrdError` and generated code union | Union is generated from `WyrdError::codes`; lifecycle failures project structured metadata, but connection failures do not | Error union/unit test covers lifecycle projection only | FAIL — TASK-REV-002 |
| REQ-040, REQ-041, INV-013, AC-010: only Python SDK enables retained Python features; new Python boundary logic lives there | Python aggregation is under `sdks/wyrd-sdk-python`; Rust/TS manifests do not enable Python features | Recorded `check:pyo3-scope`, `check:py-wheel-no-testing`, and dependency trees | PASS |
| REQ-043, REQ-044, INV-010, INV-019: do not restore retired audit CLI, bootstrap, aliases, or typed observation reads | No such surface is added in the range; docs were rewritten around SQL-only reads | Recorded CLI/codegen checks and source inspection | PASS |
| INV-006: client tier excludes server/SQL/cloud/DataFusion/Delta/Iceberg dependencies | `wyrd-client` and SDK normal manifests remain client-tier | Recorded `check:client-tier` | PASS |
| AC-003: affected Rust, Python, TypeScript, HTTP, CLI, and MCP Card/WyrdState journeys | New Rust and TS SDK journeys plus retained Python/CLI/HTTP/MCP tests cover the runtime workflows | Recorded focused and aggregate task lanes | PASS |
| AC-004: real SDK Bifrost journeys cover lifecycle, ingestion, durability, SQL, typed collection, streams, and description | Existing Oracle journeys moved with SDKs and continue through the facade | Recorded Rust/Python/TS lanes; aggregate intentionally not run | PASS |
| All five Postgres lifecycle lanes use the repository harness | `mise.toml` outer/inner tasks use `scripts/postgres/with-test-postgres.sh` and module pytest invocation | Recorded five lane passes and `test:postgres:inventory` | PASS |
| Constraint: Oracle client/queue supersedes Surfaces without losing bounded ownership, settlement, backpressure, drain, credentials, TLS, transport, or reconstruction | Oracle implementation remains the shared core; Surfaces non-Bifrost modules were composed around it | Recorded client/queue tests and language journeys | PASS |
| Constraint: preserve Surfaces Card authoring/loading/composite/reference/Eval/Drift/WyrdState behavior | Shared Card/state code was moved with minimal semantic changes and exercised through SDKs | Recorded Card/WyrdState lanes | PASS |
| Constraint: SDK roots are thin and do not duplicate transport/durable behavior | Rust re-exports; Python and TS provide foreign-runtime projections over shared Rust owners | Manifest/diff inspection and recorded client-tier check | PASS |
| Constraint: only Python SDK enables owner Python features | Python SDK is the sole aggregator | Recorded PyO3 scope/dependency evidence | PASS |
| Constraint: no Gate/Scribe/Oracle/Forge/QueryClient/BifrostGrpcTransport sibling public owner | No new sibling owner; test-only raw transport remains feature-gated | Public-surface inspection | PASS |
| Constraint/non-goals: do not restore typed reads, audit verification, bootstrap, obsolete aliases, live UI, tracked native outputs, compatibility APIs, merge/push/deploy, or run Bifrost aggregate | None was added or performed in the reviewed range | Diff/source inspection and recorded task evidence | PASS |
| Constraint: every failure is explicitly handled in this execution context | Cards misclassifies stable client failures; TypeScript constructors degrade expected failures to generic errors; docs gate records two prerender 500s as warnings | Static caller tracing and focused `mise run docs:build` | FAIL — TASK-REV-001, TASK-REV-002, TASK-REV-004 |

## Verification Notes

- Reviewed the complete `861f8d86c..f66a33769` diff, current source, manifests, generated OpenAPI, task evidence, and applicable specification/repository rules.
- Available recorded passes: Rust format/lints and Card/CLI/WyrdState lanes; Python unit/integration/typecheck lanes; TypeScript build/typecheck/unit/integration/N-API lanes; codegen, docs, examples, client/PyO3/wheel/response/protobuf/Postgres checks; focused CLI, MCP, and Rust SDK journeys.
- Independently ran `mise run docs:build`. It exited zero but emitted deterministic prerender 500s for the two how-to routes in TASK-REV-004. This invalidates the recorded docs pass as proof for those routes.
- The task-required Bifrost aggregate was correctly not run. No other broad lane was rerun during this static acceptance audit.
- `git diff --check 861f8d86c..f66a33769` passed. The read-only checks changed no reviewed source; only review artifacts exist outside the immutable candidate.

## Overall Result

**FAIL**

Four bounded, reachable implementation findings prevent exact TASK-002 acceptance: `TASK-REV-001`, `TASK-REV-002`, `TASK-REV-003`, and `TASK-REV-004`.
