# TASK-002 Wave 2 Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `87de451ed87ad059cefd579eb15ef4b028a92547`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Candidate checked during validation: `87de451ed87ad059cefd579eb15ef4b028a92547`

## Validation result

**FIX_REQUIRED.** The seven distinct issues proposed by Wave 1 are reachable
and required by the approved task or mandatory repository rules. The two SQL
boundary issues were independently reported twice and are deduplicated below.
No retained correction requires a specification revision: each reuses an
existing owner, contract, or canonical mechanism and preserves adjacent
workload, platform-login, issuance, RLS, and audit behavior.

## Wave 1 disposition

| Wave 1 finding | Disposition | Final finding | Validation summary |
|---|---|---|---|
| `TR-001` | **REVISED** | `FIND-TASK-002-1` | The configured human subject path reaches the durable identity key. The correction must also fail closed for already-stored human connections, not only reject new request bodies. |
| `TR-002` | **REVISED** | `FIND-TASK-002-2` | Multi-audience ID tokens lack `azp` enforcement. The check belongs on the human OIDC login path so workload assertions are not silently subjected to ID-token-only semantics. |
| `SEC-001` | **REVISED** | `FIND-TASK-002-3` | Provider-driven role replacement is reachable on every successful login and has no role-change event. Existing canonical `AuditEvent` and append behavior are sufficient; no new sink or mandatory new detail schema is needed. |
| `SEC-002` | **CONFIRMED** | `FIND-TASK-002-4` | The callback rediscovers the provider's allowed ID-token algorithms but verification selects the untrusted JWT header algorithm without comparing it to that set. |
| `STD-001` | **CONFIRMED** | `FIND-TASK-002-5` | Duplicate of `TD-002`; one retained RLS-ownership finding. |
| `TD-002` | **CONFIRMED** | `FIND-TASK-002-5` | Duplicate of `STD-001`; one retained RLS-ownership finding. |
| `STD-002` | **REVISED** | `FIND-TASK-002-6` | Duplicate of `TD-001`; the existing `WyrdPostgres` owner can expose the one narrow app-role operation without a raw-pool signature. |
| `TD-001` | **REVISED** | `FIND-TASK-002-6` | Duplicate of `STD-002`; one retained connection-capability finding. |
| `STD-003` | **CONFIRMED** | `FIND-TASK-002-7` | Both public state structs derive `Debug` over a plain verifier string; the previous owner used `SecretString`, and `wyrd-sql` already has `secrecy`. |

## Validated finding ledger

### FIND-TASK-002-1 — Human identity can use a mutable mapped claim instead of OIDC `sub`

- **Wave 1 IDs:** `TR-001`
- **Status:** REVISED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-008, REQ-014, INV-002, AC-006, and the task's
  `(issuer, subject)` identity and no-email-linking contract.
- **Exact location:**
  `crates/wyrd-spec/src/auth/human_connection.rs:216-241`,
  `crates/shared/wyrd-auth-oidc/src/claims.rs:38-62`,
  `crates/shared/wyrd-auth-verify/src/lib.rs:552-562`, and
  `crates/wyrd/wyrd-auth/src/callback.rs:222-251,496-513`.
- **Reachability and caller trace:** `ConnectionInput::from_slice` is the
  tenant connection write boundary and accepts every nonempty
  `claim_mapping.subject`. `human_connection_trusted_issuer` rebuilds that
  stored mapping unchanged. `AuthorizationCodeExchange::execute` reaches
  `finish_id_token_exchange` for every successful tenant callback;
  `verify_external_against` calls `map_claims`, which places the configured
  claim value in `verified.subject`; the callback passes it to
  `ensure_user_identity`, whose complete body looks up and upserts the durable
  `(issuer, subject)` identity. Its only production caller is this callback.
  Therefore `subject: "email"` can merge two different provider `sub` values
  at one issuer, while a mutable mapped claim can split one person.
- **Observable consequence:** email or another mutable/ambiguous claim becomes
  identity authority, allowing a distinct provider subject to inherit an
  existing tenant User's roles and history or causing one subject to become a
  second User after the claim changes.
- **Smallest safe correction:** keep configurable subject mapping for the
  separate workload issuer contract, but require the tenant human-connection
  contract to use exact `sub`. Enforce this at `ConnectionInput` validation and
  at the stored human-connection decode/upgrade boundary so a legacy row with
  another path fails closed rather than reaching login. Continue to allow
  independently configured email and group paths. Reuse the existing claim
  mapper and identity SQL; do not create a second human mapper or link users.
