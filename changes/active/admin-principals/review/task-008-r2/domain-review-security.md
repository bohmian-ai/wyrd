# Domain review — security, identity, and trust boundaries

Reviewer: `domain-rev-security` (independent, review-only; no source file changed)
Subject: TASK-008 cumulative candidate `4668d8d33` on `claude/admin-principals-spec-qfsmjc`
Closeout range traced: `289978fcc~1..4668d8d33`

**Result: FAIL** — three material findings (`DS-1`, `DS-2`, `DS-3`).

---

## 1. Reviewed boundary and how it was traced

| # | Boundary | How it was traced | Verdict |
|---|---|---|---|
| 1 | Header binding and indistinguishable rejection | Read `platform_extractor.rs`, `caller_extractor.rs`, `token_extract.rs` at HEAD and their diff. `extract_platform_token` now reads `WYRD_ACCESS_TOKEN_HEADER`, still returns `Option`, and every `None` renders through the single `unauthenticated()` (`platform_extractor.rs:83-94`) — missing, malformed, wrong-scope, unknown/revoked credential, and suspended principal all produce `WYRD_AUTH_401_UNAUTHENTICATED` with `{"plane":"platform"}`. `extract_wyrd_access_token`'s informative form is **not** reused. Only store/config outages differ, as before. Both extractors require the same `Bearer ` prefix, so the two planes present one header grammar. | Preserved |
| 2 | Plane separation | Separation is carried by `verify_platform`'s `PLATFORM_TOKEN_SCOPE` check (`wyrd-auth-verify/src/lib.rs`, unchanged) plus the declared extractor type. Tenant→platform proven by `platform_admin_e2e::the_two_control_planes_cannot_reach_each_other:214-238`, which also asserts the refusal is byte-identical to the anonymous one. Platform→tenant proven by the same test at `:240-256` (`GET /v1/cards` with a platform session → 401); the tenant path reaches that verdict through full `verify_authenticated_principal`, not through a claims pre-parse. Commit `343cda711` fixes the one pre-parse site (`token_extract::tenant_from_unverified_access_token`, consumed only by `components/authz/check.rs`) from `BadTokenFormat` to `Unauthenticated`. I enumerated every other token-accepting entry point: `principal_extractor`/`AuthenticatedPrincipal`, `http/middleware/authenticate.rs`, `grpc/peer_auth.rs`, `vala-bifrost-redux/gate/auth.rs` (already `Unauthenticated`), and the token-exchange `subject_token` copy at `components/auth/routes.rs:333`. The last is a request-*body* field on a preview-gated exchange, not a header credential, and its `BadTokenFormat` discloses nothing about a third party's token, so it is not a symptom of the same defect. `AuthenticatedPrincipal` is **not** on the platform path; `Principal.tenant_id` (`wyrd-runtime/src/principal.rs:23`) and `VerifiedToken`'s tenant fields (`wyrd-auth-verify/src/lib.rs:297,841,861`) remain non-optional — the only `Option<DataTenantId>` is `AuthContext::tenant_id()`, an enum accessor that landed earlier on the branch, not a nullable field. | Honored |
| 3 | The application's own `Authorization` header | Tree-wide search over `crates/ sdks/ docs/ examples/ architecture/` for `header::AUTHORIZATION`, `HeaderName::from_static("authorization")`, `"authorization",`, `.get("authorization")`, `AUTHORIZATION`, and the prose `Authorization: Bearer`. Every code hit judged individually (table in §2). | **Fails — `DS-1`, `DS-2`** |
| 4 | Signing-key resolution | `TokenVerifier::signing_key` (`wyrd-auth-verify/src/lib.rs:391-424`) is a verbatim lift of both former preambles: same `decode_header`, same `kid`→`Kid::new` mapping, same `decoding_keys.get`, same `Arc::clone`, all failures still `AuthError::InvalidToken`. The only behavioral delta is that the platform path now also records the `kid` span field, which the tenant path already did — a trace field, not a response observable. `verify_external_against` is untouched and still resolves against tenant JWKS, so the internal and external key sets stay distinct: no key confusion, no newly accepted or newly rejected token. | Behavior-preserving |
| 5 | Revocation timeliness | Diff to `platform_credentials.rs` is rustdoc only, and the corrected text matches the mechanism: `verify_platform` does not touch `TokenVerifier::cache` (the cache lives only in the tenant `verify` path), `PlatformSessions::confirm`/`confirm_credential_session` re-reads the credential row, the federated path re-reads the principal, and `resolve_grant` re-reads the grant per request. No epoch, cache, memoization, or lease was introduced anywhere on the platform path. `wyrd-server/src/auth/revoke.rs` changed by a `#[utoipa::path]` attribute only — no behavior, no audit, no epoch, no replay surface. `platform_admin_e2e::revoking_a_platform_credential_ends_its_live_sessions` and `replaying_a_revoke_does_not_disturb_the_surviving_credential` both pass. | Stop condition honored |
| 6 | Credential plaintext | Traced `wyrd-cli/src/client.rs`, `auth/{issue_key,login,refresh,trusted_issuer,workload_binding}.rs`, `principal/revoke.rs`, `query/mod.rs`, `card.rs`, `wyrd-client/src/auth.rs`, `principals/handle.rs`, `platform/handle.rs`, `tests/principal_journey.rs`. `Platform::with_session` uses `ClientConfig::default()`, whose `TokenCacheMode` default is `InMemory` (`config.rs:24-31`), so the platform session is never written to disk; `AuthMiddleware::save_to_disk` is gated on `Disk` mode. `PlatformSession`, `Platform`, `EvalProtocol`, and `EvalRun` all have hand-written `Debug` impls that withhold secrets, and `ResolvedCredential`/`CredentialSource` redact. Plaintext reaches stdout only at the sanctioned prints (`issue_key.rs:76`, `login.rs::print_tokens`). The journey deliberately passes the token via `WYRD_ACCESS_TOKEN` rather than `--token` to keep it out of the process table. | **One new leak — `DS-3`** |
| 7 | Outbound credential leakage | `WyrdClient::request_external_stream` (`wyrd-client/src/client.rs:213-228`) delegates to `HttpTransport::request_external_stream` (`transport/http.rs:287-305`), which builds the request from `self.client` and applies only caller-supplied headers: it never calls `self.auth.bearer()`, never sets `x-wyrd-access-token` or `wyrd-request-id`, and never goes through `authenticated_url`. `build_http_client` (`transport/http.rs:749-765`) sets no `default_headers` and no cookie store, so the shared pool carries nothing implicit; a redirect therefore has no Wyrd material to forward. `wyrd-cli/src/eval/agent.rs:118-127` passes exactly one header, `content-type: application/json`, and the serialized turn body — no token, no lease, no request id. Conversely the authenticated helpers all route through `authenticated_url`, which rejects cross-origin absolute URLs, so the lease and the access token cannot leave the Wyrd origin. I could not break this. | Clean |
| 8 | Eval lease | The lease is authorization-bearing and remains validated server-side: `components/eval/routes.rs::check_lease` still does the `subtle::ConstantTimeEq` compare after `lookup`, and `AuthenticatedPrincipal`/`Caller` still gates the route beneath it. Moving to `x-wyrd-eval-lease` is required by `INV-015` and weakens nothing — the client owns only the header name and value, never the verdict. `EvalRun` stores the lease pre-formatted and private, re-applies it on every retry attempt (`send_with_retry`, `extra_headers`), and exposes no accessor. `request_json_with_headers` is `pub(crate)`, so no consumer outside `wyrd-client` can compose headers. `eval_server_protocol::server_protocol_carries_lease_after_open` asserts the new header carries the lease on every post-open call. | Not weakened |
| 9 | Scope gating | Verified the structural claim at HEAD rather than accepting it. CLI: `wyrd-cli/src/cli.rs:57-85` enumerates `Plan, Apply, Get, Latest, List, Load, Delete, Auth, Eval, Principal, Query` — no platform command, and `wyrd-client`'s `Platform` handle has no consumer anywhere in `crates/` or `sdks/`. MCP: the whole catalog is `mcp/{bifrost,principals,probe}.rs`; `principals.rs` exposes exactly `principals.list_credentials` (unscoped read) and `principals.revoke_credential` (in `write_descriptors()`, discovery gated on `may_administer` reading `service_accounts_write` from the verified token, with the operation re-authorizing and auditing). No platform tool exists. The CLI refusal code is pinned by `principal_revoke_cli_journey_refuses_an_unprivileged_caller`, which asserts `WYRD_PERMISSION_403_DENIED_RBAC` on stderr. | Claim true |
| 10 | Audit | No audit write was dropped or relocated. The extractor refactor touches only header selection and `RequestId` reading; `PlatformCaller` still carries `session.credential_id`, so `platform_caller_carries_its_minting_credential` propagation into audit is intact (its unit test `a_caller_carries_a_credential_only_when_one_minted_its_session` passes). `revoke_principal`'s transactional `authorize_service_accounts_write` decision is untouched. The CLI refactor moved no authorization decision client-side: every command still receives the server's stable verdict. | Intact |
| 11 | Error catalog prose | The three re-pointed remediations are accurate for the header-carried paths that emit them. But the same codes carry server-side `detail` messages that still name the wrong header (`DS-2`), and the `X-Wyrd-Access-Token` text discloses nothing beyond what the published contract now states, so the platform plane's indistinguishability is not weakened by the prose. | **Partly fails — `DS-2`** |

