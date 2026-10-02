# Structured Ponytail validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Approved authority: `SPEC-oidc-production-readiness` revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`

The complete cumulative diff, all eight discovery/follow-up reports, the
applicable repository authorities, every cited owner, and the relevant callers,
consumers, writers, and tests were inspected. CodeGraph was unavailable because
the repository has no `.codegraph/` index. No Cargo-backed command was run, as
directed. The candidate remained the checked-out `HEAD` throughout validation.

## Source-ID dispositions

| Source ID | Disposition | Final finding | Validation |
|---|---|---|---|
| `BEH-003-01` | **CONFIRMED** | `FIND-TASK-003-2` | `BrowserSessions::exchange_api_key` returns before the shared fixed-cost verifier for malformed and route/key-tenant-mismatched input. |
| `BEH-003-02` | **CONFIRMED** | `FIND-TASK-003-3` | The chooser is rendered from cookie-name suffixes. Follow-up correctly narrows the consequence: later server validation prevents authority transfer, but the task separately prohibits the selector itself from being browser-derived. |
| `BEH-003-03` | **REVISED** | `FIND-TASK-003-4` | Consolidated with every rotation proposal. The defect includes both unsafe retirement and the keyless boot inventory omitting persisted browser ciphertext. The existing rewrap owner, not a retention-only documentation workaround, is the smallest complete correction. |
| `BEH-003-04` | **CONFIRMED** | `FIND-TASK-003-5` | The service key and session credentials can be sent to an arbitrary non-loopback `http://` origin. |
| `BEH-003-05` | **CONFIRMED** | `FIND-TASK-003-6` | The required AC-003/AC-006 browser journeys are absent from the only real BFF journey. |
| `INVREV-001` | **CONFIRMED** | `FIND-TASK-003-5` | Duplicate transport cause. |
| `INVREV-002` | **REVISED** | `FIND-TASK-003-4` | Duplicate rotation cause; correction is consolidated at `SealedSecretRewrap`. |
| `INVREV-003` | **CONFIRMED** | `FIND-TASK-003-6` | Duplicate journey-coverage cause. |
| `STD-001` | **CONFIRMED** | `FIND-TASK-003-7` | All three cited documentation defects violate the repository's explicit hard rule. |
| `MAINT-001` | **CONFIRMED** | `FIND-TASK-003-7` | Duplicate of the `config.rs` portion of `STD-001`. |
| `MAINT-002` | **REVISED** | `FIND-TASK-003-8` | Follow-up proves the empty identifier is produced on every authenticated production request, while current dangerous mock/storage consumers are guarded off. The retained defect is the missing required private-channel field and invalid typed domain value, not a demonstrated cross-tenant access. |
| `MAINT-003` | **CONFIRMED** | `FIND-TASK-003-9` | Repository-wide caller inspection finds no construction of `SessionLifetime::Until`; it exists only to support a dead SQL branch. |
| `SYSTEM-001` | **REVISED** | `FIND-TASK-003-4` | The diagnosis is confirmed. Its retention-only correction is insufficient because `REQ-005` also makes the keyring mandatory while browser ciphertext persists, yet keyless boot uses the canonical rewrap inventory and currently sees none of those rows. |
| `SEC-ID-001` | **REVISED** | `FIND-TASK-003-1` | The mismatch defect is confirmed. Because Wyrd intentionally uses one callback for multiple issuers, the correction must provide the complete RFC 9700 mix-up defense, not merely compare `iss` when convenient. |
| `SEC-ID-002` | **CONFIRMED** | `FIND-TASK-003-4` | Duplicate rotation cause. |
| `PC-001` | **CONFIRMED** | `FIND-TASK-003-4` | Duplicate rotation cause. |
| Follow-up chooser resolution | **RESOLVED / CONFIRMED** | `FIND-TASK-003-3` | The cookie suffix reaches rendered selector state but not effective tenant authority. |
| Follow-up tenant-id resolution | **RESOLVED / CONFIRMED** | `FIND-TASK-003-8` | The empty tenant id is reachable typed state; current production guards prevent a demonstrated storage or authorization effect. |

No proposal was rejected outright. Duplicate reports were collapsed by owning
cause. No unresolved disagreement or incomplete source trace remains.

## Final deduplicated finding ledger

### FIND-TASK-003-1 — Authorization responses are not bound to their issuer

- **Source IDs:** `SEC-ID-001`
- **Status:** **REVISED**
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-007 and INV-004 require an issuer-bound,
  fail-closed OIDC flow. Wyrd supports multiple authorization servers through
  one common callback, so RFC 9700 section 4.4.2 requires a mix-up defense;
  RFC 9207 section 2.4 requires exact validation of the response issuer and
  refusal before proceeding with the grant.
