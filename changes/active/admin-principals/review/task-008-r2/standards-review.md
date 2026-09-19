# TASK-008 review `task-008-r2` — repository-standards review

Reviewer: `repo-rev` (repository-standards scope only). No task-acceptance
verdict, no Ponytail ladder audit, no optional improvements.

| Item | Value |
|---|---|
| Candidate HEAD | `4668d8d333ad043b4b2f8c258beb78eebb719466` |
| Closeout range | `289978fcc~1..4668d8d33` (9 commits) |
| Working tree at report time | clean (`git status --porcelain` empty) |
| Overall result | **FAIL** |

## 1. Authority coverage

Authorities read in full: `AGENTS.md`; `architecture/agent-rules.md`;
`architecture/references/README.md` (router) and, per its routing table, the
slices below; `TESTING.md` §§ tier homes and lanes; `mise.toml` (`test:cli:journey`,
`test:wyrd`, `check:*`, `codegen:check`, `docs:check`); `.config/nextest.toml`;
`scripts/run-family-tests.sh`; `scripts/test-families.sh`.

References selected through the router for these surfaces:
`languages/rust-core.md`, `languages/errors.md`, `languages/testing-workflows.md`,
`languages/agent-harness.md`, `architecture/patterns.md`,
`doctrine/architecture-constraints.md`. Not applicable and deliberately not
loaded: every `domain/*` slice (no Vala/Bifrost/Iceberg/DataFusion/Arrow surface
changed), `languages/pyo3-boundaries.md`, `languages/python-api-and-stubs.md`,
`languages/typescript-guide.md` (no Python- or TypeScript-visible file changed —
confirmed against the diff file list and by `check:pyo3-scope`,
`check:sdk-client-tier`, and `codegen:check` all passing with no stub drift).

| Changed surface | Governing authority (file · section) |
|---|---|
| `crates/shared/wyrd-auth-verify/src/lib.rs` (new `signing_key`) | AGENTS.md §4, §5, §6, §16 · agent-rules.md rustdoc, struct-style, async |
| `crates/shared/wyrd-auth-issue/src/lib.rs` (test repair) | AGENTS.md §11 taxonomy, §16 · agent-rules.md rustdoc |
| `crates/shared/wyrd-client/src/auth.rs` (new `TokenExchange`, `AuthError::into_wyrd`) | AGENTS.md §3 client ownership, §4, §5, §6, §9, §15, §16 · references `languages/errors.md` §Boundary Conversion |
| `crates/shared/wyrd-client/src/eval/**` (new module) | AGENTS.md §3, §5 (canonical `Cards` pattern), §6, §9, §15, §16 |
| `crates/shared/wyrd-client/src/platform/**` | AGENTS.md §4 (redacted `Debug`), §5, §9 tenant/plane isolation, §16 |
| `crates/shared/wyrd-client/src/principals/handle.rs` | AGENTS.md §5, §9, §16 |
| `crates/shared/wyrd-client/src/transport/http.rs` | AGENTS.md §4, §5, §6, §9, §15, §16 |
| `crates/wyrd-spec/src/error.rs` | AGENTS.md §4 (`WyrdError` derive catalog), §9, §15 (`wyrd-spec` scope), §16 · references `languages/errors.md` |
| `crates/wyrd-spec/{schemas,tests/schemas}/ui_problem_examples.json` | agent-rules.md "never hand-edit generated artifacts"; AGENTS.md §11 codegen lane |
| `crates/wyrd/wyrd-auth/src/platform_credentials.rs` (test rustdoc) | AGENTS.md §16 · agent-rules.md rustdoc |
| `crates/wyrd/wyrd-cli/Cargo.toml`, `Cargo.lock` | AGENTS.md §4 (no wildcards, no per-crate profiles), §15 (dependency cost), `check:client-tier` / `check:sdk-client-tier` |
| `crates/wyrd/wyrd-cli/src/client.rs` (new) | AGENTS.md §5 struct-centered style, §6, §15, §16 · agent-rules.md struct-style |
| `crates/wyrd/wyrd-cli/src/{auth/**,card.rs,error.rs,eval/**,principal/revoke.rs,query/mod.rs,lib.rs}` | AGENTS.md §4, §5, §6, §15, §16 · references `languages/errors.md` §CLI Errors |
| `crates/wyrd/wyrd-cli/tests/{cli.rs,principal_journey.rs,eval_server_protocol.rs}` | AGENTS.md §11 taxonomy + verification scope, §16 · agent-rules.md test placement · TESTING.md tier homes |
| `crates/wyrd/wyrd-server/src/auth/revoke.rs` | AGENTS.md §9 (typed bodies, structured errors, audit context, versioned contract), §16 |
| `crates/wyrd/wyrd-server/src/components/auth/**` | AGENTS.md §9 tenant isolation, §15 root-cause-once, §16 · agent-rules.md tenancy |
| `crates/wyrd/wyrd-server/src/components/eval/routes.rs` | AGENTS.md §9, §15, §16 |
| `crates/wyrd/wyrd-server/src/http/openapi.rs` | AGENTS.md §5, §9 explicit versioned contracts, §16 · agent-rules.md generated artifacts |
| `crates/wyrd/wyrd-server/tests/{pg_eval_v1_protocol.rs,platform_admin_e2e.rs}` | AGENTS.md §11, §16 · agent-rules.md test placement |
| `docs/scripts/generate_api_docs.py`, `docs/src/content/docs/api/*.md` | AGENTS.md §2 (machine-readable docs are primary), §11 `docs:check` |
| `openapi.yaml` | AGENTS.md §9, §12 · agent-rules.md generated artifacts |
| `changes/active/admin-principals/tasks/TASK-008-*.md` | AGENTS.md §14 (packet location only; acceptance content is out of my scope) |