### `Authorization` hit-by-hit judgement (boundary 3)

| Hit | Judgement |
|---|---|
| `crates/skald/skald-providers/src/auth/{openai,google_oauth,...}.rs` | Outbound calls to third-party model providers. Correct use of the header; not a Wyrd surface. |
| `crates/vala/vala-core/src/alert_router/webhook.rs:176,568` | Redaction of operator-configured outbound webhook headers. Defensive, correct. |
| `crates/wyrd/wyrd-server/src/components/auth/platform_extractor.rs:247,265` | The two new unit tests that assert the header is *not* read. Correct. |
| `crates/shared/wyrd-client/tests/{pg_auth_e2e_against_fixture.rs:96,transport/http.rs:750}` | Tests asserting the SDK neither writes nor authenticates on it. Correct. |
| `crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs:465` | Asserts an OTLP export carrying `authorization` metadata is `Unauthenticated`. Correct. |
| `wyrd-ui/.svelte-kit/`, `wyrd-ui/build/` | SvelteKit-generated output, not source. Out of scope. |
| `openapi.yaml`, `sdks/` | No hit. The generated contract declares one scheme and it is `X-Wyrd-Access-Token`. Correct. |
| `crates/wyrd/wyrd-server/src/http/error.rs:202`, `crates/wyrd/wyrd-auth/src/error.rs:35` | **`DS-2`** — live caller-visible message naming the wrong header. |
| `docs/src/content/docs/for-agents/workflow.svx:28,59,81` | **`DS-1`** — agent-facing documented workflow instructs the wrong header. |
| `architecture/diagrams/wyrd-control-plane.svg:121` | Footer listing external standards Wyrd "hooks in through", alongside ext_authz and jwt-bearer. Ambiguous marketing prose rather than an instruction about Wyrd's own credential header; not reported. |

