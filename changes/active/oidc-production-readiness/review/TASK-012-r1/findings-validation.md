# TASK-012 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`

The candidate remained the named commit throughout validation. This pass read
the complete cumulative diff, every discovery report and the focused follow-up,
then independently traced each proposed finding through the current source. It
ran no Cargo or `mise` command. The recorded evidence was assessed under the
task's narrow write-set rule; full journey suites and aggregates remain change-
review evidence.

## Proposal validation

| Discovery proposal | Disposition | Source validation |
|---|---|---|
| `INV-012-001`; `MNT-TASK-012-1` | **CONFIRMED** as `FIND-TASK-012-1` | `TokenExchange::exchange` is public and still accepts the full public `TokenRequest`, including `DeviceCode` and `RefreshToken`; the CLI journey executes the device variant. The retained custom form encoder is therefore not constrained to RFC 8693 and RFC 7523 as TASK-012 requires. |
| `SEC-1` | **REVISED** as `FIND-TASK-012-2` | Saved-login identity is correctly canonicalized, but that does not cover the shared transport boundary. `HttpConfig::validate` discards its parsed URL, while `TokenExchange` and `HttpTransport` retain and print the raw spelling. Explicit credentials, platform connect, and direct CLI auth commands reach those owners without saved-login canonicalization. The correction is narrowed to reusing the existing normalized-origin authority once for both shared HTTP owners. |
| `REPO-001`; `MNT-TASK-012-2` | **REVISED** as `FIND-TASK-012-3` | The new adapter field and associated types lack rustdoc, `AsyncHttpClient::call` has placeholder documentation, and the changed OAuth async boundaries omit material uncertain-completion behavior. The changed `token_body` fixture helper also has no rustdoc. The correction is documentation on the existing owners only; no new check or abstraction is warranted. |
| `REPO-002` | **CONFIRMED** as `FIND-TASK-012-4` | TASK-012 names seven Rust tests at lines 201–206, but its evidence records only family/target lanes for them. `AGENTS.md` section 11 and the task's own lines 160–161 require exact nonzero selectors for every named test. |
| `PLAT-001` | **REJECTED** | `webbrowser 1.2.4` does invoke command interpreters in its dependency-owned WSL fallback, but the governing lead decision routes native Windows launcher closure through exactly `webbrowser` and accepts the exact Windows-target crate check. TASK-012 also fixes that dependency version and prohibits a Wyrd launcher or escaping layer. Requiring a WSL branch, replacement dependency, setting, or additional platform harness would reopen a locked decision and add nonstandard complexity. The overbroad rustdoc sentence has no independent behavioral consequence and cannot create a style-only round. |

No other discovery report proposed a material finding. The behavior,
system-resilience, and durability reports' positive conclusions were checked
against the same owners and do not conflict with the retained ledger once the
four gaps above are separated by cause.

## Final deduplicated finding ledger

### FIND-TASK-012-1 — CONFIRMED — DRIFT: the custom form exchange still admits device and refresh grants

- **Discovery sources:** `INV-012-001`, `MNT-TASK-012-1`.
- **Violated obligation:** TASK-012 lines 31–37, 43–44, 54–55, 75–78, and
  139–140 require device and refresh to be owned by `oauth2`, retain the custom
  form POST only for RFC 8693 and RFC 7523, and leave no parallel hand
  `TokenExchange` POST/decode path for the replaced grants.
- **Exact location:** `crates/shared/wyrd-client/src/auth.rs:261-275,394-438,952-1027`;
  `crates/wyrd/wyrd-cli/tests/cli_login_journey.rs:283-304,341-346`.
- **Source trace:** `wyrd_spec::auth::TokenRequest` produces all four grant
  variants. Public `TokenExchange::exchange(&TokenRequest)` passes every variant
  to `grant` and the hand `post_form` serializer. Production `AuthMiddleware`
  callers currently produce only RFC 8693 `TokenExchange` and RFC 7523
  `JwtBearer` requests before reaching this sink; `platform_session` has its own
  RFC 8693 call. The CLI journey proves the broad surface is executable by
  producing `DeviceCode` requests for pending, wrong, and replayed codes. As a
  public method on a public type, the same bypass is available to external Rust
  callers, including with `RefreshToken`. Sibling device and refresh callers
  otherwise use the correct `device_access_token` and `refresh` methods backed
  by `oauth2`.
