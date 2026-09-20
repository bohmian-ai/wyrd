# Admin principals whole-branch review 04 — structured Ponytail findings validation

## Immutable subject

| Item | Value |
|---|---|
| Repository | `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces` |
| Base | `c5c20754a167e8f4d74a555a720bd51df6179a6f` |
| Candidate / reviewed HEAD | `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30` |
| Approved authority | `changes/active/admin-principals/spec.md`, revision 10, status `approved` |
| Approved spec SHA-256 | `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837` |
| Original task authority | `TASK-001` through `TASK-008` under `changes/active/admin-principals/tasks/` |
| Prior review authority | Whole-branch-03 validation, verdict, remediation packet, and appended evidence |
| Wave-1 inputs | `task-review.md`, `standards-review.md`, `domain-review-security.md`, `domain-review-data.md`, and `domain-review-contract.md` in this directory |

The candidate and spec hash matched the values above before this report was
written. `.codegraph/` is absent, so source and callers were traced directly.
The pre-existing dirty `README.md`, approved spec, and whole-branch-03 verdict
were treated as owner state and were not changed. This validation writes only
this file.

## Validation outcome

**FIX_REQUIRED.** Sixteen material roots remain: the stable
`FIND-admin-principals-13`, `FIND-admin-principals-R3-5`, and
`FIND-admin-principals-R3-6`, plus thirteen new round-4 roots. The ledger is
deduplicated by correction boundary: the OpenAPI route/error/security/test
proposals are one retained root; architecture and public-doc drift is one root;
and missing tenant/platform token-exchange audit is one root with two reachable
issuance owners.

The full Ponytail ladder was applied. Every correction below first deletes or
narrows candidate machinery, then reuses an existing owner or standard-library
capability. No new audit sink, credential model, revocation service, schema
migration framework, HTTP client, route catalog, test harness, or configuration
knob is justified.

**SPEC_REVISION_REQUIRED: none.** All retained outcomes are already fixed by
revision 10, the original tasks, or repository authority. Their implementation
details are reversible local choices.

## Wave-1 proposal disposition

| Wave-1 proposal | Validation | Retained disposition |
|---|---|---|
| Task `TREV-R4-1` — User revocation leaves refresh authority live | **CONFIRMED** | `FIND-admin-principals-R4-2` |
| Task `FIND-admin-principals-13` — incomplete reachable OpenAPI errors | **CONFIRMED / CONSOLIDATED** | Stable `FIND-admin-principals-13` |
| Task `FIND-admin-principals-R3-5` — combined retained-history upgrade proof absent | **CONFIRMED** | Stable `FIND-admin-principals-R3-5` remains open |
| Task `FIND-admin-principals-R3-6` — exact named-command record incomplete | **CONFIRMED** | Stable `FIND-admin-principals-R3-6` remains open |
| Standards OpenAPI completeness finding | **CONFIRMED / CONSOLIDATED** | Stable `FIND-admin-principals-13` |
| Standards architecture/public-doc drift finding | **CONFIRMED / EXPANDED BY SEARCH** | `FIND-admin-principals-R4-10` |
| Standards mandatory Rustdoc finding | **CONFIRMED** | `FIND-admin-principals-R4-12` |
| Standards final diff-hygiene failure | **CONFIRMED** | `FIND-admin-principals-R4-13` |
| Security `SEC-R4-1` — revocation uncertainty fails open | **CONFIRMED** | `FIND-admin-principals-R4-1` |
| Security `SEC-R4-2` — OIDC role replacement does not advance epoch | **CONFIRMED** | `FIND-admin-principals-R4-3` |
| Security `SEC-R4-3` — refresh-replay audit omits stale credential id | **CONFIRMED** | `FIND-admin-principals-R4-4` |
| Security `SEC-R4-4` — tenant API-key exchange is unaudited | **CONFIRMED / CONSOLIDATED** | `FIND-admin-principals-R4-5` |
| Security `SEC-R4-5` — platform credential/federated session issuance is unaudited | **CONFIRMED / CONSOLIDATED** | `FIND-admin-principals-R4-5` |
| Data `DATA-04-1` — init commits before fallible credential disclosure | **CONFIRMED** | `FIND-admin-principals-R4-6` |
| Data `DATA-04-2` — generic field-id fallback weakens schema validation | **CONFIRMED** | `FIND-admin-principals-R4-7` |
| Data `DATA-04-3` — retained-history upgrade proof absent | **CONFIRMED / DUPLICATE** | Stable `FIND-admin-principals-R3-5` |
| Data `DATA-04-4` — only one provisioning failure stage is proved | **CONFIRMED** | `FIND-admin-principals-R4-8` |
| Contract `R4-CONTRACT-01` — incomplete routes and reachable errors | **CONFIRMED / CONSOLIDATED** | Stable `FIND-admin-principals-13` |
| Contract `R4-CONTRACT-02` — parallel OpenAPI lists/source-substring tests | **CONFIRMED / CONSOLIDATED** | Stable `FIND-admin-principals-13` |
| Contract `R4-CONTRACT-03` — MCP bypasses shared reactive renewal | **CONFIRMED** | `FIND-admin-principals-R4-9` |
| Contract `R4-CONTRACT-04` — stale identity/renewal docs | **CONFIRMED / DUPLICATE** | `FIND-admin-principals-R4-10` |
| Contract `R4-CONTRACT-05` — returned tenant credential bypasses the CLI | **CONFIRMED / REVISED** | `FIND-admin-principals-R4-11`; the smallest proof uses the CLI's existing explicit credential argument, which already classifies an API key and exchanges it |