---

## 2. Authority and source coverage

Read in full: `changes/active/admin-principals/tasks/TASK-008-sdk-and-mcp-projection.md`; `architecture/wyrd-design.md` §"Cross-service delegation" (lines 488-512, the canonical-header authority and its worked request); `AGENTS.md` §2, §9, §11.

Diff read as a real unified diff via `rtk proxy git diff 289978fcc~1..4668d8d33` over: `crates/wyrd/wyrd-server/src/components/auth/*`, `crates/wyrd/wyrd-server/src/auth/revoke.rs`, `crates/wyrd/wyrd-server/src/components/eval/routes.rs`, `crates/wyrd/wyrd-server/src/http/openapi.rs`, `crates/shared/wyrd-auth-verify/src/lib.rs`, `crates/shared/wyrd-client/src/{auth.rs,eval/,platform/,principals/handle.rs,transport/http.rs}`, `crates/wyrd/wyrd-auth/src/platform_credentials.rs`, `crates/wyrd-spec/src/error.rs`, all of `crates/wyrd/wyrd-cli/src/` and `crates/wyrd/wyrd-cli/tests/`, `openapi.yaml`, `docs/`.

Read at HEAD (beyond the diff, to judge the seam rather than the hunk): `token_extract.rs`, `principal_extractor.rs`, `components/auth/routes.rs`, `components/authz/check.rs`, `components/platform/identity.rs`, `vala-bifrost-redux/src/gate/auth.rs`, `wyrd-client/src/{config.rs,transport/credential.rs,transport/http.rs,client.rs,platform/handle.rs}`, `wyrd-cli/src/{cli.rs,client.rs,eval/agent.rs,eval/output.rs,error.rs}`, `wyrd-server/src/mcp/principals.rs`, `wyrd-runtime/src/principal.rs`, `mise.toml`.