- **Observable consequence:** the shared client exposes two implementations of
  device and refresh wire behavior. A caller can bypass the vetted library's
  polling, error handling, and future protocol corrections while remaining on
  the advertised shared-client API.
- **Decision-complete correction:** remove the broad public arbitrary-
  `TokenRequest` exchange surface and constrain the existing custom form owner
  to the two grant producers it still legitimately serves: RFC 8693 token
  exchange and RFC 7523 JWT bearer. Keep device and refresh reachable only
  through the existing `oauth2`-backed methods. Remove the CLI journey's manual
  device polls where the standard client path or the existing server-side grant
  coverage already proves the outcome; do not add a raw-protocol production
  helper, another grant abstraction, a compatibility path, or a source-grep
  check. Preserve the lead-approved RFC 7009 method unchanged.
- **Focused closure proof:** source/API inspection showing that no callable
  custom form path accepts `DeviceCode` or `RefreshToken`; exact focused tests
  for the affected `wyrd-client` auth behavior; and the one filtered
  `cli_device_login_journey`. Do not run or require an unfiltered journey suite
  or aggregate.

### FIND-TASK-012-2 — REVISED — INCORRECT: parsed target validation is discarded before shared secret-bearing clients are built

- **Discovery source:** `SEC-1`.
- **Violated obligation:** TASK-012 Scenario 3 lines 94–115 requires every
  shared caller to use one parsed target decision, path/query/fragment spellings
  of one deployment to resolve to one origin, and userinfo to be refused and
  never printed. The security posture prohibits secret material in diagnostics.
- **Exact location:** `crates/shared/wyrd-client/src/transport/config.rs:201-241`;
  `crates/shared/wyrd-client/src/auth.rs:187-252,405-438`;
  `crates/shared/wyrd-client/src/transport/http.rs:119-165,738-769`;
  `crates/shared/wyrd-client/src/config.rs:185-203`.
- **Source trace:** caller input enters `HttpConfig.base_url` or direct CLI and
  platform constructors. `HttpConfig::validate` parses the value only to inspect
  scheme and loopback host, accepts HTTPS userinfo, returns `()`, and discards
  the parsed value. `TokenExchange::new` and `HttpTransport::new` then retain the
  raw string, expose it from `Debug`, and append endpoint paths as text.
  `canonical_origin` correctly delegates to `HttpsOrigin::of_url`, but only the
  saved-login selection path uses it. Explicit bearer/API-key/workload
  resolution returns before that call; `WyrdClient::with_config`,
  `Platform::connect`, `wyrd auth refresh`, login/logout, and the human-login
  harness construct one or both raw-target owners directly. Thus userinfo can
  survive into diagnostics, and path/query/fragment spellings can produce a
  different or malformed effective endpoint rather than the deployment root.
- **Observable consequence:** URL userinfo, commonly credential material, can
  appear in client debug output, and equivalent spellings of one deployment can
  send access tokens, refresh tokens, API keys, workload assertions, or
  revocation forms to the wrong effective path.
- **Decision-complete correction:** reuse the existing standard `url::Url` plus
  `HttpsOrigin::of_url` normalized-origin authority at the shared `HttpConfig`
  construction boundary. Reject userinfo there without echoing it, retain one
  normalized origin, and make both `TokenExchange` and `HttpTransport` derive
  their endpoints from that same result. Preserve HTTPS and real loopback HTTP.
  Do not add another URL grammar, redaction layer, allowlist, setting, path-
  prefix compatibility mode, or downstream per-caller guard.
- **Focused closure proof:** exact owner-level tests proving that username or
  password userinfo is rejected before either shared client is constructed and
  no returned error contains the sentinel, and that case/default-port/path/
  query/fragment spellings produce the same `TokenExchange` and
  `HttpTransport` origin. Reuse the existing form/redirect tests to prove
  requests still reach root auth endpoints; do not duplicate one test per
  grant or run a full identity journey.

### FIND-TASK-012-3 — REVISED — VIOLATION: changed OAuth items omit mandatory rustdoc and uncertain-completion contracts

- **Discovery sources:** `REPO-001`, `MNT-TASK-012-2`.
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require substantive rustdoc for every new or
  materially modified Rust item, including private fields, associated types,
  helpers, and tests, and require cancellation, partial-progress, idempotency,
  and retry behavior where relevant. Missing or placeholder rustdoc is an
  explicit hard blocker.