## Deduplicated retained ledger

### `FIND-admin-principals-13` — CONFIRMED / REVISED — must-fix — the runtime OpenAPI contract is neither complete nor exact

- **Sources:** task finding, standards OpenAPI finding, `R4-CONTRACT-01`, and
  `R4-CONTRACT-02`.
- **Obligation:** `REQ-036`, `REQ-049`, `AC-014`, and `AC-019` require one
  runtime `utoipa` document covering every served public route, real security,
  typed bodies, problem media, and every reachable stable error, without a
  parallel route/error catalog.
- **Exact evidence and reachability:**
  - `http/router.rs:49-60` mounts storage, eval, authz, cards, principals,
    admin, Bifrost, query, and OTLP under protected `/v1`. `WyrdApiDoc` at
    `http/openapi.rs:146-197` includes only storage download and omits the
    upload/local storage operations (`storage/routes.rs:34-45`), all eval
    operations (`eval/routes.rs:46-51`), authz (`authz/routes.rs:9-15`), and
    OTLP (`http/otlp.rs:145-149`). Those routers are live, not deferred.
  - `principals/routes.rs:178-193,335-350,374-414` makes
    `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` reachable from issue/list, but their
    annotations at `:319-371` omit 404. `/auth/token` can surface
    `WYRD_AUDIT_503_UNAVAILABLE` from canonical auth append while its 503
    metadata at `components/auth/routes.rs:83-84` names only
    `WYRD_AUTH_503_VERIFY_UNAVAILABLE`. `/auth/issue-key` reaches the latter
    through tenant acquisition/commit at `:400-418`, while its 503 metadata at
    `:363-364` names only the audit code.
  - The test at `http/openapi.rs:268-299` source-scans only two router modules
    and hard-codes six paths. The stable-code test at `:357+` validates only
    codes already present; it cannot detect an omitted code.
    `ANONYMOUS_PATHS` at `:21-28` is a second hand-written route list, and
    `http/router.rs:177+` checks source substrings rather than requesting the
    assembled router.
- **Material consequence:** `/openapi.json` cannot be used to implement the
  served API or its refusal branches, while the green tests certify only the
  incomplete restatement.
- **Minimum safe correction:** keep `utoipa` and delete the parallel knowledge.
  Co-register route and operation metadata in each owning route module so the
  assembled router and document are composed from the same declarations;
  declare anonymous security on those operations rather than in
  `ANONYMOUS_PATHS`; annotate/register every mounted public operation; and add
  the omitted reachable codes after tracing each handler. Do not add a YAML
  snapshot, generator, second route table, or hand-written error set.
- **Closure proof:** request a real/assembled server's `/openapi.json`, prove
  `/openapi.yaml` is unrouted, compare the co-registered served/document paths,
  and inject representative audit/store failures so each returned code is
  declared by that operation. Run every named OpenAPI test by exact selector.

### `FIND-admin-principals-R3-5` — CONFIRMED — must-fix — the required historical audit upgrade journey still does not exist