## 2. Rule-by-rule result

### AGENTS.md §4 Rust Core Rules

| Rule | Result | Evidence |
|---|---|---|
| Domain newtypes over raw strings for durable ids | PASS | `principal/revoke.rs:45` now parses a `PrincipalId` before use, replacing a `String` interpolated into a URL; `eval/handle.rs:601` stores `RunId` |
| `&str`/`&[T]`/typed refs over owned params | PASS | `client.rs:31,54`; `eval/handle.rs:578,630,639,652`; `transport/http.rs:1108` `headers: &[(&str,&str)]` |
| `.clone()` is a design question | PASS | Every new clone is `Arc::clone` (`client.rs:57`, `eval/server.rs:38`, `eval/handle.rs:584`) or a small boundary value needed because the source is still borrowed (`principal/revoke.rs:47` `args.id.clone()`, `issue_key.rs:59`, `platform/handle.rs:59` `SecretString`). No `CLONES.md` waiver needed or added |
| `thiserror` in libraries, `anyhow` only in binaries | PASS | No `anyhow` added; `WyrdCliError` remains a `thiserror`+`WyrdError`-derive enum |
| `WyrdError` derive catalog, no hand-written `code()`/`status()`/problem-json | PASS | `wyrd-spec/src/error.rs:507,549,551,880,3118` change only `#[wyrd_error(...)]` metadata; `error.rs:2472` adds `AgentTurnFailed` with full metadata; the deleted CLI variants removed their metadata with them. No parallel accessor introduced |
| `tracing` with structured fields | PASS | `wyrd-auth-verify/src/lib.rs:163` preserves the `kid` span field when the read moved into `signing_key` |
| `secrecy::SecretString` + redacted `Debug` for secret-bearing structs | PASS | `TokenExchange` (`auth.rs:281`) and `EvalRun` (`eval/handle.rs:606`) hand-write `finish_non_exhaustive` `Debug`. `Platform` switching to `#[derive(Debug)]` (`platform/handle.rs:776`) is safe: it delegates to `WyrdClient`'s derive → `AuthMiddleware`'s manual redacted impl (`auth.rs:430`) and `HttpTransport`'s manual impl (`transport/http.rs:76`) |
| No `unwrap()` in non-test env/fs/net/parse/input/db paths | PASS | `mise run check:unwrap-audit` exit 0 |
| `expect()` only for named invariants | PASS | New `expect` calls are test-only (`principal_journey.rs`, `platform_admin_e2e.rs`); the two production `expect("…serializes")` calls the diff **deleted** are a net improvement |
| No wildcard deps, no per-crate profile blocks | PASS (for this diff) | `wyrd-cli/Cargo.toml:69` adds `wyrd-auth-verify = { workspace = true }`, dev-dependency, no version literal. The one `[profile]` warning in `mise run lints` comes from `sdks/wyrd-sdk-python/Cargo.toml`, untouched here — pre-existing, out of scope |

### AGENTS.md §5 Abstraction + required struct-centered style

| Rule | Result | Evidence |
|---|---|---|
| Concrete types for one implementation; no single-impl traits | PASS | No trait added anywhere in the diff. `SecurityAddon` (`http/openapi.rs:3721`) implements utoipa's own `Modify`, a third-party extension point, not a Wyrd trait invented for one caller |
| One owning concrete struct per stateful capability, inherent methods | PASS | `wyrd-client/src/eval/handle.rs` matches the canonical `cards/handle.rs::Cards` shape exactly: `EvalProtocol { client: Arc<WyrdClient> }` with `with_client`/`with_shared`/`open`, and a private `leased` helper on `EvalRun`. `TokenExchange` (`auth.rs:274`) owns the base URL and the pool and exposes `exchange`/`platform_session`/`begin_login` as inherent methods |
| Domain values vs service handles kept distinct | PASS | `EvalRun` carries identity + lease + the shared client and exposes `run_id()`; `EvalProtocol` holds no run state (`eval/handle.rs:534-542`) |
| No zero-sized utility struct to turn functions into methods | PASS | `wyrd-cli/src/client.rs` is deliberately five free functions. They are constructors whose product *is* the owning struct (`WyrdClient`, `Principals`); introducing a `struct CliClient;` to host them would be the banned zero-sized utility struct. This is the correct reading of §5, not an evasion of it |
| No god object | PASS | `Platform` shrank (three fields → one, `platform/handle.rs:777`); `HttpTransport` gained one header-passing seam, not a new responsibility |