- **Preserved adjacent behavior:** workload claim mappings, optional display
  email, group-to-role mapping, platform identity pinning, issuer separation,
  and the existing race-safe identity upsert remain unchanged.
- **Focused closure proof:** contract coverage rejects human
  `subject: "email"` and accepts `subject: "sub"`; upgrade/decode coverage
  refuses a stored non-`sub` human mapping; a callback or real-server case with
  one issuer, different signed `sub` values, and the same email produces two
  Users with no authority transfer.

### FIND-TASK-002-2 — Human ID-token audience validation omits authorized-party semantics

- **Wave 1 IDs:** `TR-002`
- **Status:** REVISED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-007, INV-004, and AC-007.
- **Exact location:**
  `crates/shared/wyrd-auth-verify/src/lib.rs:506-563` and
  `crates/wyrd/wyrd-auth/src/callback.rs:212-251`.
- **Reachability and caller trace:** `verify_external_against` has three
  production paths: tenant human callback, platform human login, and workload
  federation through `verify_external`. Its complete body lets
  `jsonwebtoken` establish only that `expected_audience` occurs in `aud`, then
  returns raw claims without reading `azp`. The tenant callback adds only nonce
  validation before identity and issuance. A signed token with
  `aud: [wyrd-client, other-client]` and missing or mismatched `azp` therefore
  reaches `ensure_user_identity` and `issue_human_session`.
- **Observable consequence:** a correctly signed ID token for another
  authorized party can establish a Wyrd tenant session when Wyrd appears only
  as an additional audience.
- **Smallest safe correction:** add OIDC authorized-party validation to the
  existing tenant human callback verification flow after generic JWT
  verification and before identity resolution: multiple audiences require a
  string `azp` equal to the bound client id, and any present `azp` must equal
  that client id. Reuse `verified.raw_claims` and the connection-bound client
  id. Do not impose ID-token `azp` rules on workload JWT assertions through
  the shared generic verifier.
- **Preserved adjacent behavior:** issuer/signature/time/audience validation,
  platform login, workload assertion semantics, nonce checking, and callback
  transactional behavior remain unchanged.
- **Focused closure proof:** callback-focused cases reject missing and
  mismatched `azp` on a multi-audience signed token, reject mismatched `azp`
  when present on a single-audience token, accept matching `azp`, and prove a
  refusal writes neither completion nor refresh row.

### FIND-TASK-002-3 — Provider-driven role changes lack canonical role-change evidence

- **Wave 1 IDs:** `SEC-001`
- **Status:** REVISED
- **Classification:** MISSING
- **Violated obligation:** REQ-017 and the task requirement that role changes
  and successful issuance are audited in the owning transaction.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/callback.rs:236-270`,
  `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:116-144`, and
  `crates/wyrd/wyrd-auth/src/issuance.rs:404-527,583-631`.
- **Reachability and caller trace:** every successful tenant OIDC callback maps
  groups and calls `replace_user_roles`. That function's complete body replaces
  the durable assignment but returns no change result. The callback then calls
  `issue_human_session`; its `issue` path appends only
  `auth.token.exchange`, whose `TokenExchange` detail contains principal,
  delegation, and expiry but no fact that roles changed. Other
  `replace_user_roles` callers are tests or refresh test setup; the production
  mutation is this callback. Mapping changes and provider group changes make
  this path observably reachable.
- **Observable consequence:** retained audit can show a token exchange but
  cannot distinguish a login that granted or revoked durable tenant roles from
  one whose assignments were unchanged.
- **Smallest safe correction:** have the existing role-assignment owner report
  whether its set replacement changed durable membership. When it did, append
  one redacted canonical event with stable operation
  `auth.user.roles.sync`, the User principal resource, and allowed outcome
  through the existing `append_auth_audit` path before the callback transaction
  commits. The existing `AuditEvent` supports detail-free events; do not add a
  new sink, publisher, table, or speculative role-history subsystem. An audit
  append failure must roll back the role update, token/refresh issuance, and
  completion together.
- **Preserved adjacent behavior:** the ordinary token-exchange event remains;
  unchanged role sets do not claim a mutation; refresh reads the same durable
  roles; canonical staging and the single publisher remain authoritative.
- **Focused closure proof:** callback/Postgres coverage proves a changed set
  emits exactly one role-sync event plus the ordinary exchange event, an
  unchanged set emits no role-sync event, and injected role-sync append failure
  leaves assignments, refresh row, and completion uncommitted.

### FIND-TASK-002-4 — ID-token algorithm is not constrained to the discovered provider set

- **Wave 1 IDs:** `SEC-002`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-007, INV-004, and
  `architecture/wyrd-security-posture.md:168-170`.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/provider.rs:20-78,137-170`,
  `crates/shared/wyrd-auth-verify/src/lib.rs:514-550`, and
  `crates/wyrd/wyrd-auth/src/callback.rs:163-184,222-227`.