- **Sources:** task finding and `DATA-04-3`.
- **Obligation:** the accepted R3 remediation requires one stateful proof that
  begins with the exact old physical table, control registration, staged row,
  chain head, and retained row; upgrades; verifies the legacy hash and field
  ids; publishes both credential shapes; restarts/replays; and reads one
  continuous history.
- **Exact evidence:** `bifrost_catalog.rs:2300-2395` creates an empty legacy
  table/control row and proves schema evolution/reconciliation only.
  `audit_publication.rs:810-845` starts from a fresh current-schema server and
  publishes two current rows once. Hash selection is covered separately in
  unit tests. No test joins old objects, chain state, upgrade, interrupted
  settlement, restart, replay, and query.
- **Material consequence:** local schema/hash tests can pass while a real
  deployment loses chain continuity, cannot read old Parquet, or duplicates a
  frozen publication range after restart.
- **Minimum safe correction:** extend the existing audit-publication/catalog
  fixture into the single scenario already specified. Do not add a generic
  migration harness or another publisher.
- **Closure proof:** one exact Postgres/object-store journey seeds the named old
  state, upgrades with the existing narrow path, verifies the old hash and
  field ids, publishes null/non-null credential rows, interrupts after durable
  publication but before settlement, restarts, replays, and queries every old
  and new row exactly once.

### `FIND-admin-principals-R3-6` — CONFIRMED — should-fix — the approved exact-command evidence is incomplete

- **Source:** task finding.
- **Obligation:** `VER-002` and the R3 packet require every named Rust test to
  be run and recorded with exact package, target, `test(=...)` selector, and
  repository setup wrapper where needed.
- **Exact evidence:** the appended table reports three `wyrd-auth` proofs and
  two replay proofs merely as passing "within the 12-test focused run". It
  gives bare names, not exact commands, for
  `identity_e2e::human_oidc_login_journey`,
  `platform_admin_e2e::a_tenant_administrator_renews_by_re_exchanging_its_credential`,
  and `platform_admin_e2e::a_failed_provisioning_can_be_retried_with_the_same_slug`.
  Aggregate lane counts do not prove those selectors ran nonzero.
- **Minimum safe correction:** no code. Run and append the exact existing tests
  with their package/target/expression, environment, wrapper, and result count.
- **Closure proof:** a reviewer can copy each recorded command, see a nonzero
  exact selection, and reproduce the stated pass.

### `FIND-admin-principals-R4-1` — CONFIRMED — must-fix — revocation-store uncertainty admits tokens

- **Source:** `SEC-R4-1`.
- **Obligation:** `INV-011`, `INV-013`, and the security foundation require
  revocation/admission uncertainty to fail closed.
- **Exact evidence and callers:** both cache-hit and cache-miss branches of
  `TokenVerifier::verify` log "failing open" and return the token when
  `RevocationCheck::epoch` errors (`wyrd-auth-verify/src/lib.rs:475-547`).
  Production installs `SqlRevocationCheck`; tenant acquisition, admission, and
  epoch failures become `ResolveError::Unavailable`
  (`wyrd-auth/src/revocation_resolver.rs:106-156`). Every protected tenant
  extractor reaches this verifier.
- **Material consequence:** a revoked or suspended principal's still-signed
  bearer and its cached permissions remain usable during a database/pool
  outage.
- **Minimum safe correction:** on either revocation error, invalidate the token
  cache entry and return the existing `AuthError::VerifyUnavailable`; reuse the
  existing stable 503 projection. Delete the fail-open tests/branch; add no
  fallback or second cache.
- **Closure proof:** exact cache-hit and cache-miss tests plus one served route
  with an unavailable revocation store all return the stable retryable 503;
  known-current and known-revoked epochs retain their current outcomes.

### `FIND-admin-principals-R4-2` — CONFIRMED — must-fix — revoking a User does not revoke its refresh family

- **Source:** `TREV-R4-1`.
- **Obligation:** `REQ-005`, `INV-013`, `AC-010`, TASK-002, and TASK-005 require
  principal revocation to end authentication on the next request.
- **Exact evidence and callers:** the served route uses one `TenantConn` and
  calls `revoke_principal_in_conn` before commit
  (`wyrd-server/src/auth/revoke.rs:77-102`). Its User arm only advances
  `auth_users.tokens_not_before` (`wyrd-auth/src/revoke.rs:35-40`).
  `RefreshTokens::execute` consumes any active User refresh row, rereads roles,
  and issues a successor at the new time (`refresh.rs:115-156`), without
  checking User status/epoch. That successor is newer than the revocation
  epoch and is admitted.