### AGENTS.md §6 Async and Runtime

| Rule | Result | Evidence |
|---|---|---|
| Every `async fn` directly awaits IO | PASS | `TokenExchange::{post,decode,begin_login,exchange,platform_session}`, `EvalProtocol::open`, `EvalRun::{next_directive,submit_agent_turn,submit_user_turn,leased}`, `AgentClient::{post_turn,exchange}` all await a request or a body read |
| Validation/parsing/planning stay synchronous | PASS | `client.rs::assemble` (`:2408`), `TokenExchange::new` (`:301`), `Platform::with_session` (`platform/handle.rs:854`), `path_with_query`, `invalid`, `parse_client_auth`, `parse_principal_kind` are all synchronous |
| No ad hoc Tokio runtime in library code | PASS | No `Runtime::new`/`block_on` added. `tokio::time::timeout` (`eval/agent.rs:100`) and `spawn_blocking` (`principal_journey.rs:141`, test-only) use the ambient runtime |

### AGENTS.md §7/§8 PyO3 and Python API

| Rule | Result | Evidence |
|---|---|---|
| `wyrd-spec` stays PyO3-free | PASS | `error.rs` change is string metadata only; `mise run check:pyo3-scope` exit 0 |
| Nothing Python-visible changed | PASS | No file under `sdks/` in the diff; `mise run codegen:check` exit 0 with no `.pyi` drift |

### AGENTS.md §9 Server and Contract

| Rule | Result | Evidence |
|---|---|---|
| Typed request/response bodies | PASS | `revoke_principal` takes `Path<PrincipalId>` and `Caller`; the client sends the typed `RevokePrincipalRequest` (`principals/handle.rs:1057`) |
| Public handlers return `WyrdError`-derived structured errors | PASS | `revoke.rs:44` returns `Result<(), WyrdErrorResponse>`, unchanged |
| `#[tracing::instrument]` with scrubbed args on write handlers | PASS (no regression) | `revoke_principal` carries none, but neither does any sibling in `components/principals/routes.rs` (`create_service_principal`, `issue_credential`, `revoke_credential`). Uniform pre-existing gap across the principals surface; this diff neither introduced nor diverged from it. Recorded, not a finding of this review |
| Audit/request context on durable writes | PASS | `revoke.rs:47-60` authorizes then `record_audit`s the decision in the decision's own commit, per agent-rules.md |
| Versioned API contracts explicit | **FAIL** | See **RR-7**: `openapi.yaml:1186` |
| No compatibility routes or aliases | PASS | Nothing added. The `Authorization`→`X-Wyrd-Access-Token` move is a replacement, not an alias: `platform_extractor.rs:3471` reads one header and `platform_admin_e2e::the_two_control_planes_cannot_reach_each_other` (17/17 PASS) proves the old header authenticates nothing |
| Tenant isolation preserved on every path | PASS | `platform_extractor.rs` still produces no tenant (`platform_caller_exposes_no_tenant` PASS); `EvalProtocol::open` never names a tenant (`eval/handle.rs:570-573`); `check:client-tier` exit 0 |

### AGENTS.md §11 Testing Workflow and taxonomy

| Rule | Result | Evidence |
|---|---|---|
| New user-facing capability ships a user-journey test | PASS | `principal_journey.rs` drives the compiled `wyrd` binary over real loopback HTTP against `WyrdTestServer`, with both the happy path and a negative flow (under-privileged caller → `WYRD_PERMISSION_403_DENIED_RBAC`) |
| Journey registered in a lane that actually runs | PASS | `tests/cli.rs:11-12` declares the module; `mise.toml:141-150` `test:cli:journey` sets `WYRD_CLI_E2E = "1"`. Verified: `mise run test:cli:journey` exit 0, `principal_journey::principal_revoke_cli_journey ... ok` and `..._refuses_an_unprivileged_caller ... ok`, `22 passed; 0 failed; 5 ignored` |
| Fast lane stays credential-free / server-free | **FAIL** | See **RR-6** |
| Right tier and directory home per TESTING.md | **FAIL** | See **RR-6** |
| Named tests run through `mise exec -- cargo nextest run` | PASS | All verification in §4 below used that form with explicit `-p`, target, and `-E` expression |