- **Exact location:** `crates/shared/wyrd-client/src/auth.rs:131,133-156,261-438,1143-1150`.
- **Source trace:** `AuthHttp` and its tuple field are new; the field is
  undocumented. Its `AsyncHttpClient::Error` and `Future` associated types have
  no docs, while `call` merely restates delegation. `AuthHttp::send` and the new
  or materially changed device authorization, device redemption, refresh,
  revocation, grant, and form-send operations cross a network boundary where a
  caller can be cancelled after the server accepted the request but before the
  response is observed. Refresh may rotate the durable token; device redemption
  may consume the code after minting; revocation is idempotent. Their current
  docs do not consistently state those partial-progress and retry consequences.
  The materially changed `token_body` fixture helper has no rustdoc explaining
  that its `expires_in` body is the RFC 6749 response shape under test.
- **Observable consequence:** the candidate violates the repository's hard
  documentation gate and leaves maintainers without the retry-safety contract
  needed to change credential rotation and redemption safely.
- **Decision-complete correction:** document the existing adapter field,
  associated types, adapter call, and changed fixture helper. On the existing
  async owner methods and shared send/form boundary, record the applicable
  cancellation, server-side partial progress, idempotency, and retry behavior;
  state the refresh rotation and device-redemption uncertain-response cases
  explicitly. Delete documentation for code removed by `FIND-TASK-012-1`
  instead of documenting dead breadth. Add no new documentation check, type,
  helper, option, or file.
- **Focused closure proof:** source inspection plus `mise run fmt` and the
  narrow Rust lint lane required by the resulting Rust write set. Documentation
  closure needs no new runtime test or aggregate.

### FIND-TASK-012-4 — CONFIRMED — VIOLATION: named Rust tests lack required exact-selector evidence

- **Discovery source:** `REPO-002`.
- **Violated obligation:** `AGENTS.md` section 11, the specification-development
  reference's test-command precision rule, and TASK-012 lines 160–161 require
  every specifically named Rust test in a task artifact or implementation
  report to include and run an exact `mise exec -- cargo nextest` selector. A
  family lane does not replace that proof.
- **Exact location:** `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md:194-213`.
- **Source trace:** the evidence table names
  `transport::config::tests::remote_cleartext_malformed_and_unsupported_targets_are_refused`,
  `transport::config::tests::https_and_loopback_http_are_accepted`,
  `auth::tests::token_exchange_refuses_remote_cleartext`,
  `saved_login::tests::canonical_origin_is_the_url_origin`,
  `saved_login::tests::unsafe_and_corrupt_stores_fail_closed`,
  `auth::tests::token_exchange_never_follows_a_redirect`, and
  `pg_tests::wyrd_client_authenticates_via_wyrd_access_token_header`. The first
  six are `wyrd-client` library tests; the last is in integration target
  `pg_auth_e2e_against_fixture` and requires the repository-managed Postgres
  setup. The recorded `test:shared` result may have executed them, but it does
  not establish that each exact selector is current and nonempty.
- **Observable consequence:** the immutable task evidence does not satisfy the
  repository's required proof that each named selector selected and passed the
  asserted test.
- **Decision-complete correction:** run and record one exact
  `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E
  'test(=<full-name>)'` command for each of the six named library tests. Run and
  record the named integration test as `-p wyrd-client --test
  pg_auth_e2e_against_fixture -E
  'test(=pg_tests::wyrd_client_authenticates_via_wyrd_access_token_header)'`
  inside `scripts/postgres/with-test-postgres.sh` after
  `mise run db:migrate:all:inner`. Update only the existing task evidence; add
  no script, check, fixture, lane, or test.
- **Focused closure proof:** the seven recorded exact commands each report one
  selected passing test. Do not rerun or require `test:shared`, an unfiltered
  identity journey, `test:rust`, `gate`, or another aggregate solely to close
  this evidence finding.

## Validation outcome

The final ledger contains four bounded findings:
`FIND-TASK-012-1` through `FIND-TASK-012-4`. All corrections are available
within the approved task and existing owners; none requires a new product,
public option, architecture, security model, compatibility mechanism,
concurrency semantic, or persistent-data decision. The validated recommendation
is `FIX_REQUIRED`, not `SPEC_REVISION_REQUIRED` or `BLOCKED`.