- **Material consequence:** the holder of a revoked human's refresh token can
  immediately restore access and receive another refresh token.
- **Minimum safe correction:** in the existing User branch and same
  `TenantConn`, reuse `revoke_refresh_family` together with the epoch update.
  Do not add a revocation service or status cache.
- **Closure proof:** a real served human journey obtains access/refresh,
  revokes the User principal, proves the old access token is refused, proves
  refresh cannot mint a successor, and observes no live successor row after
  commit.

### `FIND-admin-principals-R4-3` — CONFIRMED — must-fix — OIDC role removal does not revoke old role claims

- **Source:** `SEC-R4-2`.
- **Obligation:** `INV-013` and the security posture require a role-binding
  revocation to advance the applicable authorization epoch transactionally.
- **Exact evidence and callers:** tenant callback replaces persisted roles and
  then issues a new session in one `TenantConn`
  (`wyrd-auth/src/callback.rs:187-216`). `REPLACE_USER_ROLES_SQL` only
  deletes/inserts join rows (`wyrd-sql/.../role_assignments.rs:33-48`). Old
  access tokens retain signed role names, and verification resolves permissions
  from those names rather than rereading the User's bindings.
- **Material consequence:** after an IdP removes an administrative group and a
  later login persists the reduced set, a stolen pre-change token keeps the
  removed authority until expiry/cache eviction.
- **Minimum safe correction:** make the existing replacement query report
  whether the set changed; only on a real change, advance the User epoch in the
  same transaction, then issue the successor at an `iat` that the verifier
  admits at or after that epoch. Preserve no-op login without needless
  invalidation. Do not add a role cache or alternate role source.
- **Closure proof:** a served OIDC journey captures an admin token, removes the
  group, logs in again, proves the old token fails on its next request, proves
  the successor has only the new roles, and proves an unchanged login does not
  advance the epoch.

### `FIND-admin-principals-R4-4` — CONFIRMED — must-fix — refresh-replay audit loses the consumed credential identity

- **Source:** `SEC-R4-3`.
- **Obligation:** `REQ-048`, `REQ-037`, and `AC-009` require the consumed
  refresh credential to be attributable.
- **Exact evidence:** the replay branch has `stale.id`, revokes the family, and
  builds `auth.refresh.revoke_family`, but passes no credential id
  (`wyrd-auth/src/refresh.rs:162-189`). `auth_event` defaults it to `None`
  (`wyrd-auth/src/audit.rs:47-70`). Successful rotation already carries
  `active.id`, proving no new identifier is needed.
- **Minimum safe correction:** add `with_credential_id(Some(stale.id))` to the
  existing canonical event. Add no detail variant or second event.
- **Closure proof:** the committed replay-denial row names the exact stale UUID
  and the successor family remains unusable from a separate transaction and
  served request.

### `FIND-admin-principals-R4-5` — CONFIRMED / CONSOLIDATED — must-fix — access-token issuance is missing canonical audit on both planes

- **Sources:** `SEC-R4-4` and `SEC-R4-5`.
- **Obligation:** service-identity authority requires credential issuance and
  token exchange to append canonically in the transaction that made the grant;
  `auth::audit` declares `auth.token.exchange` for every access token, and
  `AC-009` requires exact credential attribution.
- **Exact evidence and callers:**
  - Tenant `/auth/token` commits the `TenantConn` after
    `ExchangeApiKey::execute` (`components/auth/routes.rs:125-162`). Card-free
    issuance signs and returns without a write (`exchange_api_key.rs:388-435`);
    Card-bound issuance writes only `auth.card_scope.mint` and does not attach
    the API-key credential id (`:437-504`).
  - `/auth/platform/token` calls `PlatformSessions::exchange`, which authenticates
    and touches through separate pool calls, signs, and returns without audit
    (`platform_sessions.rs:132-162`, `platform_credentials.rs:198-224`). Platform
    OIDC completion calls `issue_federated`, which rereads/signs without audit
    (`platform_login.rs:214-263`, `platform_sessions.rs:164-193`).
- **Material consequence:** tenant or global credentials and federated platform
  identities can mint privileged sessions with no durable issuance record; an
  unavailable audit append cannot fail the grant closed.