### AGENTS.md §12 Completion Standard

| Rule | Result | Evidence |
|---|---|---|
| Format, lints, targeted checks pass | PASS | `fmt`, `lints`, `check:client-tier`, `check:sdk-client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `codegen:check`, `docs:check` all exit 0 |
| Public contracts regenerate cleanly | PASS | `codegen:check` exit 0 |
| No legacy names, routes, package names, or compatibility aliases added | PASS | None found |
| **Do not circumvent a gate** | PASS | Tree-wide check of the diff: no `#[allow]` added, no `#[ignore]` added, no test deleted, no `scripts/checks/*` file touched, no boundary glob widened. The one `#[ignore]` mentioned in the task packet refers to two Rust-SDK journeys deleted on an earlier commit, not in this range |
| No surface still describes the wrong header / second identity model | **FAIL** | See **RR-3**, **RR-4** |

### AGENTS.md §15 Implementation Rules

| Rule | Result | Evidence |
|---|---|---|
| `wyrd-spec` is not a dumping ground | PASS | Only error-catalog metadata changed there; the new `EVAL_LEASE_HEADER` correctly stayed out, matching the existing per-crate-private precedent for `x-wyrd-access-token` (`transport/http.rs:57` `HEADER_WYRD_ACCESS_TOKEN` in the client, `token_extract.rs:16` `WYRD_ACCESS_TOKEN_HEADER` in the server) |
| Client-tier crates avoid `sqlx`, cloud SDKs, `datafusion`, `deltalake` | PASS | `check:client-tier` and `check:sdk-client-tier` both exit 0. The new `wyrd-cli` dependency is `wyrd-auth-verify`, dev-only, and `wyrd-cli` is a `crates/wyrd/*` application crate, not client tier |
| Reuse the existing owner before adding code | PASS (largely) | The change's centre of gravity is deletion: nine hand-rolled `reqwest::Client` sites collapse onto `wyrd-client`, and `auth_to_wyrd` moves onto `AuthError::into_wyrd` so both the transport and the platform handle share one projection |
| Fix the root cause once at the shared owner | **FAIL** | See **RR-2** |
| No scaffolding for hypothetical reuse | **FAIL** | See **RR-1**, **RR-8** |

### AGENTS.md §16 General Code Rules

| Rule | Result | Evidence |
|---|---|---|
| Rustdoc on every new item, incl. private, fields, variants, tests | PASS | Audited item by item: `client.rs` (5/5 functions + module doc), `eval/handle.rs` (module, 2 structs, all 7 fields, 2 `Debug` impls, 9 methods, 1 const), `auth.rs::TokenExchange` (struct, 2 fields, 6 methods, `into_wyrd`), `transport/http.rs::request_json_with_headers`, `openapi.rs` (2 consts, `SecurityAddon`, `modify`, the new test incl. `# Panics`), `principal_journey.rs` (module + 6 helpers + 2 tests), `eval/agent.rs` (module, struct, all 3 fields, both DTOs and all 6 of their fields, 3 methods), `token_extract.rs::request_id`, `wyrd-auth-verify::signing_key`, every re-documented `wyrd-cli` dispatch function |
| `# Errors` on every fallible function/method | **FAIL** | See **RR-5** |
| `# Panics` where a panic remains possible | PASS | `openapi.rs:3785` adds one for the new test |
| No comments/docs/annotations added to untouched code | PASS | Every added doc block sits on an item the diff changed |
| Single responsibility | PASS | `AgentClient::post_turn` was split into `post_turn` (timeout) + `exchange` (send/decode) precisely to honour this (`eval/agent.rs:2659-2661`) |
| Follow existing style | PASS | `invalid(...)` helpers, `*_PATH` consts, and `path_with_query` match neighbouring `wyrd-cli` modules |

### AGENTS.md §13 Git Identity

| Rule | Result | Evidence |
|---|---|---|
| Locally configured identity, never signed as anyone else | PASS | All 9 commits in `289978fcc~1..4668d8d33` are author **and** committer `Thorrester <sjforrester32@gmail.com>` |
| No AI co-author trailer | **Contradiction, stated not adjudicated** | Every commit in the range ends `Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>`. AGENTS.md §13 says "Never add AI co-author trailers." The session's own attribution instruction requires exactly that trailer. Both sides are literally in force and they conflict; per my brief I state this rather than rule on it. It is not counted toward the overall verdict, and no remediation is proposed — resolving it is a repository-policy decision, not a code fix |

### agent-rules.md (mandatory boundaries)

