# Repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Candidate observed at `HEAD` before and after review: `b954976648b49429f1c0950c4fa3509573885be8`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, revision 12; the original TASK-003 packet and R1-R5 remediation packets were read as recorded authority and evidence.
- Review method: source, manifests, cumulative diff, tests, architecture, and recorded evidence only. No build, compilation, test, Cargo, mise, pnpm, pytest, lint, formatting, or code-generation command was run.

## Applicable authority

The complete applicable route was read under the hierarchy in
`architecture/references/README.md`:

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`, especially Client model, Workflow, runtime identity, error catalog, and reference loading
- `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`, especially credential lifecycle, authorization, external network safety, secret handling, and audit
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/maintainer-style.md`
- `architecture/references/languages/rust-core.md`
- `architecture/references/languages/pyo3-boundaries.md`
- `architecture/references/languages/python-api-and-stubs.md`
- `architecture/references/languages/testing-workflows.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/errors.md`

`architecture/bifrost-design.md` and Vala analytical references do not apply:
the cumulative diff changes no Bifrost ingest, query, storage, publication,
maintenance, or analytical contract.

## Authority coverage

| Changed surface | Governing authority | Source and consumer coverage | Result |
|---|---|---|---|
| Shared Workflow facade and registered/local loading | `AGENTS.md` §§2-6, 9, 15-16; Wyrd design Client model and Workflow; architecture patterns Client pattern; Rust core | `crates/shared/wyrd-client/src/workflow/{mod,local,remote,gateway}.rs`; existing Cards hydrator and loader callers; Rust, Python, and TypeScript projections | PASS |
| Shared authentication and native HTTP transport | `AGENTS.md` §§4, 6, 9; security posture credential lifecycle; Rust core Async, Errors, Secrets | `crates/shared/wyrd-client/src/{auth.rs,transport/http.rs}`; sole `post_native` caller in `workflow/gateway.rs`; focused transport tests | PASS |
| Shared local configuration and secret resolution | `AGENTS.md` §§2-4, 6; security posture Cryptography and secret handling; architecture patterns Client and Storage patterns | `wyrd-client/src/{global_config.rs,config.rs,workflow/local.rs}`; `wyrd-utils/src/secret.rs`; gateway and server consumers | PASS |
| Skald execution route and Prompt/model projection | `AGENTS.md` §§2-6, 10; architecture constraints Product boundaries; Wyrd design Workflow; Rust core | `crates/skald/skald-workflow/src/{plan.rs,route.rs,workflow.rs}` and the existing provider/endpoint owners | PASS |
| Shared fallback wire contract | `AGENTS.md` §§2, 3, 9; architecture patterns Contract placement; agent harness; errors | `crates/wyrd-spec/src/gateway/{mod.rs,policy.rs}`; client producer and all four server consumers | PASS |
| Public gateway ingress and OpenAPI | `AGENTS.md` §§2, 9, 11; Wyrd design Workflow; security posture Authorization; agent harness | `wyrd-server/src/components/gateway/{ingress.rs,routes.rs}`; `pg_invocation_tests.rs`; `tests/pg_openapi_contract.rs` | PASS |
| Provider and server credential readers | `AGENTS.md` §§3-4, 6; security posture Secret handling; Rust core Secrets | `wyrd-gateway/src/credential.rs`, `wyrd-server/src/config.rs`, shared `read_secret_ref` | PASS |
| Python/PyO3 Workflow projection | `AGENTS.md` §§7-8; PyO3 boundaries; Python API and stubs; errors | `sdks/wyrd-sdk-python/src/{workflow.rs,state/mod.rs}`; public `wyrd.agent.Workflow`; generated public and source stubs; Python journey | PASS |
| TypeScript Workflow projection | `AGENTS.md` §§2-3, 11; architecture constraints Client tier and Surface alignment; testing workflows | No candidate TypeScript source change was needed; inspected `sdks/wyrd-sdk-ts/native/src/workflow.rs`, `wyrd/src/index.ts`, and `wyrd/tests/integration/workflow-loading.test.ts`, which already delegate load/run to `wyrd_client::Workflow` | PASS |
| Rust SDK projection | `AGENTS.md` §§2-3; Wyrd design Client model; architecture constraints Client tier | `sdks/wyrd-sdk-rust/src/lib.rs` remains a thin `wyrd-client` projection and adds only export-shape proof | PASS |
| Manifests and lockfile | `AGENTS.md` §§2-4, 15; agent rules feature/dependency-cost rules; architecture constraints Client tier | Workspace and touched crate manifests plus `Cargo.lock`; no new feature, server-tier dependency in a client tier, wildcard version, or profile block | PASS |
| Active architecture update | `AGENTS.md` §§1-2, 14-16; Wyrd design authority; spec-driven development | `architecture/wyrd-design.md:401-443` records the public fallback-header behavior implemented by the candidate | PASS |
| Tests and generated declarations | `AGENTS.md` §§8, 11-12, 16; testing workflows; agent harness | Inline Rust tests, HTTP transport test target, Postgres gateway integration, served OpenAPI contract, Python runtime journey, generated `.pyi` projections, and existing TypeScript journey | PASS |

