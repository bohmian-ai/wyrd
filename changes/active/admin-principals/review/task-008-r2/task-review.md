# TASK-008 task acceptance review — `task-008-r2`

| Item | Value |
|---|---|
| Task | `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md` |
| Spec | `changes/active/admin-principals/spec.md`, revision 7 (approved) |
| Branch | `claude/admin-principals-spec-qfsmjc` |
| Candidate HEAD | `4668d8d333ad043b4b2f8c258beb78eebb719466` |
| Closeout range | `289978fcc~1..4668d8d33` |
| Reviewer | `task-rev` (independent; implemented nothing; changed no source) |
| Working tree | clean at review start and at review end (`git status --porcelain` empty) |

**Overall result: FAIL** — two acceptance criteria are falsified by the tree
(`TR-1`, `TR-2`) and two surfaces still name a header no Wyrd route reads
(`TR-3`, `TR-4`). Everything else in the task is delivered, and the
verification the implementor claims reproduces.

---

## 1. Acceptance matrix

### 1.1 Acceptance criteria stated by the task

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC — No Wyrd-owned surface reads the `Authorization` header; a tree-wide search returns nothing | `platform_extractor.rs:13,68` binds `token_extract::WYRD_ACCESS_TOKEN_HEADER` (was `HeaderName::from_static("authorization")`, `289978fcc`); `components/eval/routes.rs:251,258` moves the run lease to `x-wyrd-eval-lease`; `platform/handle.rs` deletes the `reqwest::header::AUTHORIZATION` dispatch. Tree-wide grep over `crates/`+`sdks/` leaves only: `skald-providers/src/auth/{openai.rs:80,google_oauth.rs:354}` (outbound to OpenAI/Google), `vala-core/src/alert_router/webhook.rs:176,568` (operator-webhook header denylist), `skald-agent/src/loop_runtime.rs:893` and `skald-observer/src/redaction.rs:34` (redaction field lists), `wyrd-mcp/src/client.rs:438` (asserts the header is *not* written), and the two new negative unit tests. **But** `crates/wyrd/wyrd-server/src/http/error.rs:202` still *names* it in a served error detail (`TR-4`) and `docs/src/content/docs/for-agents/workflow.svx:28,59,81` still *documents* it (`TR-3`) | `cargo nextest run -p wyrd-server --lib -E 'test(/components::auth::platform_extractor::tests::/)'` → 6/6 PASS (this reviewer) | **FAIL** (see `TR-3`, `TR-4`; the *reads* half passes, the *no surface names it* half does not) |
| AC — A platform session token authenticates on `X-Wyrd-Access-Token`; a tenant access token presented to a platform route is still refused with the stable contract error, proving the scope marker and not the header separates them | `platform_extractor.rs:54-61` reads the canonical header; `verify_platform`'s `PLATFORM_TOKEN_SCOPE` check unchanged (`wyrd-auth-verify/src/lib.rs:445-448`); `platform_admin_e2e.rs:204,215` now presents the tenant JWT **on the canonical header** to `/platform/tenants` and asserts `401` plus an identical `code` to the anonymous request — the header is no longer doing the separating | `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- … cargo nextest run -p wyrd-server --test platform_admin_e2e` → 17/17 PASS, incl. `the_two_control_planes_cannot_reach_each_other` (this reviewer) | PASS |
| AC — An application's own `Authorization` header is carried through untouched and never consulted | `platform_extractor.rs:237-271` adds `an_applications_own_authorization_header_is_never_read` and `authorization_alone_yields_no_token`; the extractor never reads `authorization` | Both tests PASS in the focused run above | PASS |
| AC — Signing-key resolution exists once and both internal verify paths call it | one private `TokenVerifier::signing_key` at `crates/shared/wyrd-auth-verify/src/lib.rs:394-424`; called at `:444` (`verify_platform`) and `:512` (cached tenant path). `verify_external_against` (`:687-706`) left alone — it resolves `OidcKid` against a tenant JWKS | `cargo nextest run -p wyrd-auth-verify --lib` → 41/41 PASS (this reviewer) | PASS |
| AC — No `reqwest::Client` is constructed anywhere in `crates/wyrd/wyrd-cli/src` | single assembly point `crates/wyrd/wyrd-cli/src/client.rs:53-62` over `wyrd-client`; residual `reqwest` use in `wyrd-cli/src` is `Method` only (`auth/issue_key.rs:4`, `auth/workload_binding.rs:4`, `auth/trusted_issuer.rs:6`) plus `reqwest::Method`/`reqwest::Body` at `eval/agent.rs:123,125` | `grep -rn "reqwest::Client" crates/wyrd/wyrd-cli/src/` → no matches; `mise run check:client-tier` exit 0 (this reviewer) | PASS |
| AC — Every CLI command that calls a Wyrd route authenticates on the canonical header and is proven to do so by a test that would have caught the original defect | `principal/revoke.rs:60`, `auth/issue_key.rs:64`, `auth/trusted_issuer.rs:116,168,194`, `auth/workload_binding.rs`, `card.rs`, `query/mod.rs`, `eval/server.rs:39` all route through `crate::client::client(...)`; `auth/{login,refresh}.rs` fold onto `wyrd_client::auth::TokenExchange`. `crates/wyrd/wyrd-cli/tests/principal_journey.rs:128` drives the shipped binary over a real socket and asserts the revoked token stops working — it fails on an unauthenticated refusal, which is exactly the original defect | `WYRD_CLI_E2E=1 mise run test:cli:journey` → 22 passed, 0 failed, incl. `principal_journey::principal_revoke_cli_journey` (this reviewer) | PASS |
| AC — `eval/*` is moved onto the shared client or removed, with the reason recorded; it is not left hand-rolled | `eval/server.rs` now drives `wyrd_client::eval::{EvalProtocol, EvalRun}` (`crates/shared/wyrd-client/src/eval/handle.rs`), which owns the `x-wyrd-eval-lease` header behind the new `pub(crate)` `HttpTransport::request_json_with_headers`; `eval/agent.rs:1-8` records that the agent endpoint is a third-party URL and uses the **pre-existing** credential-free `WyrdClient::request_external_stream` (`transport/http.rs:287-305`, which sends no `x-wyrd-access-token` and no `wyrd-request-id`). Ladder check: the capability is the one the task's REFACTOR explicitly authorizes, the external seam is reused rather than added, and the header helper is crate-private so no consumer composes headers | `test:cli:journey` → `eval_server_protocol::server_protocol_carries_lease_after_open` PASS, asserting every post-open call carries `Bearer cli-lease-token` on `x-wyrd-eval-lease` | PASS |
| AC — Unreachable per-command CLI error variants are deleted, not left in place | `HttpBuild`, `Http`, `UrlJoin` removed. **`RevokeFailed` (`crates/wyrd/wyrd-cli/src/error.rs:242`) was not**, nor were `AuthFailed` (`:227`), `AdminFailed` (`:257`), `IssueKeyFailed` (`:272`) — all four lost their last constructor in `3a71e0409` | `grep -rn` for each variant across `crates/` returns only the declaration in `error.rs`; `cargo clippy -p wyrd-cli --all-targets` exit 0, because a `pub` enum variant is not dead code to clippy — the implementor's "`mise run lints` (dead-code clean)" evidence cannot detect this | **FAIL** (`TR-1`) |
| AC — The generated contract declares the authentication scheme and regenerates cleanly with no hand edits | `SecurityAddon` modifier at `crates/wyrd/wyrd-server/src/http/openapi.rs:26-77` registers `wyrdAccessToken` (`apiKey`/`header`/`X-Wyrd-Access-Token`), requires it document-wide, and clears it on the three `ANONYMOUS_PATHS`; `openapi.yaml:4532-4540` carries the scheme and `security: []` on `/auth/platform/{token,login,callback}`. Eval routes are not in the document, so the stop condition ("a route whose real auth requirement contradicts INV-015") is not triggered | `mise run codegen:check` exit 0 with a clean tree afterwards (this reviewer); `http::openapi::tests::every_authenticated_path_declares_the_one_wyrd_scheme` PASS | PASS |
| AC — The CLI performs the administrative operations against a real server, **including receiving a once-returned credential and using it on a subsequent call**, through a lane that actually runs | Revocation half: `principal_journey.rs`, registered at `crates/wyrd/wyrd-cli/tests/cli.rs:11`, run by `mise.toml:141-150` (`test:cli:journey`, which sets `WYRD_CLI_E2E=1`). Once-returned-credential half: **no such test exists.** `crates/wyrd/wyrd-cli/tests/` contains no `auth` journey; `auth issue-key`'s only tests are clap-parsing (`auth/issue_key.rs:87-160`); `grep -rn 'cargo_bin("wyrd")'` finds no invocation of `auth issue-key`, `auth login`, or `auth refresh` anywhere | `test:cli:journey` 22 passed — none of them issues a credential through the CLI and re-presents it. The evidence row's "the pre-existing `auth` journeys" do not exist | **FAIL** (`TR-2`) |
| AC — A tenant-scope caller cannot invoke a platform-plane operation through the CLI or an MCP tool; the refusal is the stable contract error | Structural: `crates/wyrd/wyrd-server/src/mcp/` exposes `bifrost`, `principals`, `probe` only — no platform tool; `wyrd-cli/src/cli.rs` declares no platform command. HTTP side proven | `platform_admin_e2e::the_two_control_planes_cannot_reach_each_other` PASS; `principal_journey::principal_revoke_cli_journey_refuses_an_unprivileged_caller` PASS, asserting `WYRD_PERMISSION_403_DENIED_RBAC` on stderr | PASS |
| AC — No Python or TypeScript administrative binding exists, and no unrun administrative journey remains in the tree | `git diff --stat 968c92641..4668d8d33 -- sdks/` touches only `sdks/wyrd-sdk-rust/src/lib.rs` (+5/−2); `sdks/wyrd-sdk-rust/tests/` holds `cards_state.rs` alone — `principals.rs` and `platform.rs` stay deleted; no administrative symbol in `sdks/wyrd-sdk-python/python/wyrd/__init__.pyi` or `sdks/wyrd-sdk-ts/src` | `mise run check:sdk-client-tier` exit 0; `mise run codegen:check` exit 0 with no stub drift (this reviewer) | PASS |
| AC — MCP administrative write tools are unavailable without their explicit scope and available with it; read tools remain available | Unchanged from `128eb40`; `mcp/mod.rs:136,162,181-183` keeps `descriptors_unscoped()` always and `write_descriptors()` behind `principals::may_administer(&caller)` | `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs:35` (`a_write_tool_is_scoped_at_dispatch_not_merely_hidden`) is `#[ignore]`d but selected by `mise.toml:368` (`test:bifrost:journey:mcp:inner`, `--run-ignored=all`), so the lane runs it. Not re-executed here (prior-landed, outside this closeout) | PASS |
| AC — Platform revocation behavior is unchanged and its rustdoc states the caches-nothing invariant rather than the stale no-tokens premise | `crates/wyrd/wyrd-auth/src/platform_credentials.rs:429-439` replaces "the platform plane issues no access tokens" with the caches-nothing statement and names why the tenant plane needs an epoch and this one does not. No mechanism changed in the diff | `platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions` PASS (behavior unchanged) | PASS |
| AC — No document, example, or surface still describes a second identity model, a removed bootstrap path, or credential-keyed authorization | `bootstrap-key`/`SYSTEM_OPERATOR_ID`/`bootstrap-admin` absent from `docs/src`, `crates`, `architecture` (only an unrelated `audit.rs:194` string literal). Error-catalog remediations for `WYRD_AUTH_401_UNAUTHENTICATED`, `WYRD_AUTH_400_BAD_TOKEN_FORMAT`, `WYRD_PERMISSION_401_UNAUTHENTICATED`, `WYRD_EVAL_401_MISSING_LEASE` now name the right headers (`crates/wyrd-spec/src/error.rs:507,546,885,3118`). Second identity model: gone. **Wrong header still described** at `docs/src/content/docs/for-agents/workflow.svx:28,59,81` and `wyrd-server/src/http/error.rs:202` | `mise run codegen:check` exit 0; `mise run docs:check` exit 0 — neither lane inspects header prose in `.svx` bodies | **FAIL** (`TR-3`, `TR-4`) |