- **Minimum safe correction:** tenant exchange appends exactly one existing
  `auth.token.exchange` event on its current `TenantConn` for Card-free and
  Card-bound paths, naming `row.api_key_id`; retain the distinct Card-scope
  event. Platform credential and federated issuance reuse
  `OperatorPool::begin_platform_audited` and existing transaction-scoped
  platform queries so authentication/touch or pin, canonical append, and
  commit form one grant boundary. Credential exchange names its credential;
  federated issuance names the resolved principal and no credential. Add no
  platform audit table or publisher.
- **Closure proof:** real tenant Card-free/Card-bound, platform credential, and
  platform OIDC exchanges each commit one attributed exchange row; an injected
  append failure returns no token and commits none of that grant's side effects.

### `FIND-admin-principals-R4-6` — CONFIRMED — must-fix — initialization can commit a credential that stdout never exposes

- **Source:** `DATA-04-1`.
- **Obligation:** `REQ-021`, `REQ-023`, TASK-003, and `AC-001` require failed
  initialization to remain uninitialized/retryable and never retain an initial
  credential whose plaintext was not exposed.
- **Exact evidence:** `initialize_platform_root` commits at
  `boot/init.rs:109-146`. Only afterward does `main::init` perform four
  `println!` calls, with two preceding the secret (`main.rs:83-93`). A closed
  stdout therefore fails after the unique root and verifier are durable;
  retry then returns `AlreadyInitialized`.
- **Minimum safe correction:** make terminal delivery an explicit fallible
  writer step in the existing initialization workflow. Write and flush the
  one disclosure while the transaction is still open, then commit; a writer
  error drops the transaction. Use `std::io::Write`; do not add an output
  service, queue, temp secret file, or recovery exception.
- **Closure proof:** an injected writer failure leaves zero root principal,
  grant, and credential rows, and the next initialization succeeds and emits
  one credential. A successful writer followed by a database failure may
  expose an unusable secret but must remain retryable, which is the only
  attainable ordering across stdout and PostgreSQL.

### `FIND-admin-principals-R4-7` — CONFIRMED — must-fix — the audit repair weakened every stable-ID schema check

- **Source:** `DATA-04-2`.
- **Obligation:** R3-5 permits only the exact legacy `audit_log` additive
  evolution and forbids widening generic built-in evolution; unrelated schema
  mismatch must remain a refusal.
- **Exact evidence and callers:** every existing physical table reaches
  `schema_shape_matches` through `validate_physical_table`
  (`bifrost_catalog.rs:1156-1179`). After positional comparison fails, the new
  generic fallback compares a complete top-level field-id map and accepts any
  order (`:1490-1550`). Logs/traces and other canonical tables carry those IDs,
  so reordered unrelated physical schemas now pass. The comment claiming
  "nothing else reaches here" is false.
- **Minimum safe correction:** restore strict positional matching as the
  generic validator. Put ID-based reconciliation only behind the existing
  exact `audit_log` legacy-fingerprint gate, validating the known old ids and
  single appended credential field. Delete `fields_by_stable_id` from the
  generic path; do not create a migration framework.
- **Closure proof:** a non-audit canonical table with two stable-ID fields
  swapped returns `MetadataMismatch`; fresh and exact evolved audit layouts
  both validate; the combined R3-5 journey remains green.

### `FIND-admin-principals-R4-8` — CONFIRMED — must-fix — AC-007 proves only one provisioning failure stage

- **Source:** `DATA-04-4`.
- **Obligation:** `AC-007` and TASK-004 explicitly require injected failure at
  each provisioning stage, followed by convergence to one tenant/admin.
- **Exact evidence:** `platform_admin_e2e.rs:3003-3106` installs one trigger on
  `wyrd.auth_service_accounts`; it tests admin-principal insertion only.
  `TenantProvisioning` has distinct durable boundaries for claim/audit commit,
  tenant connection, builtin role seed, principal, grant, credential, tenant
  commit, and active promotion (`provisioning.rs:141-270,417-501`). Abandoned
  and racing-slug cases do not inject those failures.
- **Minimum safe correction:** parameterize the existing real Postgres journey
  over those actual durable boundaries. Reuse SQL triggers/failpoints already
  used by the suite; do not add a production failure-injection framework.