## Rule results

| Applicable rule | Evidence | Result |
|---|---|---|
| `wyrd-client` is the sole SDK-facing shared Rust client implementation; language bindings may not duplicate transport or durable semantics. | `wyrd-client/src/workflow/mod.rs:36-44` owns the facade and retained client; Python stores `ClientWorkflow` at `sdks/wyrd-sdk-python/src/workflow.rs:204-215`; TypeScript stores `wyrd_client::Workflow` at `sdks/wyrd-sdk-ts/native/src/workflow.rs:21-26`. | PASS |
| Skald owns reusable workflow execution and stays independent of server/registry IO. | `skald-workflow/src/route.rs:202-212` owns execution dependencies while the client facade delegates to Skald at `wyrd-client/src/workflow/mod.rs:155-160`; no server-tier dependency was added to Skald. | PASS |
| Stateful workflows and dependency-backed behavior need cohesive concrete owners and inherent methods. | `Workflow`, `Workflows`, `PublicWyrdGatewayCaller`, `SelectedRoutes`, `WorkflowExecutionDependencies`, and `GatewayFallbackOverride` own the changed workflows. Free helpers are narrow projection, conversion, or error constructors. | PASS |
| Async is limited to real IO or intentional IO composition; blocking filesystem/config work must not occupy polling threads. | `Workflow::from_path` and `Workflow::run_with` move synchronous loading/config assembly to `spawn_blocking` (`workflow/mod.rs:78-96`, `142-160`); secret reads do the same (`workflow/local.rs:126-140`, `wyrd-gateway/src/credential.rs:161-177`). | PASS |
| Public contracts belong in PyO3-free, IO-free `wyrd-spec`; public errors remain structured. | Fallback encoding/validation is declarative in `wyrd-spec/src/gateway/policy.rs:88-184`; IO stays in client/server; public server failures continue through `WyrdError` and native protocol envelopes. No PyO3, Tokio, HTTP client, or server framework entered `wyrd-spec`. | PASS |
| Public gateway handlers authenticate before request-specific fallback parsing and preserve server-owned authorization/audit behavior. | Authentication middleware wraps all ingress routes at `wyrd-server/src/components/gateway/ingress.rs:101-169`; each handler parses fallback only inside the `Ok(caller)` branch (`ingress.rs:261-297`, corresponding Gemini path, and `routes.rs::openai_call`). The existing `GatewayInvocation` remains the sole governed invocation owner. | PASS |
| Tenant/principal identity may come only from verified authentication state. | The new header carries only fallback candidates. `Caller` remains middleware-derived; no tenant, principal, credential, or role field was added to Workflow or gateway payloads. | PASS |
| Tenant-controlled external URLs require effective-address screening, connection pinning, redirect refusal, and bounded transport. | ExtGateway construction continues through `EndpointPolicy` and `ExternalGatewayClient` at `skald-workflow/src/route.rs:326-354`; the existing owner documents and implements screened DNS, pinned answers, no proxy, and no redirects in `skald-providers/src/endpoint.rs:1-10,98-130`. | PASS |
| Secrets use redacted types and are resolved only at the owning runtime boundary. | `wyrd-utils/src/secret.rs:23-39` returns `SecretString`; file validation is on the opened handle (`:42-69`); local Workflow bindings resolve only selected references (`workflow/local.rs:95-110,126-148`); debug implementations expose names, never values. | PASS |
| Public native model calls must have bounded cancellation and must not replay a potentially dispatched model call. | `PublicWyrdGatewayCaller::call` races cancellation and the call deadline (`workflow/gateway.rs:52-124`); `HttpTransport::post_native` sends once and renews a rejected credential without replay (`transport/http.rs:351-410`). Cancellation and partial progress are documented. | PASS |
| Error projections must be stable and redact provider/transport internals. | Native refusal decoding keeps catalog code/title/remediation or a fixed provider category (`workflow/gateway.rs:157-213`); pre-response Wyrd errors use the catalog projection (`:313-329`); raw upstream message text is not retained. | PASS |
| New or materially modified Rust items require substantive rustdoc, `# Errors`, panic contracts, and relevant cancellation/partial-progress documentation. | Changed production items and new test helpers carry item/field docs; fallible items have `# Errors`; panic-capable new tests/helpers have `# Panics`; durable async client operations explain drop, retry, and partial-progress boundaries. | PASS |
| Imports live at module top and signatures use imported bare or role-specific aliased types. | Changed modules use top-level imports; the Python boundary deliberately aliases `SkaldWorkflow` and `ClientWorkflow`; no new function-scoped import or qualified signature was found. | PASS |
| Python wrappers convert at the edge, use the shared runtime bridge, and retain no `Bound` value across an await. | `PyWorkflow::from_path` and `run` detach the GIL and use `wyrd_runtime::runtime()` (`sdks/wyrd-sdk-python/src/workflow.rs:481-562`); Python values are converted before the Rust async call. | PASS |
| Public Python source, registration, package exports, generated stubs, and runtime tests stay aligned. | Existing registration/export remains intact; both generated stub projections carry the revised `Workflow.run` contract; the added integration test imports only public `wyrd` modules. Recorded `codegen:check`, Python typecheck, lints, and tests are green. | PASS |
| Every public surface projects the same durable vocabulary and fields. | Rust/Python/TypeScript all use the shared `Workflow`; the fallback contract is one `GatewayFallbackOverride`; the server and client use the same `FALLBACK_HEADER`; no compatibility alias or second vocabulary was added. | PASS |
| Generated artifacts are changed only through their source/generator and checked for drift. | The two `.pyi` changes match the PyO3 documentation source, and R5 records `mise run codegen:stubs` followed by green `codegen:check`; OpenAPI is tested from the served document rather than checked in. | PASS |
| External Rust tests must earn a separate binary; Python tests use top-level `def test_*`; user-facing behavior needs journey evidence at its owning phase. | `workflow_transport.rs` exercises real HTTP/mock transport seams; the Python journey is a top-level test against `WyrdTestServer`; existing TypeScript integration drives the shared Workflow facade. TASK-003 explicitly assigns the full local Workflow-to-public-gateway journey to TASK-005 and server lifecycle to TASK-004, while this task supplies the scoped transport/server/Python proofs it owns. | PASS |
| Do not introduce unsupported machinery, checks, settings, options, or compatibility paths. | No new repository check, Cargo feature, allowlist, compatibility alias, retry setting, polling option, transport, gateway route, or credential API appears in the cumulative diff. The new request-scoped header, shared configuration section, shared secret reader, and focused test files are all directly established by the approved revision/current design and use conventional typed-header, config, secret-wrapper, and HTTP-fixture mechanisms. No human-directed `DRIFT` item was found. | PASS |
| No legacy vocabulary, task/agent references in permanent code, or gate circumvention. | Production source contains no plan/task/agent-history reference, new `#[allow]`, `#[ignore]`, weakened test, or widened boundary exclusion. | PASS |

