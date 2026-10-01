# TASK-001 r3 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Candidate tree: `09ffa00e468cc5d518333b8e5ad20ffd4380c150`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation inputs: `TASK-001-R1-production-readiness-gaps.md` and
  `TASK-001-R2-remaining-production-readiness-gaps.md`

The candidate and tree matched the immutable subject at the start and end of
validation. The complete cumulative diff, the original task, both prior
verdicts and validation ledgers, both remediation tasks, all five Wave 1
reports, applicable repository authorities, and the live source and callers
named below were inspected independently.

## Wave 1 proposal validation

| Wave 1 proposal | Result | Final finding | Validation |
|---|---|---|---|
| `task-review.md:TASK-REV-R3-001` | **REVISED** | `FIND-TASK-001-20` | `HumanConnections::begin_login` freshly discovers and returns `authorization_endpoint` without applying the deployment's production scheme rule. This is distinct from closed `FIND-TASK-001-18`, whose exact correction covered server requests through `ScreenedHttp::client_for`. The proposed correction is narrowed: Wyrd must validate the browser destination's scheme, but building and discarding a DNS-pinned HTTP client would add work and falsely imply that Wyrd can pin the browser's later resolution. |
| `standards-review.md:STD-R3-001` | **CONFIRMED** | `FIND-TASK-001-21` | The R2-added fallible test helper `bounded_get` has `# Panics` but no mandatory `# Errors`, contrary to `AGENTS.md` §16 and `architecture/agent-rules.md`. No new test or helper is needed. |
| `domain-review-security.md:SEC-R3-001` | **CONFIRMED** | `FIND-TASK-001-22` | A verified active recovery key reaches role resolution and `permissions.contains(identity_connections:write)`, which is a second principal-permission decision. Only the bearer caller's decision is appended. Deduplicated with `domain-review-tenancy-data.md:TD-R3-002`. |
| `domain-review-security.md:SEC-R3-002` | **REVISED** | `FIND-TASK-001-20` | Same reachable live-login gap as `TASK-REV-R3-001`; deduplicated. The retained correction validates the effective browser redirect scheme before durable state, without performing irrelevant DNS pinning for a URL Wyrd does not fetch in this path. |
| `domain-review-security.md:SEC-R3-003` | **CONFIRMED** | `FIND-TASK-001-23` | Every domain-name call to `ScreenedHttp::client_for` awaits `tokio::net::lookup_host` before reqwest's ten-second timeout exists. The repository security authority expressly requires a bounded resolver, and TASK-001 makes this shared path reachable from tenant connection test, login, callback, and JWKS operations. |
| `domain-review-tenancy-data.md:TD-R3-001` | **REJECTED** | — | The host-selected callback tenant is real but unchanged from the base commit. TASK-001's declared requirement set excludes REQ-006 and INV-001, and its material stop conditions forbid changing the callback contract. Fixing this requires a separate callback-routing/persistent-state decision; it is not a defect introduced by, or acceptance obligation of, this task. |
| `domain-review-tenancy-data.md:TD-R3-002` | **CONFIRMED** | `FIND-TASK-001-22` | Duplicate of `SEC-R3-001`; the same sole production activation call path and missing recovery-principal audit row were independently verified. |
| `domain-review-secrets.md` empty proposal set | **CONFIRMED** | — | Secret input, sealing, redaction, cross-store rewrap, restrictive key-file loading, and the post-writer rotation proof expose no additional material finding in this task. |

## Caller and reachability validation

### Live authorization redirect

`wyrd-server/src/auth/login.rs::try_initiate_login` is the production tenant
login caller of `HumanConnections::begin_login`. The complete method at
`wyrd-auth/src/login.rs:129-172` reads the current Active connection, performs
fresh discovery, takes `metadata.authorization_endpoint`, builds a browser URL,
persists login state, and returns that URL. `AbsoluteUrl` deliberately permits
both HTTP and HTTPS. Candidate testing screens and fetches the authorization
endpoint in `HumanConnections::probe_callback`, but that stamp is a snapshot;
the live discovery document can change afterward. `ScreenedHttp::client_for`
is not called for the freshly discovered browser destination in `begin_login`.

The path is required by TASK-001 because the new Active tenant connection is
consumed by login and INV-004 keeps TLS fail closed. The minimum correction is
not another HTTP client or cached metadata. Reuse the existing deployment
address policy's scheme rule at the effective redirect boundary: HTTPS is
required under `BlockInternal`, while `AllowInternal` retains HTTP for local
providers.

### Recovery-principal permission decision