---

## 3. Verification performed, and its limits

Ran (focused only; no broad aggregate relied on):

```
mise exec -- cargo nextest run --locked -p wyrd-auth-verify --lib
  → 41 tests, 41 passed

mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(/components::auth::platform_extractor::/) or test(/components::auth::token_extract/) or test(/http::openapi::/)'
  → 10 tests, 10 passed (includes the two new Authorization-is-never-read cases
    and every_authenticated_path_declares_the_one_wyrd_scheme)

WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test platform_admin_e2e'
  → 17 tests, 17 passed

WYRD_CLI_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 cargo nextest run --locked -p wyrd-cli --test cli \
   -E "test(/principal_journey::/) or test(=eval_server_protocol::server_protocol_carries_lease_after_open)" --test-threads=1'
  → 3 tests, 3 passed
```

I also confirmed that `tests/principal_journey.rs` is registered as a module in `tests/cli.rs` (`#[path = "principal_journey.rs"] mod principal_journey;`), so it really runs inside the `--test cli` target the `test:cli:journey` lane invokes — the task's own "unrun tests do not earn their place" standard is met here.

**Limits — what I could not verify.**

- `wyrd-server --lib` auth `pg_tests` (`principal_extractor`, `caller_extractor`, `auth::routes`) need Postgres and were not run under the wrapper; I judged those seams by source only.
- I ran no lint, clippy, `codegen:check`, `check:client-tier`, or `docs:check` lane. The implementor's claim that `docs:check` passes is consistent with `DS-1`: that lane evidently does not check documented header correctness, so its PASS is not evidence against `DS-1`.
- `Platform`/`PlatformSession` and `Principals::revoke_credential` have no test and no consumer in-tree; I verified `call_no_content`'s empty-body path by reading `request_json_with_headers` (`transport/http.rs:177-186`), which substitutes `b"null"` for an empty `2xx` body, so `D = ()` decodes correctly. That is source evidence, not executed evidence.
- The `token_extract::tenant_from_unverified_access_token` change of `343cda711` has **no test at any tier**; its only consumer is `/v1/authz/check`, which no test in the closeout exercises with a platform session. I read the code and judged it correct, but the new error contract is unproven. I do not raise it as a separate finding because the observable change moves toward less disclosure, not more, and the task's acceptance criteria do not name that route.
- I did not attempt a live redirect experiment against `request_external_stream`; the no-leak conclusion rests on reading that no Wyrd header or default header is ever attached to that request, which makes a redirect immaterial.

---

## 4. Material findings

### `DS-1` — MISSING: the agent-facing documented workflow still tells callers to authenticate with `Authorization: Bearer`