- **Exact locations:**
  `crates/wyrd-spec/src/auth/oidc.rs:400-414,634-655`;
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:327-345`;
  `crates/wyrd/wyrd-auth/src/login.rs:114-120`;
  `crates/wyrd/wyrd-auth/src/callback.rs:75-165`;
  `crates/shared/wyrd-auth-oidc/src/provider.rs:21-79`.
- **Producer-to-consumer evidence:** `HumanConnections::begin_login` stores the
  selected issuer in `LoginState`. `CallbackQuery` deliberately discards the
  RFC 9207 `iss` parameter, the route forwards only `code` and `state`, and
  `AuthorizationCodeExchange` therefore cannot compare the response issuer
  before sending the code and client authentication to the state-selected
  token endpoint. Later ID-token issuer verification occurs after that
  disclosure boundary and is not a substitute. The candidate's regression
  test positively requires the unsafe discard. RFC 9207 requires rejection of
  a present mismatch; RFC 9700 requires a mix-up defense for this multi-issuer,
  single-callback topology.
- **Observable consequence:** a callback that identifies a different issuer is
  accepted far enough to attempt token exchange against the state-selected
  server, allowing an authorization-server mix-up path to disclose a code or
  client credential or bind the wrong authorization server.
- **Decision-complete smallest correction:** preserve tolerance for unrelated
  provider parameters such as `session_state`, but model the standard `iss`
  parameter and reuse the already-persisted `LoginState.issuer`. Extend the
  existing provider metadata projection with
  `authorization_response_iss_parameter_supported`; because the approved
  common callback rules out distinct per-issuer redirect URIs, connection
  testing/activation must refuse a human provider that cannot supply issuer
  identification. `AuthorizationCodeExchange` must require `iss`, compare it
  by exact string equality with the bound issuer after state resolution and
  before token-endpoint IO, and audit/refuse a missing or mismatched value
  without contacting any token endpoint. This adds no second callback or
  verifier and keeps the comparison in the existing exchange owner.
- **Focused closure proof:** contract tests accept unrelated parameters while
  retaining typed `iss`; provider qualification refuses absent issuer-response
  support; matching `iss` completes; missing/mismatched `iss` spends the state,
  records the refusal, performs zero token-endpoint calls, and establishes no
  completion or browser session. Regenerate the callback schemas.

### FIND-TASK-003-2 — Browser API-key refusals bypass fixed-cost verification

- **Source IDs:** `BEH-003-01`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** REQ-010 requires reuse of the existing API-key
  exchange, and `architecture/wyrd-security-posture.md` requires every invalid
  tenant API-key condition to perform exactly one verification and return one
  indistinguishable refusal.
- **Exact locations:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:261-290`;
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:154-229`;
  `crates/wyrd/wyrd-auth/src/credential_verify.rs:21-68`;
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:135-173`.
- **Producer-to-consumer evidence:** `exchange_api_key` cheaply parses the key
  and returns for malformed input or a route/key tenant mismatch before
  `ExchangeApiKey::execute`. The shared exchange owner routes malformed,
  cross-tenant, unknown-prefix, and wrong-secret values through exactly one
  `verify_presented` call, and the public token route explicitly pays the dummy
  verification when it cannot open a tenant connection. The BFF handler maps
  all results to the same status, but it cannot erase the timing distinction.
- **Observable consequence:** malformed and route-mismatched credentials are
  measurably cheaper than live-prefix or correctly shaped invalid keys, so the
  browser-login boundary leaks credential class by clock.
- **Decision-complete smallest correction:** resolve the route tenant first.
  When it exists, open that tenant's `TenantConn` and send the presented key
  directly through the existing `ExchangeApiKey::execute`; its shared
  `verify_api_key` owner already performs the parse, tenant comparison, row
  lookup, and one real/dummy verification. When the route names no tenant and
  no connection can be opened, reuse `verify_presented(..., None)` exactly as
  the public token route does before returning the same refusal. Only a
  successful exchange may create a session. Do not add a second parser or
  verifier and do not verify any input twice.
- **Focused closure proof:** around each browser exchange, assert
  `verifications_performed()` advances exactly once for malformed,
  unknown-route, cross-tenant, unknown-prefix, wrong-secret,
  expired/revoked, and valid keys; every invalid HTTP response remains the
  same `401`, and only the valid route/key pair creates a session.

### FIND-TASK-003-3 — The tenant chooser renders unverified cookie hints