- **Closure proof:** every case leaves the directory failed/non-admitted and no
  usable credential; retry preserves the original tenant id and yields exactly
  one tenant-admin principal, grant, and live credential with no orphan.

### `FIND-admin-principals-R4-9` — CONFIRMED — must-fix — first-party MCP owns a second, incomplete Wyrd HTTP path

- **Source:** `R4-CONTRACT-03`.
- **Obligation:** `REQ-047`, `REQ-048`, `AC-018`, and TASK-008 make
  `wyrd-client` the sole Wyrd HTTP/auth owner; MCP is a consumer.
- **Exact evidence and callers:** the public `WyrdMcpHttpClient` stores a raw
  `reqwest::Client`, assembles `X-Wyrd-Access-Token` and request-id headers, and
  explicitly omits reactive 401 renewal (`wyrd-mcp/src/client.rs:30-86`). All
  five `rmcp::StreamableHttpClient` operations delegate through it (`:89-200`).
  The real first-party MCP journey constructs `reqwest::Client::new()` directly
  (`wyrd-mcp/tests/bifrost/mcp/connectivity.rs:78-98`). This is a shipped public
  client type (`wyrd-mcp/src/lib.rs`), not dead test scaffolding.
- **Material consequence:** proactive expiry works, but the mandated one-time
  durable-credential re-exchange/replay after a 401 does not; HTTP pool/header
  policy has two owners.
- **Minimum safe correction:** keep `rmcp` protocol framing in `wyrd-mcp`, but
  construct the decorator from the existing `WyrdClient`/`HttpTransport`
  connection and auth capabilities, not a caller-supplied raw client. Put the
  bounded 401 classification, `force_refresh`, and single replay in the shared
  client capability used by the adapter. Do not add an MCP facade, protocol
  wrapper, or third transport.
- **Closure proof:** a real adapter test returns 401 on the first MCP request,
  observes exactly one new credential exchange and one replay, succeeds, and
  proves a second 401 is terminal. A source check leaves no independent Wyrd
  header/raw-client construction in the MCP crate.

### `FIND-admin-principals-R4-10` — CONFIRMED / CONSOLIDATED — must-fix — active architecture and public docs describe the removed identity/renewal model

- **Sources:** standards docs finding and `R4-CONTRACT-04`.
- **Obligation:** `REQ-036`, `REQ-040`, `REQ-048`, `AC-013`, and the active
  design/doctrine require public surfaces to describe the shipped contract.
- **Exact evidence:** `architecture/wyrd-design.md:491-506` says API keys are
  exchanged once and then auto-refreshed; `:117-118,624-626` attributes scope
  resolution to refresh. The implementation returns no machine refresh token
  and `AuthMiddleware` re-exchanges the durable credential. Public pages still
  say there are three principal kinds, require Service/Agent Card binding, and
  give API-key exchange a refresh token
  (`concepts/authentication.svx:22-56,103-129`,
  `concepts/identity-and-auth.svx:33-40,89-93`,
  `self-hosting/authentication.svx:13-30`). The principal-kind table in
  `concepts/authorization.svx:82-100` is likewise three-kind. The canonical enum
  has five tags (`wyrd-spec/src/auth/principal_kind.rs:18-54`).
- **Material consequence:** operators and independent clients are told to build
  a refresh flow the server does not issue and cannot discover Card-free or
  plane-scoped administrators.
- **Minimum safe correction:** update the active design and every affected
  public page found by the focused identity/refresh search to the five kinds,
  optional machine Card binding, platform/tenant planes, machine durable-
  credential re-exchange, and human-only refresh rotation. Reuse the existing
  pages; add no compatibility terminology or new guide.
- **Closure proof:** the focused stale-phrase search is empty except true human
  refresh references, generated `llms.txt`/`llms-full.txt` are regenerated, and
  `mise run docs:check` passes.

### `FIND-admin-principals-R4-11` — CONFIRMED / REVISED — must-fix — the operator journey bypasses the returned credential at the shipped CLI boundary

- **Source:** `R4-CONTRACT-05`.
- **Obligation:** `AC-002`, `REQ-040`, and TASK-008 require the real operator
  journey to receive the tenant credential and use it on the subsequent CLI
  configuration/principal calls.