## Material findings

None.

The cumulative candidate contains no material repository-rule violation. No
optional improvement or style preference is promoted to a finding.

## Verification evidence and limits

This reviewer ran no verification lane, as required. The following evidence
was inspected in the immutable task/remediation packets rather than rerun:

- focused `wyrd-client` remote-lifecycle, native-gateway, cancellation,
  renewal, config-snapshot, and loader selectors;
- focused `skald-workflow`, `wyrd-spec`, `wyrd-gateway`, `wyrd-server`, served
  OpenAPI, and Python retained-client selectors;
- `test:shared`, `test:wyrd-sdk`, `test:gateway:native`,
  `test:principals:integration`, and the gateway Postgres module;
- Rust format/lints, Python format/lints/typecheck/unit tests, TypeScript unit
  tests/typecheck/N-API declaration check, code-generation drift, client-tier,
  SDK-client-tier, and PyO3-scope checks.

The latest R5 packet records a focused mutation turning the one-snapshot
mixed-route proof red and then green after restoration. Earlier remediation
packets record direct red/green proof for native `401` renewal ordering,
already-dispatched cancellation, retained Python client context, and catalog
error redaction. These are credible recorded results for this static review;
they are not fresh execution by this reviewer. TASK-004 and TASK-005 still own
the server lifecycle and full public user journeys assigned to them by the
approved plan, so their future evidence is not a TASK-003 standards failure.

## Overall result

**PASS**