- **Violated obligation.** `AC-013` / `INV-015`; TASK-008 acceptance criteria "No Wyrd-owned surface reads the `Authorization` header; a tree-wide search returns nothing" and "No document, example, or surface still describes a second identity model, a removed bootstrap path, or credential-keyed authorization". `architecture/wyrd-design.md:497-500` is the authority. `AGENTS.md` §2 makes machine-readable docs a primary surface, not a secondary one.
- **Location.** `docs/src/content/docs/for-agents/workflow.svx:28`, `:59`, `:81`.
- **Evidence.** Line 28: `-H "Authorization: Bearer $WYRD_ACCESS_TOKEN"` against `GET $WYRD_SERVER_URL/v1/cards/default/my-dataset/0.1.0`. Line 59: the same header on `POST $WYRD_SERVER_URL/v1/cards`. Line 81: a Python snippet building `{"Authorization": f"Bearer {token}"}`. Those routes take the tenant `Caller` extractor, which reads `X-Wyrd-Access-Token` only (`token_extract.rs:19-49`). Every other docs page is already correct — `self-hosting/local-development.svx:41` even states "Wyrd does not use the standard `Authorization` header" — which makes this page the lone contradiction in the published contract. The implementor's acceptance row claims PASS with the evidence `grep -rn "AUTHORIZATION\|\"authorization\"" crates/`: that search never entered `docs/`, and its pattern would not have matched `Authorization: Bearer` prose in any case.
- **Observable consequence.** An agent following its own dedicated workflow page verbatim — the exact audience `AGENTS.md` names as primary — sends no credential Wyrd reads and receives `WYRD_AUTH_401_UNAUTHENTICATED` on step 1 and step 3. The page also teaches the failure mode this task exists to eliminate, so the defect propagates into every client written from it.
- **Testable correction.** Change all three occurrences to `X-Wyrd-Access-Token: Bearer <token>` (matching `concepts/authentication.svx:12` and `bifrost/reading-data.svx:66`), then add a docs gate that fails when any `docs/src/content/docs/**` code fence sends an `Authorization` header to a `$WYRD_SERVER_URL` path. Re-run `mise run docs:check`. The gate must fail on the current tree and pass after the edit.

### `DS-2` — MISSING: a live 400 response still names the `Authorization` header in its `detail`

- **Violated obligation.** Same acceptance criterion as `DS-1` ("no surface still describes ... the wrong header"), and the task's item 4/`AC-014` premise that the contract a client reads must be internally consistent. The closeout re-pointed the `WYRD_AUTH_400_BAD_TOKEN_FORMAT` *remediation* in `wyrd-spec` but left the `message` that ships in the same problem document.
- **Location.** `crates/wyrd/wyrd-server/src/http/error.rs:202` and `crates/wyrd/wyrd-auth/src/error.rs:35` (asserted by the fixture at `crates/wyrd-spec/src/error.rs:4168`).
- **Evidence.** `AuthError::BadTokenFormat => bad_token_format("authorization header malformed")` and `AuthError::BadTokenFormat => WyrdError::BadTokenFormat { message: "authorization header malformed", .. }`. `AuthError::BadTokenFormat` is raised by `wyrd-auth-verify/src/lib.rs:472,599,606,609,685` — an oversized token or a value that is not a compact JWT — and reaches the caller through `verify_authenticated_principal`, so it is live on every tenant route. The emitted problem document therefore reads `"detail": "authorization header malformed"` next to `"remediation": "Use \`X-Wyrd-Access-Token: Bearer <token>\` ..."`: the same response names two different headers.
- **Observable consequence.** `curl -H 'X-Wyrd-Access-Token: Bearer <20KB of junk>' $WYRD_SERVER_URL/v1/cards` returns a 400 whose human-readable detail tells the operator to fix a header they did not send and Wyrd does not read. A client or agent that keys remediation off `detail` rather than `remediation` is sent to the wrong header — the precise confusion `INV-015` exists to end, now published by Wyrd itself.
- **Testable correction.** Change both messages to name `X-Wyrd-Access-Token` (e.g. `"X-Wyrd-Access-Token header malformed"`), update the `wyrd-spec` fixture at `error.rs:4168`, and add a unit assertion that no `WyrdError` `message` or `remediation` produced by the auth-error mappers contains the string `authorization header`. Verify with `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/http::error::/)'` and `-p wyrd-spec --lib -E 'test(/error::tests::/)'`.