- **Exact evidence:** the journey reads `admin_credential`, then calls the test
  server's in-process `exchange_api_key` and passes the resulting bearer as
  `WYRD_ACCESS_TOKEN` (`wyrd-cli/tests/operator_journey.rs:87-129,151-188`).
  The operator page asks for an unexplained `<tenant access token>` immediately
  after creation (`running-the-server.svx:67-101`). The CLI's existing explicit
  credential path already classifies a supplied secret as API key versus bearer
  and lets `wyrd-client` exchange it (`wyrd-cli/src/client.rs:9-41`), so no new
  CLI option is required.
- **Minimum safe correction:** pass the returned credential directly through
  the existing `--token`/credential input for issuer configuration and
  restricted-principal creation; remove the fixture exchange from this journey.
  Update the page to hand the printed `admin_credential` to that same input.
- **Closure proof:** the real-server CLI journey has no in-process exchange
  between tenant creation and later commands, and both issuer configuration and
  restricted-principal creation succeed with the once-returned value.

### `FIND-admin-principals-R4-12` — CONFIRMED — must-fix — cumulative touched Rust items still violate the mandatory all-item Rustdoc rule

- **Source:** standards Rustdoc finding.
- **Obligation:** `architecture/agent-rules.md` and the repository completion
  standard require accurate Rustdoc on every new/materially modified item,
  including private helpers and tests, with applicable contracts.
- **Exact evidence:** added/materially changed tests lack any Rustdoc at
  `wyrd-auth-issue/src/lib.rs:629-710`, `wyrd-client/src/auth.rs:713-797`,
  `wyrd-auth/src/revoke.rs:110-132,201-202,304-305`, and
  `wyrd-spec/src/auth/principal_kind.rs:62-86`. Several contain `expect`/panic
  paths without the required `# Panics` contract. The recorded strict command
  covers only public `wyrd-sql` docs and cannot inspect these private/test
  items.
- **Minimum safe correction:** document the existing items and their real
  panic/error behavior. Perform a diff-based all-item audit across the
  cumulative range; do not add a lint suppression, helper trait, or permanent
  repository check merely to police this one change.
- **Closure proof:** the diff audit finds no undocumented new/materially
  modified item, the cited tests have accurate docs/sections, and the relevant
  lints/exact tests pass.

### `FIND-admin-principals-R4-13` — CONFIRMED — should-fix — the committed diff fails whitespace hygiene

- **Source:** standards final hygiene result, independently reproduced.
- **Obligation:** repository completion requires the final diff to pass
  `git diff --check`.
- **Exact evidence:** `git diff --check <base> <candidate>` reports added blank
  lines at EOF in:
  - `review/whole-branch-01/task-review.md:298`
  - `review/whole-branch-02/task-review.md:293`
  - `review/whole-branch-03/verdict.md:119`
  - `verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md:180`
  - `TASK-003-binding-projection-and-runtime-activity.md:177`
  - `TASK-005-production-drift-verifier.md:194`
  - `TASK-007-operator-connections-and-delivery.md:229`
- **Minimum safe correction:** delete only the seven extra EOF blank lines.
- **Closure proof:** the exact base-to-candidate `git diff --check` emits no
  output and exits zero.

## Rejected or merged proposals

- A separate round-4 OpenAPI route, error, anonymous-security, or test finding
  is rejected as duplicate. All four facts have the same owner and closure
  boundary, stable `FIND-admin-principals-13`.
- Separate tenant and platform token-exchange audit IDs are rejected as
  duplicate contract roots. They retain distinct subproofs under
  `FIND-admin-principals-R4-5`; no shared service abstraction is requested.
- A second docs finding is rejected as duplicate. Active architecture and all
  affected public pages close together under `FIND-admin-principals-R4-10`.
- The security report's domain-local statement that R3-5 is closed is rejected
  as a whole-change conclusion. The schema/hash implementation exists, but the
  expressly required cross-boundary historical upgrade proof does not.
- The task review's PASS for initialization and all-stage provisioning is
  rejected by direct source/test evidence under `R4-6` and `R4-8`.
- The proposed CLI correction does not earn a new `WYRD_API_KEY`-specific admin
  option: the existing explicit CLI credential already classifies and exchanges
  an API key. The retained gap is proof and documentation of that existing path.
- Optional cleanup, base-only debt, a generic Iceberg migration framework, a
  permanent Rustdoc scanner, and platform-identity CLI expansion are rejected.

## Prior-finding closure at this candidate