| Rule | Result | Evidence |
|---|---|---|
| No raw `sqlx::PgPool`, `TenantConn`/`OperatorPool` only | PASS | No signature in the diff takes a pool; `platform_extractor.rs` keeps `&OperatorPool` |
| RLS is the tenant boundary, no manual tenant filters | PASS | None added |
| Audit records authorization decisions, one write path | PASS | `revoke.rs` audit flow unchanged |
| Cross-tier imports through the owning tier's re-exports | PASS | `wyrd-cli` imports `wyrd_client::{auth,config,error,transport}` and `wyrd_client::eval`, never a transitive crate |
| Bare names in signatures, `use` at top of module | PASS | New `use` blocks are all at module top; `eval/handle.rs:664-673` and `client.rs:2368` use bare `Method`, `WyrdClient`, `WyrdCliError`. `check_lease`'s function-scoped `use subtle::ConstantTimeEq;` (`eval/routes.rs:257`) is pre-existing and falls under the stated trait-scope exception |
| External `tests/` file must earn its place | PASS | `principal_journey.rs` spawns the compiled binary and stands up a real HTTP surface — two of the named qualifying reasons |
| Postgres/live-server tests in `mod pg_tests` or a `pg_*` file | **FAIL** | See **RR-6** |
| Never circumvent a gate; `#[allow(clippy::…)]` needs a justification comment | PASS | No new `#[allow]` anywhere in the diff |
| Never hand-edit generated artifacts (`.pyi`, OpenAPI/JSON schemas, `tests/schemas/` goldens) | PASS | `openapi.yaml` and both `ui_problem_examples.json` files reproduce byte-identically under `mise run codegen:check` (exit 0), so they are regenerator output, not hand edits |
| SSRF-screen server-side fetches of caller-supplied URLs | N/A | The one caller-supplied URL added is `--agent-url`, fetched by the **CLI** process on the operator's own behalf, not by the server |
| `build.rs` writes only to `OUT_DIR` | N/A | No build script changed |
| Never reference plans, tasks, or agents in the codebase | PASS | Checked every new doc comment; `principal_journey.rs:3101` describes the defect behaviourally without naming a task or agent |

## 3. Findings

### RR-1 · VIOLATION · unreachable CLI error variants left in the public catalog

- **Rule:** AGENTS.md §15 ("remove or decline speculative work"; "Do not scaffold for hypothetical reuse"), §12 Completion Standard.
- **Location:** `crates/wyrd/wyrd-cli/src/error.rs:227` (`AuthFailed`), `:257` (`AdminFailed`), `:272` (`IssueKeyFailed`).
- **Observable consequence:** after this change no code anywhere in the tree constructs these three variants. Verified: `grep -rn 'WyrdCliError::\(AuthFailed\|AdminFailed\|IssueKeyFailed\)'` over `crates/`, `docs/`, and `openapi.yaml` returns nothing outside the enum definition itself. They are not caught by `dead_code` because the enum is `pub`, so three `WYRD_CLI_*` codes stay published in the error catalog that no execution path can ever emit. An agent consuming the catalog is told about failure modes that cannot occur; a future contributor reading `error.rs` cannot tell which variants are live. This also contradicts the packet's own acceptance row, which claims unreachable per-command variants were deleted while naming only `HttpBuild`, `Http`, `UrlJoin`, `RevokeFailed`.
- **Correction (testable):** delete the three variants. Confirm with `mise run lints` (exit 0) and `mise run codegen:check` (exit 0 — these codes appear in no generated artifact; `grep -rn 'WYRD_CLI_' docs/src/content/docs/ openapi.yaml crates/wyrd-spec/schemas/` lists only `WYRD_CLI_400_CARD_EXTENSION_UNSUPPORTED` and `WYRD_CLI_500_IO`, so removal is not a generated-contract change).

### RR-2 · REGRESSION · the diff diverged two copies of `tenant_from_unverified_access_token`

- **Rule:** AGENTS.md §15 ("Fix a root cause once at the shared owner instead of patching each symptom"; "prove that the repository does not already provide the needed behavior").
- **Location:** `crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:68` (changed by this diff) versus `crates/wyrd/wyrd-server/src/components/auth/routes.rs:333` (untouched duplicate), with its error constructor at `routes.rs:349`.
- **Observable consequence:** the same function exists twice in the same crate. This diff changed one copy so that a well-formed JWT whose claims are not tenant access-token claims — the concrete case being a platform session, which now travels on the same `X-Wyrd-Access-Token` header — yields `WYRD_AUTH_401_UNAUTHENTICATED` instead of `WYRD_AUTH_400_BAD_TOKEN_FORMAT`. The function's own new rustdoc (`token_extract.rs:64-66`) states the reason: a 400 would "tell a caller which kind of token it holds." Before the diff both copies returned `BadTokenFormat` and were consistent. After it, the copy at `routes.rs:333` — reached by the `/auth/token` delegation exchange at `routes.rs:129` — still answers `400 BadTokenFormat` with the message `"subject_token is not a compact JWT"` for a platform session. So a caller probing `/v1/cards` learns nothing (401), while the same caller probing `/auth/token` with the same token gets a 400 that both misdescribes the token and discloses its kind. The information-disclosure property this change closes on one route is left open on another, and the misleading message is now factually wrong for that input class.
- **Correction (testable):** delete `routes.rs:333-354` and call the `pub(crate)` owner `crate::components::auth::token_extract::tenant_from_unverified_access_token`. Move the three existing unit tests at `routes.rs:392-445` onto `token_extract.rs` (which currently has no `mod tests` at all) and add a fourth asserting that a well-formed JWT with non-tenant claims yields code `WYRD_AUTH_401_UNAUTHENTICATED`. Prove with `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/components::auth::token_extract::tests::/)'`.