### `DS-3` — REGRESSION: `wyrd auth login` now echoes the pasted OIDC callback, including the authorization code, into an error printed to stderr

- **Violated obligation.** TASK-008 **Invariants**: "Credential plaintext crosses a client surface exactly once, in the response that created it, and is never persisted by a client." An error stream is a second surface, and in CI it is a persisted one.
- **Location.** `crates/wyrd/wyrd-cli/src/auth/login.rs:98-103` (the `value: input.to_owned()` at `:101`), rendered by `WyrdCliError::InvalidArgument`'s `#[error("invalid {field} {value:?}: {expected}")]` (`crates/wyrd/wyrd-cli/src/error.rs:309-323`) and printed by `crates/wyrd/wyrd-cli/src/eval/output.rs:13-25`.
- **Evidence.** Before this closeout the same branch returned a value-free hint: `WyrdCliError::AuthFailed { status: 0, detail: "paste the full callback URL or \`code=<>&state=<>\` query string" }`. The diff replaced it with `InvalidArgument { field: "callback", value: input.to_owned(), expected: ... }`. `print_cli_error` then emits the operator's pasted text **twice** — once inside the `wyrd_cli_error` JSON line as `message`, once as `error: ...` prose — both on stderr. The branch is reached whenever `code` and `state` are not *both* present, so a paste that carries a `code` but no `state` (a truncated paste, or an IdP that returns `session_state` instead of `state`) is echoed with the authorization code intact.
- **Observable consequence.** A single-use OIDC authorization code lands in terminal scrollback, in any redirected stderr, and — because the JSON line is machine-shaped and designed to be collected — in CI logs and log aggregators. The code is still redeemable at `/auth/token` until it is used or expires, so this converts an operator typo into a credential disclosure on a durable surface. It is also the one place in the closeout where a credential reaches a surface it did not reach before.
- **Testable correction.** Keep `InvalidArgument` but stop carrying the raw input: pass a shape description instead (`value: "<callback>"`, or name only which of `code`/`state` was absent), so the message is actionable without reproducing the credential. Add a unit test in `login.rs` asserting that `parse_callback_input("?code=super-secret-code")`'s error `Display` and `print_cli_error` JSON contain neither `super-secret-code` nor the substring `code=`. Verify with `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(/auth::login::tests::/)'`.

---

## 5. Result

**FAIL.**

The security core of this closeout is sound and, in several places, better than the task asked for: the platform plane moved onto the canonical header without surrendering its indistinguishable rejection; signing-key resolution collapsed without changing a single accept/reject decision or confusing the internal and external key sets; the caches-nothing invariant is stated truthfully and no epoch, cache, or lease was smuggled onto the platform path; the two-plane separation is now genuinely carried by scope and extractor type and is proven in both directions against a real server; and the highest-risk new seam — the CLI's call to a third-party agent endpoint through the shared client — carries no Wyrd credential on any path I could construct. `AuthenticatedPrincipal` was kept off the platform plane and no nullable tenant appeared.

What fails is the reach of the change, not its centre. `INV-015` is a property of every surface, and two surfaces still publish the old header: the one documentation page written for the agents `AGENTS.md` calls Wyrd's primary consumer (`DS-1`), and the `detail` of a 400 that any caller can trigger (`DS-2`). The acceptance row asserting a clean tree-wide search rests on a `grep` that never left `crates/` — so that criterion is claimed, not met. Separately, one credential now reaches a surface it did not reach before this closeout (`DS-3`).

All three are small, local, and independently testable. None requires re-planning.