| Stable prior ID | Independent disposition |
|---|---|
| `FIND-admin-principals-1` | **CLOSED.** One canonical staging/publisher path remains; reviewed same-plane allowed effects use the audited transaction. |
| `FIND-admin-principals-2` | **CLOSED.** Live provisioning/recovery owners no longer retain `WyrdPostgres`; acquisition remains at the route/state boundary. |
| `FIND-admin-principals-3` | **CLOSED.** Public conflict messages keep physical constraint identifiers server-side. |
| `FIND-admin-principals-4` | **CLOSED for the implementation authority.** The two planes and closed kind enum are implemented. Public docs describing three kinds remain a new projection drift root, `R4-10`. |
| `FIND-admin-principals-8` | **CLOSED.** Last-admin protection counts usable credentials or pinned identities under serialization. |
| `FIND-admin-principals-13` | **OPEN / REVISED.** Duplicate YAML/snapshot machinery is gone and many admin paths are typed, but runtime route and reachable-error coverage is incomplete and its tests use parallel lists. |
| `FIND-004-3` | **CLOSED.** Provisioning retry retires undisclosed prior credentials and returns one usable replacement. |
| `FIND-005-1` | **CLOSED.** Issue-key allowance, effect, issuance evidence, and commit use one `TenantConn`. |
| `FIND-003-2` | **CLOSED for process/repeat proof.** The newly found stdout-before-commit ordering is the distinct `R4-6` failure of `REQ-023`. |
| `FIND-004-5` | **CLOSED for configuration ordering and recovery docs.** The journey configures an issuer before creating a restricted principal. Direct use of the returned credential at the CLI boundary remains the narrower new proof gap `R4-11`. |
| `FIND-TASK-001-10` | **WAIVED IN FULL by the owner.** Not reopened; no provenance change is requested. |
| `FIND-admin-principals-R2-2` | **CLOSED.** Stored platform kind reaches session, context, and audit. |
| `FIND-admin-principals-R2-3` | **CLOSED for machine/human renewal separation and successful-rotation attribution.** Role-change epoch and principal-revocation interaction are distinct `R4-3` and `R4-2`. |
| `FIND-admin-principals-R2-4` | **CLOSED for durable family replay containment.** The missing stale credential on that committed event is the narrower `R4-4`. |
| `FIND-admin-principals-R2-5` | **CLOSED for per-request tenant admission.** Resolver uncertainty itself fails open and is the distinct `R4-1`. |
| `FIND-admin-principals-R2-6` | **CLOSED.** Invalid tenant keys converge on one real/dummy verification. |
| `FIND-admin-principals-R3-1` | **CLOSED.** Platform operations pass exact resources; tenant create uses the requested slug. |
| `FIND-admin-principals-R3-2` | **CLOSED.** The canonical parsed issuer is persisted and trailing-slash first login is covered. |
| `FIND-admin-principals-R3-3` | **CLOSED.** Existing redacted secret types own wire and CLI secret handling. |
| `FIND-admin-principals-R3-4` | **CLOSED only for its previously named items.** New/materially changed undocumented items are a genuinely new cumulative-diff violation, `R4-12`. |
| `FIND-admin-principals-R3-5` | **OPEN / REVISED TO PROOF GAP.** The narrow fingerprint/hash implementation exists; the mandatory historical upgrade/restart journey does not. The generic schema fallback additionally introduced separate regression `R4-7`. |
| `FIND-admin-principals-R3-6` | **OPEN.** Appended evidence still omits reproducible exact commands for multiple named proofs. |

## Verification performed by this validator

- Inspected the complete Wave-1 proposal set, the cumulative diff inventory,
  current source, every reachable caller/body cited above, all eight task
  packets, the approved spec, and prior R3 validation/verdict/remediation and
  implementation evidence.
- Reproduced `git diff --check` and the seven failures recorded in `R4-13`.
- Did not rerun Cargo/Postgres/Bifrost suites: this phase validates static
  findings and recorded command evidence; it does not substitute an aggregate
  lane for the exact proofs that remain missing.
- `mise run gate` is not requested; approved `VER-003` forbids using broad
  aggregates as acceptance evidence for this change.

## Final integrity recheck

After this report was written:

- `git rev-parse HEAD` remained
  `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`.
- `shasum -a 256 changes/active/admin-principals/spec.md` remained
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.

This validator changed no candidate/product source and did not touch the dirty
owner files.