- **Reachability and caller trace:** `AuthorizationCodeExchange::complete`
  rediscovers the provider and receives
  `id_token_signing_alg_values_supported`, then discards that field when it
  calls `finish_id_token_exchange`. The latter invokes
  `verify_external_against`; its complete body rejects HMAC but constructs
  `Validation::new(header.alg)` from the untrusted JWT header. The same shared
  verifier is also used by platform login and workload federation, so a broad
  hard-coded algorithm change could alter adjacent contracts. An RSA JWKS key
  can support more than one RSA signing mode, making a provider-unadvertised
  asymmetric header path reachable.
- **Observable consequence:** Wyrd can accept a validly signed tenant ID token
  under an asymmetric algorithm the provider did not advertise for ID tokens,
  widening the qualified trust policy.
- **Smallest safe correction:** on the existing tenant callback path, carry
  the freshly discovered supported ID-token algorithm set into the verification
  step and reject a header algorithm not in the supported asymmetric
  intersection before identity resolution. Reuse the provider metadata and
  current verifier/JWKS owner; do not add administrator configuration or a
  parallel verifier. Keep the shared workload verifier's policy unchanged
  unless its own authority separately requires an algorithm contract.
- **Preserved adjacent behavior:** screened discovery, JWKS rotation,
  signature/issuer/audience/time verification, platform login, and workload
  federation remain intact.
- **Focused closure proof:** a callback/verifier case with discovery advertising
  `RS256` rejects a valid JWKS-compatible token signed with an unadvertised
  asymmetric algorithm, accepts `RS256`, and proves rejection occurs before
  User, refresh, or completion persistence.

### FIND-TASK-002-5 — TenantConn login-state queries duplicate the RLS tenant boundary

- **Wave 1 IDs:** `STD-001`, `TD-002`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` requires
  `TenantConn`/forced RLS to be the single tenant-selection boundary and bans
  manual per-query tenant predicates on that path.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:19-64,199-224,238-329`.
- **Reachability and caller trace:** `insert_login_state` runs the purge and
  insert from `HumanConnections::begin_login`; `consume_login_state` and
  `complete_login_state` run from `AuthorizationCodeExchange`; and
  `redeem_login_completion` runs from `LoginCompletions::redeem`. Their full
  bodies all execute through `&mut TenantConn`, while the migration at
  `20260925000001_auth_login_state_binding.sql:51-55` enables and forces an RLS
  policy on `data_tenant_id`. Purge, consume, complete, and redeem nevertheless
  repeat `data_tenant_id = $1`, and the unit test requires that duplicate.
- **Observable consequence:** tenant selection has two independently maintained
  definitions that can drift; the source and its test violate the repository's
  mandatory load-bearing RLS boundary even though the current values agree.
- **Smallest safe correction:** retain `data_tenant_id` only as inserted row
  ownership data. Remove the tenant predicates and corresponding binds from
  purge, consume, complete, and redeem, preserving their state hash/binding,
  expiry, consumed, and completion predicates. Delete the SQL-text assertion
  that mandates the prohibited shape; reuse forced RLS rather than adding
  another guard.
- **Preserved adjacent behavior:** one-use state, expiry, binding uniqueness,
  consume-before-provider-IO, sealed completion, and cross-tenant refusal do
  not change.
- **Focused closure proof:** a Postgres test proves tenant B cannot purge,
  consume, complete, or redeem tenant A's row while tenant A can perform each
  valid transition once; run the focused SQL/auth tests and
  `mise run check:tenant-isolation`.

### FIND-TASK-002-6 — Cross-tenant state lookup propagates raw `PgPool`

- **Wave 1 IDs:** `STD-002`, `TD-001`
- **Status:** REVISED
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` permits only
  `TenantConn` and `OperatorPool` in library SQL capability signatures and
  keeps runtime app-pool selection behind `WyrdPostgres`.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/platform/tenant_resolver.rs:37-68` and
  `crates/wyrd/wyrd-auth/src/callback.rs:113-139`.