`wyrd-server/src/components/admin/identity.rs::activate_candidate` is the sole
production caller of `HumanConnections::activate`. It evaluates and passes the
bearer caller's audited decision. Under the same locked tenant transaction,
`recovery_key_authorizes` verifies a second credential, loads its principal's
roles, resolves current permissions, and evaluates
`identity_connections:write`. The allowed and valid-but-underprivileged arms
therefore make a second permission decision, but no second `AuditEvent` is
appended. Malformed, unknown, cross-tenant, hash-mismatched, and inactive keys
do not reach permission evaluation and need no fabricated decision row.

The original task expressly requires the recovery key to hold this permission,
and repository authority requires one transactional row per permission
evaluation. The correction belongs in the existing activation transaction and
canonical append path; no second audit sink, authorization service, or public
contract is needed.

### DNS deadline

All OIDC production callers of the shared `ScreenedHttp::client_for` were
traced: discovery in `wyrd-auth/src/callback.rs::discover_provider`, token
exchange in `exchange_code_for_id_token`, candidate authorization/token probes
in `wyrd-auth/src/connections.rs`, and JWKS retrieval in
`wyrd-auth-oidc/src/jwks.rs::fetch_jwks`. For a domain host they all enter
`resolve_and_screen`, whose complete body awaits `tokio::net::lookup_host`
directly. The reqwest timeout is installed only on the client built after that
await, so it cannot bound resolution. A stalled resolver can therefore retain
login, callback, and administration work indefinitely before any request is
sent.

The minimum correction is one fixed deadline around the existing shared DNS
lookup, reusing `FETCH_TIMEOUT` and the existing redacted `Unresolved` mapping.
No resolver trait, configuration knob, dependency, retry layer, or per-caller
timeout is justified.

### Rejected callback-tenant proposal

`wyrd-server/src/auth/callback.rs::callback` calls
`exchange_authorization_code`, which calls `resolve_callback_tenant` before
`AuthorizationCodeExchange::execute`; the latter then consumes state inside
that tenant's RLS transaction. The same host-derived selection and complete
caller chain are present at the base commit. The proposal is consequently not
disputed as an architecture observation, but it is rejected from this ledger:
the original TASK-001 front matter does not claim REQ-006 or INV-001, and the
task names a different callback contract as a stop condition. A correction
would need an approved choice for recovering tenant identity before a
`TenantConn` exists rather than being smuggled into this remediation.

## Validated finding ledger

### FIND-TASK-001-20 — REVISED — INCORRECT: live login permits a newly discovered cleartext authorization endpoint

- **Wave 1 sources:** `TASK-REV-R3-001`, `SEC-R3-002`.
- **Violated obligation:** TASK-001 REQ-004 requires validated discovered
  provider endpoints, and INV-004 requires TLS to remain fail closed.
- **Exact location:** `crates/wyrd/wyrd-auth/src/login.rs:137-157`; shared
  production scheme policy at
  `crates/shared/wyrd-auth-oidc/src/screening.rs:154-156,218-220`.
- **Evidence:** Live login performs fresh HTTPS issuer discovery and returns the
  discovered authorization URL after only `AbsoluteUrl` validation, which
  accepts HTTP. Candidate qualification previously screened a snapshot, but
  the provider can advertise a different endpoint after activation.
- **Observable consequence:** A compromised or misconfigured provider can make
  production Wyrd redirect the browser, OAuth state, nonce, PKCE challenge,
  client id, and callback value to a cleartext login endpoint.
- **Decision-complete correction:** Before constructing or persisting login
  state, apply the existing deployment policy's scheme rule to the freshly
  discovered authorization endpoint. `BlockInternal` must refuse non-HTTPS;
  `AllowInternal` must preserve the repository-managed HTTP provider path.
  Do not build a throwaway pinned client, add a policy knob, cache discovery,
  or change token/JWKS/platform-login behavior.
- **Focused closure proof:** Under `BlockInternal`, live tenant login against
  HTTPS discovery advertising an HTTP authorization endpoint returns the
  existing redacted discovery refusal before inserting a login-state row or
  returning a redirect. The existing `AllowInternal` local-provider login
  remains successful.

### FIND-TASK-001-21 — CONFIRMED — VIOLATION: the new fallible bounded-body test helper lacks `# Errors`

- **Wave 1 source:** `STD-R3-001`.
- **Violated obligation:** `AGENTS.md` §16 and
  `architecture/agent-rules.md` require every new fallible Rust function,
  including a private test helper, to document its error conditions.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/screening.rs:481-496`.
- **Evidence:** `bounded_get` was added by R2, returns
  `Result<Vec<u8>, BodyError>`, and documents only its panics.
- **Observable consequence:** The candidate violates a hard repository
  completion rule on touched Rust even though the executable behavior passes.
- **Decision-complete correction:** Add only a `# Errors` section stating that
  the helper propagates `BodyError::TooLarge` for a response above the shared
  cap and `BodyError::Read` for transfer or decoding failure. Keep the helper
  and its existing tests unchanged.