### RR-3 · MISSING · agent-facing documentation still instructs `Authorization: Bearer` on `/v1` routes

- **Rule:** AGENTS.md §2 ("Wyrd is agent-first and headless. MCP, CLI, HTTP, generated schemas, stable errors, and machine-readable docs are primary surfaces"), §12 Completion Standard.
- **Location:** `docs/src/content/docs/for-agents/workflow.svx:28` and `:59`.
- **Observable consequence:** the page an agent is pointed at documents the canonical register-a-card workflow as
  `curl -sf "$WYRD_SERVER_URL/v1/cards/default/my-dataset/0.1.0" -H "Authorization: Bearer $WYRD_ACCESS_TOKEN"` and
  `curl -X POST "$WYRD_SERVER_URL/v1/cards" -H "Authorization: Bearer $WYRD_ACCESS_TOKEN" …`. No Wyrd route reads `Authorization` — that is the invariant this whole change establishes and that `platform_extractor.rs::authorization_alone_yields_no_token` now asserts. An agent that follows the documented workflow literally receives `401` on both steps, on the primary surface the repository declares primary. `mise run docs:check` passes because that lane verifies build, links, and a11y contrast, not header correctness, so no gate catches it. These two lines predate this diff, but they fall squarely inside the criterion the packet marks PASS ("No surface still describes a second identity model or the wrong header") while the diff *did* correct the sibling instances in `wyrd-spec/src/error.rs` and `docs/scripts/generate_api_docs.py`.
- **Correction (testable):** change both headers to `X-Wyrd-Access-Token: Bearer $WYRD_ACCESS_TOKEN`. Prove with `grep -rn 'Authorization: Bearer' docs/src/content/` returning no `/v1`-route example, plus `mise run docs:check`.

### RR-4 · MISSING · the served `BadTokenFormat` detail names the wrong header

- **Rule:** AGENTS.md §12 Completion Standard; §9 (public errors describe the actual contract); references `languages/errors.md` §HTTP Errors ("Use actionable messages").
- **Location:** `crates/wyrd/wyrd-server/src/http/error.rs:202`, `crates/wyrd/wyrd-auth/src/error.rs:35` — both `"authorization header malformed"`.
- **Observable consequence:** this diff changed the catalog entry for the same code so that its `title` is now `"Access token header malformed"` and its `remediation` is `"Use \`X-Wyrd-Access-Token: Bearer <token>\` …"` (`wyrd-spec/src/error.rs:550-551`). The `detail` field of the very same problem response still says `authorization header malformed`. A caller who sends a malformed `X-Wyrd-Access-Token` receives one payload whose `title`/`remediation` point at the right header and whose `detail` points at a header Wyrd never reads, and is sent to inspect the wrong thing. The catalog is internally inconsistent as shipped.
- **Correction (testable):** change both message strings to name `X-Wyrd-Access-Token`. Prove with `grep -rni 'authorization header' crates/` returning nothing outside tests that deliberately assert the header is *not* read, plus `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=error::tests::auth_variant_problem_json_roundtrips)'`.

### RR-5 · VIOLATION · missing rustdoc and `# Errors` on materially modified fallible functions

- **Rule:** AGENTS.md §16 ("Every new or materially modified Rust item MUST have rustdoc … Every fallible Rust function or method MUST include a `# Errors` section"; "Missing or placeholder rustdoc on any touched Rust item is a hard blocker"); agent-rules.md ("Missing or placeholder rustdoc is `BLOCK_BEFORE_MERGE`").
- **Location:**
  - `crates/wyrd/wyrd-cli/src/auth/login.rs:78` — `parse_callback_input` has **no rustdoc at all** and no `# Errors`, while this diff rewrote its failure branch from `WyrdCliError::AuthFailed` to `WyrdCliError::InvalidArgument` (`login.rs:99-103`).
  - `crates/wyrd/wyrd-server/src/components/eval/routes.rs:255` — `check_lease` is fallible, carries a one-line doc and no `# Errors`, while this diff changed the header it reads from `Authorization` to `EVAL_LEASE_HEADER` (`:259`) — that is, it changed the precise condition under which the function fails.