- **Reachability and caller trace:** the sole caller of
  `resolve_by_login_state_for_app` is
  `AuthorizationCodeExchange::execute`; it obtains
  `HumanConnections::postgres()`, extracts `app_pool()`, and passes a raw pool
  into the public query function before every callback. The SQL function's
  complete body is deliberately narrow, but its type admits any `PgPool`.
  Existing slug-resolver raw-pool drift does not authorize adding another
  security-sensitive raw-pool API.
- **Observable consequence:** the Rust boundary does not encode that only the
  runtime `wyrd_app` role may invoke the definer lookup; a caller can supply an
  operator, migrator, fixture, or misconfigured pool.
- **Smallest safe correction:** move this one lookup behind an inherent narrow
  operation on the existing `WyrdPostgres` owner, which uses its private app
  pool internally. The callback passes only the state hash and never obtains or
  forwards a pool. Preserve the existing SECURITY DEFINER function and its
  one-column result; do not introduce a new connection wrapper or generic
  repository trait.
- **Preserved adjacent behavior:** the definer function's least disclosure,
  unknown/expired/consumed refusal, tenant transaction acquisition, and the
  unrelated existing slug-resolution surface remain unchanged.
- **Focused closure proof:** callback/Postgres coverage proves valid resolution
  and unknown, expired, consumed, and cross-tenant refusal through the new
  owner method; run `mise run check:from-pools-allowlist` and
  `mise run check:tenant-isolation`.

### FIND-TASK-002-7 — Durable login state exposes the PKCE verifier through `Debug`

- **Wave 1 IDs:** `STD-003`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` §4 requires `SecretString` and redacted
  debug behavior for secret-bearing structs; the security posture prohibits
  secrets in diagnostics.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:125-188,199-260`,
  `crates/wyrd/wyrd-auth/src/login.rs:107-132`, and
  `crates/wyrd/wyrd-auth/src/callback.rs:147-175`.
- **Reachability and caller trace:** `HumanConnections::begin_login` constructs
  `NewLoginState`; `insert_login_state` binds it; `consume_login_state` returns
  `ConsumedLoginState`; and `AuthorizationCodeExchange::complete` reads the
  verifier for the provider request. Both public state structs derive `Debug`
  while holding `code_verifier: String`, so ordinary formatting exposes the
  possession secret. Before this candidate, `LoginStateEntry` held the verifier
  as `SecretString`; the candidate removed that protection during relocation.
- **Observable consequence:** logging or assertion diagnostics can disclose the
  live verifier that authorizes the in-flight code exchange.
- **Smallest safe correction:** restore `SecretString` on `NewLoginState` and
  `ConsumedLoginState`, exposing it only for the SQL bind and token request.
  Keep the private SQL decode row as `String` if SQLx requires it, converting at
  that boundary. Reuse `secrecy`, which is already installed in `wyrd-sql`; no
  wrapper type is needed.
- **Preserved adjacent behavior:** database representation, PKCE challenge and
  token request bytes, state cloning where genuinely required, and login-state
  lifecycle remain unchanged.
- **Focused closure proof:** format both public state values containing a
  sentinel verifier and assert the sentinel is absent, then run the focused
  login-state/callback tests and `mise run lints`.

## Recommendation

Route all seven retained findings as one bounded TASK-002 remediation. They
share the tenant callback and login-state seam but do not require a new product,
public API, architecture, security-policy, concurrency, or persistent-data
decision. Apply the root-cause corrections in the existing human-connection,
callback, role-assignment/audit, `WyrdPostgres`, and login-state owners; do not
add compatibility paths, generic abstractions, or a second audit mechanism.

## Verification limits

- This validation inspected the complete base-to-candidate diff, all Wave 1
  reports, and the complete bodies and callers named above. No Cargo, Postgres,
  or IdP lane was rerun during Wave 2.
- The implementation evidence records green identity, principals, SQL,
  codegen, docs, tenant-isolation, format, and lint lanes, but none directly
  exercises the seven retained gaps. The evidence does not record
  `check:from-pools-allowlist`.
- TASK-003's BFF redemption route and TASK-004's CLI handoff persistence remain
  outside this task and were not treated as missing behavior.
- The review directory is review output outside the immutable candidate. The
  candidate commit itself remained unchanged throughout validation.