- **Focused closure proof:** Direct source inspection plus `mise run fmt` and
  `mise run lints`; no additional behavioral test is warranted for a rustdoc-only
  correction.

### FIND-TASK-001-22 — CONFIRMED — VIOLATION: activation does not audit the recovery principal's permission decision

- **Wave 1 sources:** `SEC-R3-001`, `TD-R3-002`.
- **Violated obligation:** `AGENTS.md` current decisions and
  `architecture/agent-rules.md` require one canonical transactional audit row
  for every allowed or denied principal-permission evaluation; TASK-001
  requires the recovery credential to prove the same tenant's
  `identity_connections:write` authority.
- **Exact location:** `crates/wyrd/wyrd-auth/src/connections.rs:451-497,918-952`;
  production caller at
  `crates/wyrd/wyrd-server/src/components/admin/identity.rs:302-312`.
- **Evidence:** Activation appends the bearer caller's decision, then the
  recovery-key path independently resolves another principal's permissions and
  calls `contains(identity_connections:write)`. Neither the allowed nor the
  valid-but-underprivileged outcome is appended for that recovery principal.
- **Observable consequence:** Retained evidence cannot identify which recovery
  principal and non-secret credential authorized activation or show that a
  valid recovery principal was denied; required audit failure also cannot stop
  that second decision from taking effect.
- **Decision-complete correction:** In the existing locked activation
  transaction, append a second canonical decision for each recovery key that
  resolves to an active principal and reaches permission evaluation. Attribute
  it to that principal and the verified non-secret credential id, use
  `identity_connections:write`, and record Allowed or Denied before promotion
  or committed refusal. Audit failure must roll back/refuse activation.
  Preserve the bearer decision and the indistinguishable behavior of malformed,
  unknown, cross-tenant, hash-mismatched, or inactive credentials; add no audit
  path or public field.
- **Focused closure proof:** The activation journey uses a recovery credential
  distinct from the bearer and asserts exactly both attributed decisions. A
  valid active recovery principal without the permission records Denied and
  does not promote; injected failure of the recovery-decision append leaves
  the old Active and Candidate unchanged.

### FIND-TASK-001-23 — CONFIRMED — VIOLATION: shared provider DNS resolution is outside every operation deadline

- **Wave 1 source:** `SEC-R3-003`.
- **Violated obligation:** `architecture/wyrd-security-posture.md` requires DNS
  resolution through a bounded resolver and total-operation bounds before any
  tenant-directed fetch; TASK-001 requires screened provider IO and INV-004
  keeps DNS controls fail closed.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/screening.rs:154-183,194-215`.
- **Evidence:** Domain-name screening awaits `tokio::net::lookup_host` before
  the reqwest client carrying `FETCH_TIMEOUT` is built. Every live discovery,
  token, JWKS, and candidate-probe caller shares this path.
- **Observable consequence:** A tenant-controlled name or unhealthy system
  resolver can retain request tasks without the advertised ten-second provider
  bound, allowing request/worker exhaustion before an outbound connection.
- **Decision-complete correction:** Bound the existing single DNS lookup with
  the existing fixed `FETCH_TIMEOUT`, map expiry to the same redacted
  `ScreenError::Unresolved`, and keep the exact returned address set for
  `resolve_to_addrs`. Add no resolver abstraction, dependency, retry, or
  configuration surface.
- **Focused closure proof:** A deterministic Tokio-time check holds resolution
  pending, advances through the fixed deadline, and observes `Unresolved`;
  retain the existing address-screening, pinning, proxy, and local-provider
  checks.

## Prior-finding closure

`FIND-TASK-001-1` through `FIND-TASK-001-19` remain closed at their validated
correction boundaries. In particular, `FIND-TASK-001-18` closed the shared
server-fetch scheme gap exactly as prescribed; `FIND-TASK-001-20` is the
separate browser redirect path that never calls that fetch owner.

## Verification limits

- Static validation only, as assigned. No Cargo-backed or `mise` Cargo lane
  was run. Existing implementation evidence was inspected but not reproduced.
- CodeGraph was unavailable because the repository has no `.codegraph/`
  directory; caller tracing used `rg`, the cumulative diff, base source, and
  complete function bodies.
- Long Postgres/provider journeys, controlled commercial IdPs, served OpenAPI,
  codegen, docs, formatting, lints, and boundary lanes remain unrerun.
- A real stalled system resolver was not induced on the shared workstation.
  The DNS finding is source-proven by the await/timeout ordering; its closure
  proof must use deterministic Tokio time rather than host load or resolver
  disruption.

## Final disposition

**FIX_REQUIRED** — retain new findings `FIND-TASK-001-20` through
`FIND-TASK-001-23`. All are bounded implementation corrections within approved
behavior and require no specification revision.