- **Source IDs:** `BEH-003-02`; follow-up chooser resolution
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** TASK-003 explicitly prohibits a tenant selector
  based on untrusted browser data; REQ-015 requires switching to revalidate an
  independent target-tenant session.
- **Exact locations:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:183-213,261-295`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/components/app/Shell.svelte:80-100`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/components/app/TenantChooser.svelte:1-24`.
- **Producer-to-consumer evidence:** `ServerSessions.metadata` enumerates every
  cookie name with the session prefix, validates only the suffix syntax, and
  publishes the suffix as a tenant. `Shell` and `TenantChooser` render it as a
  selectable option. `switch` subsequently calls the existing server-backed
  `read(target)`, so a forged option does not become authority; that correct
  downstream guard does not satisfy the separate prohibition on an
  untrusted-data selector.
- **Observable consequence:** a forged or stale
  `wyrd_session_victim=<anything>` cookie makes `victim` appear as tenant state
  to a signed-in user, then fails only after selection.
- **Decision-complete smallest correction:** keep cookie names only as lookup
  hints and reuse `ServerSessions.read` to resolve each distinct hinted tenant
  before it enters `SessionMetadata`. Render the server-returned key/name only;
  omit and clear unknown, expired, or cross-tenant hints. Keep the current
  server-backed `switch` check; do not add a UI membership store or a new
  endpoint.
- **Focused closure proof:** with one valid current session plus forged,
  expired, duplicate, and cross-tenant-named cookies, load a real page across
  both replicas and prove that only independently server-verified sessions
  render; invalid hints are cleared/omitted, and a verified second session
  renders and switches successfully.

### FIND-TASK-003-4 — Sealing-key rotation omits durable browser-session ciphertext

- **Source IDs:** `BEH-003-03`, `INVREV-002`, `SYSTEM-001`, `SEC-ID-002`,
  `PC-001`
- **Status:** **REVISED**
- **Classification:** `INCORRECT`
- **Violated obligation:** REQ-005 requires the deployment keyring whenever
  recoverable browser credentials persist, requires missing key material to
  refuse the affected flow, and requires rotation without stranding existing
  sessions; REQ-018 and AC-007 require a truthful procedure and rotated-key
  proof.
- **Exact locations:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:230-243,292-305,320-356,389-505,541-545`;
  `crates/wyrd/wyrd-sql/migrations/20261001000001_auth_browser_sessions.sql:8-56`;
  `crates/wyrd/wyrd-auth/src/sealing.rs:1-29,73-118`;
  `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:419-468`;
  `docs/src/content/docs/self-hosting/authentication.svx:63-88`;
  `docs/src/content/docs/self-hosting/sso-and-oidc.svx:21`.
- **Producer-to-consumer evidence:** session creation writes access,
  refresh/API-key, and CSRF envelopes. Renewal only reseals access and the SSO
  refresh token; it never reseals CSRF or the API-key bootstrap credential.
  `SealedSecretRewrap` inventories only human connections, trusted issuers,
  and the platform connection, so both its `remaining == 0` retirement signal
  and its keyless-boot inventory ignore every browser-session envelope. After
  K1 is removed, `read/current/renew/logout` cannot open those fields. The
  published runbook nevertheless says the provider-only zero count plus the
  two-minute completion window means no stored secret needs K1.
- **Observable consequence:** an operator following the supported rotation
  procedure can end every pre-roll UI session across tenants and skip clean
  refresh-family revocation. A deployment can also boot without a keyring
  while recoverable browser ciphertext still persists, contradicting the
  keyring condition fixed by REQ-005.
- **Decision-complete smallest correction:** extend the existing
  `SealedSecretRewrap` plus its existing operator SQL/CAS mechanism to inventory
  and rewrap every non-null `auth_browser_sessions` sealed column. Each
  column's exact old bytes must fence its swap; a concurrent renewal or logout
  that wins causes `remaining` and is repaired by the next canonical pass.
  Include these values in the same report and keyless-boot decision, rather
  than creating a session-specific rotation engine. Update the existing
  authentication/SSO guidance and rewrap log contract so `remaining == 0`
  truthfully covers provider and browser-session stores; keep only the
  short-lived login-completion exception. A documentation-only 12-hour
  retention rule is rejected because it leaves the canonical keyless inventory
  false while ciphertext rows persist.