- **Observable consequence:** the two functions whose error behaviour this change altered are the two that do not document it. `parse_callback_input` is the function an operator hits when they paste a callback URL wrongly, and a maintainer reading it cannot tell from the signature which of the CLI's ~20 error variants it produces. `check_lease` is a security check whose three distinct refusal paths (absent header, no scheme separator, wrong scheme or empty token) are undocumented. Every sibling touched in this diff — `client.rs`, `eval/handle.rs`, `TokenExchange`, the seven re-documented `wyrd-cli` dispatch functions — did receive `# Errors`, so this is an omission against the diff's own standard, and §16 states it as a hard blocker rather than a style note.
- **Correction (testable):** add rustdoc with intent, workflow role, and an `# Errors` section to both. Prove by inspection plus `mise run lints` (clippy `missing_errors_doc` is only a public-API lint, so it does not substitute for the review check — the check is the rule itself).

### RR-6 · VIOLATION · live-server journey tests report a hollow PASS in every lane but one

- **Rule:** agent-rules.md ("Tests needing Postgres, Docker, or a live server go in `mod pg_tests` (or a `pg_*` file), never the fast lane"); AGENTS.md §11 (journeys "run in a gated lane … so the fast lane stays credential- and server-free").
- **Location:** `crates/wyrd/wyrd-cli/tests/principal_journey.rs:133` (`#[tokio::test] async fn principal_revoke_cli_journey`, gate at `:137`) and `:192` (`…_refuses_an_unprivileged_caller`, gate at `:196`). Both are file-top-level, neither inside `mod pg_tests` nor marked `#[ignore]`.
- **Observable consequence:** the family lane is `cargo nextest run --locked -p <pkgs>` (`scripts/run-family-tests.sh:21`) with no `--skip pg_tests` and no default filter in `.config/nextest.toml`, and it does not set `WYRD_CLI_E2E`. The two tests are therefore *selected and executed* there, hit `if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") { return; }`, and report success having asserted nothing. Measured directly:

  ```
  $ env -u WYRD_CLI_E2E mise exec -- cargo nextest run --locked -p wyrd-cli \
      --test cli -E 'test(/principal_journey::/)'
      Starting 2 tests across 1 binary (25 tests skipped)
          PASS [   0.015s] (1/2) wyrd-cli::cli principal_journey::principal_revoke_cli_journey_refuses_an_unprivileged_caller
          PASS [   0.015s] (2/2) wyrd-cli::cli principal_journey::principal_revoke_cli_journey
       Summary [   0.015s] 2 tests run: 2 passed, 25 skipped
  ```

  0.015s for a test that is supposed to boot a server, bootstrap two principals, spawn a subprocess, and revoke. Both sibling patterns in the very same test binary avoid this: `card_lifecycle.rs:554` puts its `WYRD_CLI_E2E`-gated journeys inside `mod pg_tests`, and `query_server_journey.rs:71` marks its journeys `#[ignore = "requires the serialized Postgres-backed CLI journey lane"]` so the runner reports them as *ignored* rather than passed. The new file adopts neither, so the repository's primary-tier evidence for this capability reads green in lanes that never exercised it — indistinguishable in a CI summary from a lane that did.
- **Correction (testable):** wrap both tests in `mod pg_tests { … }` (matching `card_lifecycle.rs:554`, the nearest precedent in the same binary) or add `#[ignore = "requires the CLI journey lane"]` (matching `query_server_journey.rs:71`). Prove both directions: `env -u WYRD_CLI_E2E mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(/principal_journey::/)'` must report the tests as ignored/not-run rather than passed, and `mise run test:cli:journey` must still show both `... ok`.

### RR-7 · DRIFT · the generated public contract carries Rust intra-doc links

- **Rule:** AGENTS.md §9 ("Versioned API contracts are explicit"), §2 (generated schemas and machine-readable docs are primary agent surfaces).
- **Location:** `openapi.yaml:1186-1187`, from the rustdoc at `crates/wyrd/wyrd-server/src/auth/revoke.rs:24-28`.
- **Observable consequence:** the newly published operation's `description` contains
  ``Returns [`WyrdError::PermissionDeniedRbac`] when the caller lacks `service_accounts:write`, [`WyrdError::AuditUnavailable`] when …``.
  These are Rust intra-doc link references; they resolve to nothing in a YAML contract, a rendered API reference, or a generated client. `grep -c 'WyrdError::' openapi.yaml` returns exactly **2**, both introduced here — this is the first and only such leak in the document, so it is drift rather than an established pattern. (The bare `# Errors` heading is a separate matter: `grep -c '# Errors' openapi.yaml` returns 25, so that part is pre-existing repository style and out of scope.) An agent reading the contract to decide how to handle a refusal is handed a broken symbol reference instead of the `WYRD_PERMISSION_403_DENIED_RBAC` code it can actually match on.