### 1.2 Spec obligations the task carries

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-036` — administrative operations headless over the contract; generated contract MUST declare the authentication scheme its administrative paths require | `openapi.rs:26-77` + `openapi.yaml:4532-4540`; `crates/wyrd/wyrd-server/src/auth/revoke.rs:29-41` publishes `POST /v1/principals/{principal_id}/revoke`, so the route the CLI drives is now in the artifact; signing-key resolution consolidated | `codegen:check` exit 0; `every_authenticated_path_declares_the_one_wyrd_scheme` PASS | PASS |
| `REQ-040` — documentation covers the operator journey, the SaaS model, rotation, and credential-loss recovery | Not advanced by this closeout, and the agent-facing workflow page still shows a header that cannot authenticate | `docs:check` exit 0 (it does not validate header prose) | **FAIL** (`TR-3`) |
| `REQ-047` — every Wyrd-owned surface calling a Wyrd HTTP route does so through `crates/shared/wyrd-client`; no surface assembles its own auth header or maps its own status codes | All CLI commands routed (see AC row above); per-command status→error mapping deleted in favour of `WyrdCliError::Server { source: WyrdError }` (`error.rs:335-345`); the lease header lives in `wyrd-client`, not in `wyrd-cli`; `Platform` now runs on `HttpTransport` | `check:client-tier` exit 0; `cargo clippy -p wyrd-server -p wyrd-auth-verify -p wyrd-client -p wyrd-cli -p wyrd-mcp --all-targets` exit 0 (this reviewer) | PASS |
| `INV-013` — revocation effective no later than the next request, by each plane's own mechanism | Mechanism untouched; rustdoc corrected (`platform_credentials.rs:429-439`); `principal_journey.rs:66-72` pins `cache_ttl: Duration::ZERO` so the CLI journey observes the tenant plane's epoch | `revoking_a_platform_credential_ends_its_live_sessions` PASS; `principal_revoke_cli_journey` PASS | PASS |
| `INV-014` — administrative identity stays server-owned; no SDK/CLI/UI becomes a source of truth | The CLI holds no durable state; `client.rs` only assembles transport; no token is persisted (`ClientConfig` default token cache is `InMemory`, `config.rs:214`, and `AuthMiddleware::persist` returns early for any non-`Disk` mode, `auth.rs:562-565`; a `BearerToken` credential performs no exchange at all) | `check:client-tier`, `check:sdk-client-tier` exit 0 | PASS |
| `INV-015` — every plane authenticates on `X-Wyrd-Access-Token`; the caller's own `Authorization` is never read; separation by claims and extractor type, not header | Platform extractor rebound; eval lease moved off `Authorization`; `token_extract::tenant_from_unverified_access_token` (`token_extract.rs:75-80`) now returns `Unauthenticated` rather than `BadTokenFormat` for a well-formed JWT with non-tenant claims, which is what lets a platform session on `/v1/cards` be refused as `401` now that both planes share the header (`343cda711`); module docs on `platform_extractor.rs:1-19` and `wyrd-client/src/platform/mod.rs:5-19` replaced with the scope-marker/extractor rationale | 6/6 platform extractor unit tests PASS; `the_two_control_planes_cannot_reach_each_other` PASS in both directions | PASS (code) / see `TR-3`, `TR-4` for the doc-and-message residue |
| `AC-013` — one administrative identity model across HTTP, CLI, SDK, MCP, schemas, errors, docs; `bootstrap-key` and its fabricated identities gone; no second identity model in the schema | No `bootstrap-key` residue; one client implementation; one header | `codegen:check`, `docs:check` exit 0 | PASS |
| `AC-014` — contract evidence proves the administrative HTTP surface is implementable by an independent client: paths, typed bodies, stable error codes, **and the authentication scheme**; the CLI and MCP exercise those operations against a real server | Scheme published; `/v1/principals/*` and `/platform/*` paths published (`docs/src/content/docs/api/openapi.md:15-36` regenerated, `074a40e87`); CLI proven against a real server for revocation; MCP proven by its own journey lane | `codegen:check`; `test:cli:journey`; `platform_admin_e2e` | PASS for the contract half; the CLI half is incomplete only on the once-returned-credential flow (`TR-2`) |
| `VER-001..VER-006` — narrow verification scope; every named test run through its exact focused expression under the managed Postgres wrapper; no broad aggregates required | Reproduced with focused selectors only: two `cargo nextest run … -E 'test(=…)'`/`test(/…/)` expressions, one `--lib` package run, one `--test platform_admin_e2e` under `scripts/postgres/with-test-postgres.sh`, plus `check:client-tier`, `check:sdk-client-tier`, `codegen:check`, `docs:check`, and a five-package `cargo clippy`. No `gate`, no `test:rust`, no family lane, no `--all-features` workspace lane. Selectors confirmed with `cargo nextest list` before running | all exit 0 / all PASS | PASS |

### 1.3 Prohibited changes and non-goals

| Prohibition / non-goal | Evidence | Result |
|---|---|---|
| No Python or TypeScript administrative binding (no PyO3, napi, `.pyi`, `.d.ts`) | `git diff --stat 968c92641..4668d8d33 -- sdks/` → only `sdks/wyrd-sdk-rust/src/lib.rs`; `codegen:check` reports no stub drift; no Python or TypeScript file in the closeout diff | PASS |
| Do not reinstate the deleted Rust SDK administrative journeys; do not narrow the Rust SDK's `wyrd-client` re-export | `sdks/wyrd-sdk-rust/tests/` holds only `cards_state.rs`; `sdks/wyrd-sdk-rust/src/lib.rs` untouched by this closeout | PASS |
| Do not route the platform plane through `AuthenticatedPrincipal` | `platform_extractor.rs:107-160` still builds `PlatformPrincipal`/`AuthContext` directly; `AuthenticatedPrincipal` appears only in `caller_extractor.rs:63`; two extractors, two stores, shared plumbing only (`token_extract::request_id`) | PASS |
| Do not add a revocation epoch to the platform plane | No epoch introduced anywhere in the closeout diff; `platform_credentials.rs` change is rustdoc only | PASS |
| Do not weaken the platform plane's indistinguishable rejection | `extract_platform_token` still returns `Option` and every failure renders through the single `unauthenticated()` (`platform_extractor.rs:63-75`); the rustdoc records why `extract_wyrd_access_token`'s informative form was deliberately *not* reused; `the_two_control_planes_cannot_reach_each_other` asserts the tenant-token and anonymous refusals share a `code` | PASS |
| Do not hand-edit generated stubs, schemas, or `openapi.yaml` | Scheme added via the `SecurityAddon` modifier in the generator; route list via `generate_api_docs.py` + regeneration; `crates/wyrd-spec/schemas/ui_problem_examples.json` and its test twin follow from the catalog derive. `mise run codegen:check` exit 0 and `git status --porcelain` empty afterwards — regeneration reproduces every committed artifact byte-for-byte | PASS |
| Do not add a compatibility route, alias, or transitional flag | No alias route, no dual-header fallback, no flag in the diff; the old header is removed outright | PASS |
| Non-goals: no UI; no platform operation exposed to tenant-scope clients; no widening beyond the named callers | No UI file in the diff; no platform tool or CLI command; the write set stays within the task's declared paths plus `auth/revoke.rs` (contract publication), `components/eval/routes.rs` + `token_extract.rs` (both forced by "no surface reads `Authorization`"), and `wyrd-auth-issue` tests (branch-local build repair) | PASS |
| New dependency earned? | `crates/wyrd/wyrd-cli/Cargo.toml:69` adds `wyrd-auth-verify` under `[dev-dependencies]` only (one `Cargo.lock` line, a workspace-internal crate, no new third-party code). Used by `principal_journey.rs:17,68` for `WyrdAuthVerifySettings { cache_ttl: Duration::ZERO }` — without it the in-process server's verified-token cache outlives the revocation and the journey cannot observe it. No lower rung available | PASS |
| `fac7888dc` "incidental repair" claim | `git show --stat 3699b3c37` confirms that commit changed `issue_platform_access_token`'s `credential_id` to `Option<Uuid>` and left `wyrd-auth-issue`'s own unit tests on the old signature, so the crate's test target did not compile. `fac7888dc` changes exactly two call sites and one assertion (`Some(credential)` / `Some(credential.to_string())`). Minimal, correct, branch-local | PASS |
| Deferral of `POST /v1/principals/{principal_id}/revoke` ignoring `RevokePrincipalRequest` | Legitimate: the published `utoipa::path` declares **no** `requestBody`, and `openapi.yaml:1174-1206` carries none, so the artifact does not promise a body the server ignores and an independent client implementing from the artifact alone succeeds. `AC-014`'s implementability claim therefore holds. The journey test does pass while the body is inert, but no acceptance criterion in this task depends on the body being honoured | PASS (deferral does not falsify a criterion) |

---

## 2. Proposed findings

### TR-1 — MISSING

- **Obligation.** Task acceptance criterion: "Unreachable per-command CLI error
  variants are deleted, not left in place." Task REFACTOR (Scenario 3): "delete
  … the per-command error variants that become unreachable,
  `WyrdCliError::RevokeFailed` among them."
- **Location.** `crates/wyrd/wyrd-cli/src/error.rs:227` (`AuthFailed`), `:242`
  (`RevokeFailed`), `:257` (`AdminFailed`), `:272` (`IssueKeyFailed`).
- **Evidence.** `3a71e0409` removed every constructor of all four: `login.rs`
  and `refresh.rs` now raise `WyrdCliError::Server`, `trusted_issuer.rs` and
  `workload_binding.rs` raise `InvalidArgument`/`Io`/`Server`, `issue_key.rs`
  raises `Server`. `grep -rn` for each identifier across `crates/` returns only
  the declaration in `error.rs` — no constructor, no test, no match in
  `sdks/`. The task's closeout evidence table asserts "`HttpBuild`, `Http`,
  `UrlJoin`, `RevokeFailed` removed from `crates/wyrd/wyrd-cli/src/error.rs`";
  the first three were, `RevokeFailed` was not, and the three siblings were
  never enumerated. The cited verification (`mise run lints`, "dead-code …
  clean") cannot detect this: these are `pub` variants of a `pub` enum, so
  neither `dead_code` nor clippy fires — `cargo clippy -p wyrd-cli
  --all-targets` exits 0 with all four present.
- **Observable consequence.** `WyrdCliError`'s derived catalog advertises four
  stable CLI error codes — `WYRD_CLI_401_AUTH_FAILED`,
  `WYRD_CLI_500_REVOKE_FAILED`, `WYRD_CLI_500_ADMIN_FAILED`,
  `WYRD_CLI_500_ISSUE_KEY_FAILED` — that no code path can produce, each
  carrying a status-shaped `status: u16` field and remediation text that
  contradicts the task's own "never re-mapped from a status code" rule now
  recorded on `WyrdCliError::Server` (`error.rs:335-340`). An operator or agent
  reading the catalog cannot tell which codes are reachable, and the dead
  variants are a standing invitation for a future command to re-introduce
  per-command status mapping.
- **Required testable correction.** Delete all four variants and their
  `#[wyrd_error]` attributes from `crates/wyrd/wyrd-cli/src/error.rs`. The
  compiler is the test: with no constructors, removal must compile clean under
  `cargo clippy --locked -p wyrd-cli --all-targets`; if any removal breaks the
  build, that variant was reachable and stays. Re-run
  `mise run codegen:check` to confirm no generated catalog artifact referenced
  the removed codes.

### TR-2 — MISSING

- **Obligation.** Task acceptance criterion: "The CLI performs the
  administrative operations against a real server, **including receiving a
  once-returned credential and using it on a subsequent call**, through a lane
  that actually runs." Task invariant: "Credential plaintext crosses a client
  surface exactly once, in the response that created it." Spec `AC-014`: "The
  CLI and MCP exercise those operations against a real server."
- **Location.** `crates/wyrd/wyrd-cli/tests/cli.rs:1-13` (the lane's whole
  module list) and `crates/wyrd/wyrd-cli/src/auth/issue_key.rs:87-160` (the
  command's only tests).
- **Evidence.** `crates/wyrd/wyrd-cli/tests/` contains `card_lifecycle.rs`,
  `eval_local_records.rs`, `eval_server_protocol.rs`, `loader.rs`,
  `principal_journey.rs`, `query_server_journey.rs` — no `auth` journey.
  `grep -rn 'cargo_bin("wyrd")'` across `crates/` and `sdks/` finds no
  invocation of `auth issue-key`, `auth login`, or `auth refresh`. `issue_key.rs`
  carries three clap-parsing tests only; none performs HTTP. The closeout
  evidence row cites "`principal_journey.rs` (revoke) and the pre-existing
  `auth` journeys through the same client" — the second half names tests that do
  not exist. The `test:cli:journey` run reproduced here lists all 22 tests and
  none issues a credential and re-presents it.
- **Observable consequence.** The one flow in which the CLI handles credential
  plaintext — `auth issue-key` printing a key that is never recoverable — has no
  end-to-end coverage. A regression in the single-print path (a response field
  renamed, the key elided from the printed block, the issued key rejected on its
  next use) ships undetected, and the defect class the task exists to close (a
  CLI transport path with no test) survives on the exact command the task itself
  names as having carried the original header defect.
- **Required testable correction.** Add one real-server CLI test in the
  `test:cli:journey` lane, following `principal_journey.rs`'s shape, that runs
  `wyrd auth issue-key` against a bootstrapped tenant, captures the printed
  `key:` from stdout, exchanges it for an access token, and asserts that token
  reaches an authenticated `/v1` route. It must fail if the key is absent from
  stdout or unusable. Register it in `crates/wyrd/wyrd-cli/tests/cli.rs` and
  verify with `WYRD_CLI_E2E=1 mise run test:cli:journey` plus the focused
  `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E
  'test(=<module>::<name>)'`.

### TR-3 — INCORRECT

- **Obligation.** `INV-015` ("The caller's own `Authorization` header belongs to
  the calling application and is never read by any Wyrd surface"); `REQ-040`
  (documentation obligation, a declared obligation of this task); task
  acceptance criterion "No document, example, or surface still describes a
  second identity model, a removed bootstrap path, or credential-keyed
  authorization", whose evidence row the implementor widened to "or the wrong
  header".
- **Location.** `docs/src/content/docs/for-agents/workflow.svx:28`, `:59`,
  `:81`.
- **Evidence.** The agent-facing workflow page instructs a caller to
  authenticate against Wyrd routes with the application's own header:
  line 28 `curl -sf "$WYRD_SERVER_URL/v1/cards/default/my-dataset/0.1.0" -H
  "Authorization: Bearer $WYRD_ACCESS_TOKEN"`; line 59 the same header on
  `POST $WYRD_SERVER_URL/v1/cards`; line 81 `"Authorization": f"Bearer
  {token}"` in the Python snippet. Both routes take the tenant `Caller`
  extractor, which reads `X-Wyrd-Access-Token` only
  (`crates/wyrd/wyrd-server/src/components/auth/token_extract.rs:24-49`). The
  file contains zero occurrences of `x-wyrd-access-token`. The sibling page
  `docs/src/content/docs/self-hosting/local-development.svx:41` states the
  opposite and correct rule ("Wyrd does not use the standard `Authorization`
  header… send it as `X-Wyrd-Access-Token`"), so the two pages contradict each
  other. `mise run docs:check` exits 0 because it does not inspect header prose
  in `.svx` bodies.
- **Observable consequence.** An agent or operator who follows the
  documented three-step workflow verbatim receives `401
  WYRD_AUTH_401_UNAUTHENTICATED` on every request. This is the documentation
  surface for the primary consumer persona the spec names, and it is now the
  only place in the tree that still teaches the removed identity model's header.
- **Required testable correction.** Replace the header in all three places with
  `X-Wyrd-Access-Token: Bearer $WYRD_ACCESS_TOKEN` (and the Python dict key with
  `"X-Wyrd-Access-Token"`). Re-run `mise run docs:check`. Because prose is not
  compiled, pair the edit with a repository grep assertion only if one already
  exists; otherwise the correction's proof is the corrected page plus the
  existing `platform_admin_e2e`/`principal_journey` evidence that the canonical
  header is the one that works.

### TR-4 — INCORRECT

- **Obligation.** `INV-015`; the same acceptance criterion as `TR-3`. This
  closeout explicitly owns the remediation prose for this very error code:
  `21abea8c7` rewrote `WYRD_AUTH_400_BAD_TOKEN_FORMAT`'s title and remediation
  to name `X-Wyrd-Access-Token` (`crates/wyrd-spec/src/error.rs:543-548`).
- **Location.** `crates/wyrd/wyrd-server/src/http/error.rs:202`.
- **Evidence.** `AuthError::BadTokenFormat => bad_token_format("authorization
  header malformed")`. The served problem document therefore carries
  `title: "Access token header malformed"`, `remediation: "Use
  \`X-Wyrd-Access-Token: Bearer <token>\` …"`, and `detail: "authorization
  header malformed"` — the detail names a header the server does not read and
  contradicts the remediation beside it in the same payload.
- **Observable consequence.** A caller whose `X-Wyrd-Access-Token` is malformed
  is told in the human-readable field that its `Authorization` header is at
  fault. For an automated consumer reading `detail` (the spec's target persona
  is "a careful, literal interpreter that has no ability to ask for
  clarification") this points remediation at the wrong header, and it is the
  last served string in the tree that describes the removed identity model.
- **Required testable correction.** Change the message to name the canonical
  header, e.g. `bad_token_format("x-wyrd-access-token header malformed")`, and
  pin it with a unit assertion on the rendered problem `detail` for
  `AuthError::BadTokenFormat` in `crates/wyrd/wyrd-server/src/http/error.rs`'s
  test module, run through
  `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E
  'test(=http::error::tests::<name>)'`.

---

## 3. Verification limits

What this review could not prove, and why:

1. **MCP scope gating was not re-executed.** The criterion rests on
   `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/principals.rs::a_write_tool_is_scoped_at_dispatch_not_merely_hidden`,
   which is `#[ignore]`d and selected only by `test:bifrost:journey:mcp`
   (`mise.toml:362-368`). I verified the test exists, asserts both the catalog
   difference and the dispatch-time refusal, and is selected by a lane that
   passes `--run-ignored=all`. I did not run that lane: it is prior-landed work
   (`128eb40`), untouched by this closeout, and running it is a whole-binary
   journey lane rather than a focused selector. Accepted on inspection.
2. **`mise run test:platform:journey` remains broken** (missing `setup:postgres`
   dependency), independently of this task, as the implementor records. I used
   the subject file's working equivalent. I did not verify that repairing the
   lane is out of this task's write set beyond the implementor's assertion and
   the absence of `mise.toml` from the closeout diff — note that the task's
   declared test write set *does* list `mise.toml`, and the new CLI journey
   needed no `mise.toml` change because `test:cli:journey` already runs
   `--test cli` whole.
3. **`mise run fmt` and `mise run lints` were not run as such.** `lints` is a
   workspace-wide clippy aggregate; per `VER-003`/`VER-004` I substituted the
   task's own focused five-package `cargo clippy --locked … --all-targets`
   (exit 0) and confirmed the tree is clean, which also confirms formatting was
   committed as generated.
4. **The revocation request body's inertness is unproven either way.** The
   server ignores `RevokePrincipalRequest` while the CLI makes `--kind`
   mandatory, so `wyrd principal revoke <id> --kind agent` against a service
   principal would presumably still succeed. I judged the deferral legitimate
   because the published contract declares no request body, so no acceptance
   criterion of this task depends on it. I did not construct that
   wrong-`--kind` case to confirm the behaviour.
5. **`signing_key`'s span claim was not exercised.** Its rustdoc
   (`wyrd-auth-verify/src/lib.rs:397-399`) says it records the key id "on the
   current span"; `verify_platform` declares no `kid` field on its
   `#[tracing::instrument]`, so the `record` is a no-op on the platform path
   (the tenant path at `:454-458` does declare `kid = Empty`). Behaviour is
   unaffected and I am not reporting it; it is recorded here so a later reader
   does not mistake it for something I missed.
6. **Only the closeout range was audited in depth.** The cumulative branch
   range was read for context (163 files). Landed-earlier items
   (`4225069`, `7ecd356`, `128eb40`, `384e167`) were verified for present-tree
   consistency, not re-reviewed as implementations.

---

## 4. Overall result

**FAIL**