- **Focused closure proof:** create live OIDC and API-key browser sessions under
  K1; run the canonical pass with K2+K1, including a concurrent renewal/logout
  CAS race; require a subsequent pass to reach zero; restart with K2 only and
  prove both modes can read, renew/act, pass CSRF, and perform their mode-specific
  logout behavior. A keyless boot must refuse while any non-null session
  envelope remains. The existing real two-replica BFF journey must record the
  supported rotation path.

### FIND-TASK-003-5 — The private BFF credential channel permits non-loopback plaintext

- **Source IDs:** `BEH-003-04`, `INVREV-001`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** TASK-003 fixes a BFF service-key channel over TLS on
  a private route; REQ-005/009 and the deployment authority confine plaintext
  to loopback development or an authenticated local transport.
- **Exact locations:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:3-6`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:75-93,154-180,189-213,298-329`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.test.ts:1-22`.
- **Producer-to-consumer evidence:** `serverUrl()` accepts any string and the
  channel sends the raw deployment key there. The same origin carries raw flow
  and session ids, the bootstrap API key, CSRF token, and access-token
  responses. The current unit test positively configures
  `http://wyrd.internal:8080`; only the real journey's loopback HTTP topology
  is an approved plaintext exception.
- **Observable consequence:** a production misconfiguration can send the BFF
  key and recoverable tenant session authority in cleartext between pods or
  processes. The service key authenticates the caller but provides no
  confidentiality or server authentication.
- **Decision-complete smallest correction:** validate the existing
  `WYRD_SERVER_URL` once at the shared upstream boundary using the native URL
  parser. Require `https:` for every non-loopback hostname; allow `http:` only
  for `localhost` and literal loopback addresses used by local/test topology.
  All `ServerSessions` calls continue to reuse that one origin and HTTP client;
  add no bypass flag, second transport, or separate BFF URL.
- **Focused closure proof:** boundary tests prove HTTPS is accepted, loopback
  HTTP remains usable, and non-loopback HTTP is rejected before the supplied
  fetcher observes a request. Retain the loopback real BFF journey and add a
  production-shaped TLS channel exercise.

### FIND-TASK-003-6 — Required multi-provider and replacement browser journeys are absent

- **Source IDs:** `BEH-003-05`, `INVREV-003`
- **Status:** **CONFIRMED**
- **Classification:** `MISSING`
- **Violated obligation:** TASK-003 explicitly requires AC-003 and AC-006
  browser journeys, scenarios 2/3 require verified switching and settings
  actions, and `AGENTS.md` makes the user journey the primary contract for a
  user-facing capability.
- **Exact locations:**
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:1-13,145-249`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:185-375`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/settings/+page.server.ts:96-141`;
  `mise.toml:576-615`.
- **Producer-to-consumer evidence:** the only production BFF host activates one
  Keycloak tenant and seeds one OIDC-off tenant. The switch test reaches only
  the no-target-session branch. Settings cross the BFF boundary for stage and
  deactivate only; test, activate, remove, replacement login, recovery
  preservation, and non-inheritance do not. The lane already starts both
  Keycloak and Dex, so a second harness or dependency is unnecessary.
- **Observable consequence:** the successful two-session switch, different-
  provider tenant isolation, same-issuer cross-tenant refusal, and three
  exposed settings actions can regress while every claimed UI check remains
  green. Server-only TASK-002 tests do not exercise these BFF form/cookie seams.
- **Decision-complete smallest correction:** extend the existing
  `identity_ui_e2e` host and existing real HTTP Vitest journey; reuse the
  repository-managed Keycloak and Dex providers and the same two BFF replicas.
  Establish independently authenticated sessions for two active provider
  tenants in one browser, take the successful switch branch, and cover wrong-
  tenant plus same-issuer cross-tenant refusal. Through the public settings
  actions, test and activate a replacement, preserve an authorized recovery
  route, log in through the replacement without inheriting the old principal's
  authority/email identity, and remove/retire the old connection. Retain the
  existing OIDC-off, stage, deactivate, denial, replica, and leak checks.
- **Focused closure proof:** add exact named Vitest scenarios for the
  multi-provider switch and provider replacement/settings paths, run them
  through `WYRD_IDENTITY_TARGET=ui`, then run the unfiltered identity journey
  to prove all earlier Rust and UI journeys remain selected.

### FIND-TASK-003-7 — Mandatory rustdoc is missing or attached to the wrong item

- **Source IDs:** `STD-001`, `MAINT-001`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` make substantive rustdoc mandatory for every
  new or materially modified Rust item, including private and trait methods;
  missing documentation is a hard merge blocker.