- **Correction (testable):** give the `#[utoipa::path]` attribute an explicit `description = "…"` that names the stable codes in plain text, so the rustdoc keeps its Rust links and the contract carries contract prose. Prove with `grep -c 'WyrdError::' openapi.yaml` returning `0` and `mise run codegen:check` exit 0.

### RR-8 · VIOLATION · header constant exported beyond its only consumer

- **Rule:** AGENTS.md §15 ("Do not scaffold for hypothetical reuse or future requirements").
- **Location:** `crates/wyrd/wyrd-server/src/components/eval/routes.rs:252` — `pub(crate) const EVAL_LEASE_HEADER`.
- **Observable consequence:** `grep -rn 'EVAL_LEASE_HEADER' crates/wyrd/wyrd-server/src/` returns only the definition and its single use at `:259`, both inside `routes.rs`. The crate-wide visibility has no consumer and will not be flagged by `dead_code`. The server's precedent for the analogous constant is private (`transport/http.rs:57` `const HEADER_WYRD_ACCESS_TOKEN`), and `token_extract.rs:16` is `pub(crate)` only because two extractor modules genuinely read it. Lowest-confidence finding in this report: the consequence is a slightly wider surface than earned, not a behaviour defect.
- **Correction (testable):** drop `pub(crate)`. Prove with `mise run lints` exit 0.

## 4. Verification actually executed

Narrow lanes and focused expressions only; no `gate`, no `test:rust`, no whole-family lane, no `--all-features` workspace test lane.

| Command | Result |
|---|---|
| `mise run fmt` | exit 0 |
| `mise run lints` | exit 0 (one unrelated `[profile]` warning from `sdks/wyrd-sdk-python/Cargo.toml`) |
| `mise run check:client-tier` | exit 0 |
| `mise run check:sdk-client-tier` | exit 0 |
| `mise run check:pyo3-scope` | exit 0 |
| `mise run check:unwrap-audit` | exit 0 |
| `mise run codegen:check` | exit 0, "All checks passed!", no stub or schema drift |
| `mise run docs:check` | exit 0, 60 pages, contrast AA light+dark |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/components::auth::platform_extractor::tests::/) + test(=http::openapi::tests::every_authenticated_path_declares_the_one_wyrd_scheme)'` | 7 tests run, 7 passed |
| `mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib` | 41 tests run, 41 passed |
| `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=error::tests::auth_variant_problem_json_roundtrips)'` | 1 passed |
| `mise run test:cli:journey` | exit 0; `principal_journey::principal_revoke_cli_journey ... ok`, `..._refuses_an_unprivileged_caller ... ok`; `22 passed; 0 failed; 5 ignored` |
| `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test platform_admin_e2e'` | exit 0; `17 tests run: 17 passed, 0 skipped` |
| `env -u WYRD_CLI_E2E mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(/principal_journey::/)'` | 2 passed in 0.015s having asserted nothing — evidence for **RR-6** |
| `rtk proxy git log --format='%H\|%an <%ae>\|%cn <%ce>' 289978fcc~1..4668d8d33` | all 9 commits `Thorrester <sjforrester32@gmail.com>` for author and committer |
| `rtk proxy git status --porcelain` | empty before and after this review; no source file changed |

## 5. Overall result

**FAIL**

Authority coverage is complete — every changed surface is mapped to its governing
sections, every applicable reference slice was selected through the router and
read, and nothing is blocked for want of an authority. The change is
substantively good work whose centre of gravity is deletion: nine hand-rolled
HTTP clients collapse onto `wyrd-client`, the struct-centered style is followed
closely enough that the new `wyrd-client/src/eval/` module is a faithful sibling
of the canonical `Cards` handle, and every boundary and codegen gate passes.

It fails on eight findings, of which four are decisive:

- **RR-5** is a stated hard blocker in both AGENTS.md §16 and agent-rules.md
  (`BLOCK_BEFORE_MERGE`), on the two functions whose error behaviour this diff
  changed.
- **RR-6** means the primary-tier evidence for this capability reports success in
  lanes that never ran it, diverging from both sibling patterns in the same test
  binary.
- **RR-2** leaves the information-disclosure property the change deliberately
  closes on `/v1` still open on `/auth/token`, because a duplicated function was
  changed in one copy only.
- **RR-3** leaves the repository's own agent-facing workflow page instructing a
  header no Wyrd route reads, on the surface §2 declares primary.

RR-1, RR-4, RR-7, and RR-8 are smaller but concrete and each has a one-line
correction. The §13 co-author-trailer conflict is recorded in §2 as a stated
contradiction and is excluded from this verdict.