- **Exact locations:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:132-135`;
  `crates/wyrd/wyrd-server/src/components/auth/bff.rs:167-174`;
  `crates/wyrd/wyrd-server/src/config.rs:3505-3531`.
- **Producer-to-consumer evidence:** the new `Debug::fmt` and `From::from`
  methods have no rustdoc. The pre-existing `env_opt` description now sits
  above `parse_bff_service_key_hashes`, falsely saying the parser reads an
  environment variable, while `env_opt` is left undocumented. These are all
  changed items and the explicit rule includes trait methods.
- **Observable consequence:** the candidate fails a hard repository acceptance
  rule, and navigation at two secret-bearing/configuration boundaries describes
  the wrong operation or none at all.
- **Decision-complete smallest correction:** add concise workflow-specific
  rustdoc to the two trait methods; move the two-line environment-reading
  contract back to `env_opt`; leave only the hash-list and rotation-overlap
  contract on `parse_bff_service_key_hashes`. Add no wrapper or documentation
  abstraction.
- **Focused closure proof:** the touched crates' rustdoc/lint coverage and
  `mise run fmt` pass, and the existing BFF hash parser test remains green.

### FIND-TASK-003-8 — The production session projection fabricates an empty tenant id

- **Source IDs:** `MAINT-002`; follow-up tenant-id resolution
- **Status:** **REVISED**
- **Classification:** `MISSING`
- **Violated obligation:** TASK-003's packet-local `SessionRead` contract
  includes `tenant_id`; repository rules require domain values rather than
  placeholders and require the UI to project server-owned identity.
- **Exact locations:**
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:315-356`;
  `crates/wyrd/wyrd-server/src/components/auth/bff.rs:227-271`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:14-34,183-227`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/session.ts:8-27,119-137`.
- **Producer-to-consumer evidence:** `BrowserSessions::read` already receives
  the authoritative `DataTenantId` from `current`, then drops it from
  `BrowserSessionView`; `ReadResponse`, TypeScript `Read`, and `ServerSession`
  cannot project it. `context` still promises a complete `TenantContext` and
  inserts `tenantId: ''` on every authenticated production request. Current
  production guards prevent that sentinel from reaching mock tenant-keyed
  storage or selecting authority, so the consequence is kept to the violated
  typed contract and latent invalid state.
- **Observable consequence:** a valid domain type contains a false tenant
  identity, so a future normal transport implementation or reordered guard can
  consume an empty durable key without the type system exposing the mistake.
- **Decision-complete smallest correction:** carry the already-known
  `DataTenantId` through `BrowserSessionView`, the existing internal
  `ReadResponse`, TypeScript `Read`, and `ServerSession`, then populate
  `TenantContext.tenant.tenantId` from it. Keep the id in the server-only
  session projection and out of `SessionMetadata`; add no second tenant lookup.
- **Focused closure proof:** a focused `ServerSessions` test asserts that the
  private read's tenant id reaches `context`, while page-data and real-journey
  leak checks prove it is not added to browser metadata.

### FIND-TASK-003-9 — `SessionLifetime::Until` is dead speculative capability

- **Source IDs:** `MAINT-003`
- **Status:** **CONFIRMED**
- **Classification:** `DRIFT`
- **Violated obligation:** the repository's Ponytail/abstraction rules prohibit
  branches and configuration surfaces with no current caller.
- **Exact locations:**
  `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:29-44,126-134,224-252`;
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:230-243,292-305`;
  `crates/wyrd/wyrd-sql/src/queries/auth/mod.rs:27-30`.
- **Producer-to-consumer evidence:** the only two constructors both use
  `SessionLifetime::For`; repository-wide symbol search finds no construction
  of `Until`. The unused variant alone creates a two-source expiry model,
  `COALESCE`, two optional binds, a public re-export, and comments describing
  behavior that the SSO owner does not use.
- **Observable consequence:** maintainers must reason about impossible nullable
  bind combinations and a producer-owned absolute-expiry path that cannot run.
- **Decision-complete smallest correction:** delete the enum and its re-export;
  make `BrowserSessionWrite` carry the one real fixed `Duration`; compute
  absolute expiry only from PostgreSQL's clock plus that duration; preserve the
  independently stored refresh-token expiry. Add no replacement abstraction.
- **Focused closure proof:** static caller inspection shows no `SessionLifetime`
  symbol remains, the SQL insert has one non-null lifetime bind, and the
  existing SSO/API-key session journeys plus the owning SQL lane remain green.

## Validation result

**FIX_REQUIRED** — nine bounded findings remain. None requires a new product,
public API, architecture, security, concurrency, or persistent-data decision;
each correction is available within the approved task using an existing owner
or native mechanism. The validated ledger is complete and unblocked.
