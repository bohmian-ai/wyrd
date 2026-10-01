# TASK-002 R9 Structured Ponytail Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `fa2bda92a7e79471b79b607870c1e86a9f35639c`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Required remediation authority: TASK-002 R1 through R8, including their
  verdicts, validated ledgers, remediation tasks, and stable
  `FIND-TASK-002-1` through `FIND-TASK-002-17`

The candidate remained exactly
`fa2bda92a7e79471b79b607870c1e86a9f35639c` throughout validation. The review
artifacts are untracked output outside the immutable subject.

## Completeness and method

All required Wave 1 reports were present and readable:

- `task-review.md`
- `standards-review.md`
- `domain-review-security.md`
- `domain-review-tenancy-data.md`

No `.codegraph/` index exists, so validation used Git, `rg`, and direct source
inspection. I read the approved specification and original task, all supplied
R1-R8 remediation tasks and their prior validation/verdict authority, the
complete cumulative changed-file inventory and base-to-candidate diff, the
applicable repository, server, SQL-capability, security, contract-generation,
and testing authorities, and the complete bodies and callers named below.

For every proposed correction I first asked whether the finding could be
deleted, whether an existing owner already supplied the behavior, and whether
Rust, PostgreSQL, or an installed dependency supplied the minimum mechanism.
No retained correction needs a new dependency, trait, wrapper hierarchy,
configuration knob, table, route, public wire shape, isolation mode, or
security decision.

## Wave 1 disposition

| Wave 1 proposal | Disposition | Stable finding | Validation result |
|---|---|---|---|
| `TASK-REV-001` | **CONFIRMED**, deduplicated with `TD-001` | `FIND-TASK-002-18` | The callback writes the provider role set before taking the existing User refresh-family lock. Two disjoint writes can therefore union, and the later issuer can read and mint that union. |
| `TD-001` | **CONFIRMED**, deduplicated with `TASK-REV-001` | `FIND-TASK-002-18` | Same reachable transaction-ordering defect and same minimum correction. |
| `TASK-REV-002` | **CONFIRMED** | `FIND-TASK-002-19` | The materially revised source field documentation and both generated schemas promise an API-key refresh token that runtime issuance and architecture explicitly refuse. |
| `REPO-001` | **CONFIRMED** | `FIND-TASK-002-20` | The changed `POST /auth/token` write handler has no scrubbed handler span, contrary to the explicit server rule and local callback/login pattern. |
| `REPO-002` | **CONFIRMED** | `FIND-TASK-002-21` | The changed public router owner retains placeholder rustdoc and an undocumented `expect` panic. |
| `TD-002` | **REVISED** | `FIND-TASK-002-22` | The changed login path does propagate a raw app pool across its library boundary. The minimum correction is a narrow inherent `WyrdPostgres` capability that delegates to the existing slug resolver internally; duplicating the SQL or refactoring unrelated existing server callers is unnecessary. |
| Security report's explicit empty ledger | **EMPTY LEDGER REJECTED AS AN OVERALL CONCLUSION** | `FIND-TASK-002-18` already retained | Its cryptographic, state, SSRF, secret, connection-cutoff, refresh-containment, audit, and principal-plane conclusions were independently sustained, but its claim that provider-derived roles remain exact under all reachable callbacks misses the confirmed role-serialization defect. No additional distinct security finding was found. |

## Independent caller tracing and Ponytail outcomes

### `TASK-REV-001` / `TD-001` — callback role serialization

`AuthorizationCodeExchange::execute` is reached from the common HTTP callback
through `exchange_authorization_code`. It resolves the tenant from the state
hash, consumes and commits the state before provider IO, and then calls
`complete`, whose only completion path calls
`finish_id_token_exchange`. The latter verifies the token, nonce, authorized
party, and active connection, opens one `TenantConn`, resolves the canonical
User, computes mapped roles, calls `replace_user_roles`, audits a real role
change, calls `TenantTokenIssuer::issue_human_session`, seals the completion,
and commits.

`replace_user_roles` has one production caller: this callback. Its single CTE
deletes omitted roles and inserts wanted roles with `ON CONFLICT DO NOTHING`,
but neither the CTE nor its caller locks the User or refresh family first. At
PostgreSQL's ordinary statement snapshots, two callback transactions starting
from no roles and inserting distinct role rows do not conflict. Each can insert
its own row.

`issue_human_session` then takes `lock_refresh_family`, followed by the
connection-slot lock, and `TenantTokenIssuer::issue` reads the current durable
roles. All production family-lock callers were traced: refresh rotation takes
the same lock before classification, administrative User revocation takes it
before suspension/family retirement, and `issue_human_session` takes it before
connection and principal reads. Reacquiring the same transaction advisory lock
inside `issue_human_session` is safe and does not change the established
family-before-connection order.

The failing interleaving is reachable: callbacks A and B insert disjoint A/B
roles before either family lock; A acquires the lock, reads A, mints, and
commits; B then acquires the lock and its later role read sees committed A plus
its own B, so B mints and commits A+B. The final durable set is also A+B. This
violates the exact verified-group authority boundary even though the current
sequential role/audit tests pass.

Ponytail outcome: keep every existing owner and add no lock abstraction. Take
the already-installed, tenant-qualified `lock_refresh_family` after canonical
User resolution and before `replace_user_roles`, retaining it through role
audit, session issuance, completion, and commit. Locking only issuance is too
late; adding per-role locks, a new table, serializable transactions, or a
process mutex is larger and weaker.

### `TASK-REV-002` — token-response source contract

`TokenResponse` is the shared response projected by
`ExchangedToken::into_response`. `TenantTokenIssuer::issue`, used by API-key,
workload, delegation, and as the access-token stage of human issuance, always
starts with `refresh_token: None`. Only `issue_human_session` inserts a human
refresh row and replaces that field with `Some`; its production callers are
the OIDC callback and human refresh rotation. The token route's API-key arm
returns the unchanged `None` response.

The current field rustdoc nevertheless lists `wyrd_api_key` as refresh-token
issuing. Schemars copies that false sentence into both checked schema trees.
The server architecture and user documentation correctly say machines renew by
re-exchanging their durable key and receive no refresh token. The source field
was materially revised in this candidate while retaining the false API-key
claim, so this is within the cumulative review rather than unrelated old prose.

Ponytail outcome: no runtime or type change. Correct the one source field
comment to name only human OIDC login and its human refresh rotation, then use
the existing schema generator and drift check. Generated JSON must not be
hand-edited.

### `REPO-001` — token handler instrumentation

`auth_router` mounts `token` as the sole `POST /auth/token` HTTP handler, and
the assembled server merges that router in `http::router`. The complete handler
selects API-key, delegation, refresh, or workload issuance and owns or composes
durable token, refresh, and audit commits. The cumulative change removes the
authorization-code arm from this function, so the handler is materially
modified. Unlike the changed login and callback handlers, it has no
`#[tracing::instrument]` attribute.

Ponytail outcome: the explicit server rule requires the span; there is no
existing outer handler span that makes it redundant. Add only the local pattern
`#[tracing::instrument(level = "debug", skip_all)]`. `skip_all` is required
because the arguments include API keys, assertions, access tokens, and refresh
tokens. No runtime test or tracing helper is justified.

### `REPO-002` — router rustdoc and panic contract

`auth_router` has two callers: the assembled HTTP router and its route-mount
test. Its complete body mounts login, callback, token, and issue-key routes and
places one shared governor over them. The candidate added the login surface to
that composition. Its entire rustdoc remains `Build auth routes.`, and the body
contains `GovernorConfigBuilder::finish().expect(...)` without a `# Panics`
contract.

Ponytail outcome: preserve the static governor and body. Replace only the
placeholder rustdoc with the four-surface/shared-admission role and document
that invalid static governor settings panic. A new config type, fallible public
constructor, helper, or runtime test would be churn for a source-contract
defect.

### `TD-002` — login slug resolution capability

`HumanConnections::begin_login` is called by the unauthenticated login HTTP
adapter. Its complete body validates initiation/public-origin/keyring state,
resolves the route slug, reads the Active tenant connection, performs screened
discovery, creates PKCE/state/nonce, inserts the tenant login-state row, and
commits. The new route-slug step calls
`resolve_by_slug_for_app(self.postgres().app_pool(), ...)`.

`resolve_by_slug_for_app` executes the narrow SECURITY DEFINER slug lookup, but
its public signature accepts arbitrary `&PgPool`. Its other production callers
are existing server boot/workload-routing code. `WyrdPostgres::app_pool` is the
raw escape used by the changed login owner, while the adjacent callback already
uses the repository-native precedent `WyrdPostgres::login_state_tenant`, which
keeps app-role selection inside the concrete database handle.

The current caller supplies the correct pool, so this is a capability-shape
violation, not evidence of present cross-tenant disclosure. It is still
reachable on every begin-login request and explicitly violates the rule that a
library workflow must not propagate raw pools.

Ponytail outcome: add one narrow inherent slug-resolution method on the
existing `WyrdPostgres` owner and make `begin_login` pass only the typed slug.
The method should reuse/delegate to the existing resolver with its private app
pool. Do not duplicate the query, introduce a repository trait or pool wrapper,
or refactor the unrelated pre-existing server callers as part of this task.

### Security empty-ledger validation

The state key remains 32 random bytes and only its SHA-256 is stored. The common
callback resolves only that hash through the least-disclosure owner method,
consumes state before provider IO, and uses no host/header/claim tenant
selector. Provider discovery, token exchange, and JWKS access remain on the
screened, redirect-disabled, bounded, pinned HTTP path. Fresh advertised
algorithm membership precedes the shared asymmetric signature/JWKS verifier;
mandatory issuer, audience, expiry, optional `nbf`, OIDC `iat`, nonce, `azp`,
and Subject Identifier checks precede identity persistence. Human identity is
tenant-qualified `(issuer, sub)` and email does not link. The exact connection
revision is rechecked, and issuance takes family then connection locks before
reading status and minting. PKCE, provider credentials, Wyrd credentials, and
sealed completions retain redacted/secret-backed handling. Workload, tenant
human, and platform paths remain distinct.

The security report's empty conclusion therefore stands for those boundaries,
but not for provider-role exactness: `FIND-TASK-002-18` is itself a reachable
authority-expansion defect. It is retained once rather than copied into a
second security finding.

## Final deduplicated finding ledger

### FIND-TASK-002-18 — Concurrent callbacks can union disjoint provider roles

- **Wave 1 source IDs:** `TASK-REV-001`, `TD-001`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-008 and INV-003 require a callback to grant only
  tenant roles mapped from that callback's verified groups; TASK-002 requires
  role persistence, issuance, and audit to form one correct tenant transaction.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/callback.rs:228-266`,
  `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:34-53,117-146`,
  and `crates/wyrd/wyrd-auth/src/issuance.rs:453-538`.
- **Evidence:** role replacement runs before the existing User family lock.
  Two disjoint inserts do not conflict; the callback that waits for the family
  lock can later read the other callback's committed row plus its own row and
  mint the union. Existing role-sync and journey proofs are sequential.
- **Observable consequence:** a callback asserting only role B can receive
  role A permissions, and the combined durable set remains renewable.
- **Decision-complete correction:** after `ensure_user_identity` returns the
  canonical User id and before `replace_user_roles`, reuse
  `lock_refresh_family(conn, "user", principal_id)` on the same `TenantConn`.
  Preserve role-sync audit, `issue_human_session`, completion sealing, commit,
  and the family-before-connection lock order. Add no lock owner or isolation
  change.
- **Focused closure proof:** add one deterministic Postgres proof using the
  production callback and issuance owners, one existing User, and two disjoint
  mapped role sets. Control both lock-acquisition orderings without sleeps;
  prove the second callback waits before role replacement, each returned access
  token contains exactly that callback's asserted role, the final durable set
  equals the later serialized callback's exact set, role-sync audit cardinality
  remains correct, and no union survives.

### FIND-TASK-002-19 — Generated token contract falsely promises API-key refresh

- **Wave 1 source ID:** `TASK-REV-002`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-013 and the runtime identity authority keep
  machine renewal on durable-credential re-exchange; TASK-002 requires the
  public schema/documentation to agree with the changed token behavior.
- **Exact location:** `crates/wyrd-spec/src/auth/token.rs:87-101`,
  `crates/wyrd-spec/schemas/auth_token_response.json:26`, and
  `crates/wyrd-spec/tests/schemas/auth_token_response.json:26`.
- **Evidence:** the source and generated descriptions list
  `wyrd_api_key`, while `TenantTokenIssuer::issue` returns
  `refresh_token: None` and only `issue_human_session` fills it. Architecture
  and user docs state that machines re-exchange and receive no refresh token.
- **Observable consequence:** schema/OpenAPI consumers are told to persist and
  rotate renewable authority that the API never returns.
- **Decision-complete correction:** correct only the source field rustdoc to
  describe refresh tokens as present for a human OIDC login and its human
  refresh rotation. Regenerate both schema trees through the existing
  generator. Preserve runtime behavior and the wire shape.
- **Focused closure proof:** inspect the source and regenerated schema
  descriptions, run `mise run codegen:check`, and retain the existing
  API-key/machine journey proof that the runtime response omits the field.

### FIND-TASK-002-20 — Changed token write handler lacks scrubbed instrumentation

- **Wave 1 source ID:** `REPO-001`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` §9 and the server pattern require every
  write handler to carry `#[tracing::instrument]` with scrubbed arguments.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:58-272`.
- **Evidence:** `token` is the mounted `POST /auth/token` handler, was
  materially changed by grant retirement, performs or composes durable token,
  refresh, and audit writes, and has no handler instrumentation. Login and
  callback use the required local pattern.
- **Observable consequence:** this credential boundary lacks the mandated
  request span; naïve default instrumentation would additionally expose
  secret-bearing arguments.
- **Decision-complete correction:** add only
  `#[tracing::instrument(level = "debug", skip_all)]` immediately above the
  handler. Record no request body, headers, API key, assertion, access token,
  or refresh token.
- **Focused closure proof:** source inspection plus `mise run fmt`,
  `mise run lints`, and the existing principals/identity route lanes. No new
  test harness is warranted.

### FIND-TASK-002-21 — Changed auth router has placeholder rustdoc and no panic contract

- **Wave 1 source ID:** `REPO-002`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` §16 requires substantive rustdoc for
  every materially modified Rust item and `# Panics` whenever a panic remains.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:40-56`.
- **Evidence:** the changed public owner mounts four auth surfaces under one
  governor but says only `Build auth routes.`; its static governor construction
  calls `expect` with no documented panic.
- **Observable consequence:** the source fails a hard repository gate and does
  not state the shared admission boundary or the static invariant maintainers
  must preserve.
- **Decision-complete correction:** revise only `auth_router` rustdoc to name
  login, callback, token, and issue-key composition under the shared governor,
  and add `# Panics` for rejection of the static governor configuration.
  Preserve the implementation.
- **Focused closure proof:** direct source inspection, `mise run fmt`,
  `mise run lints`, and `git diff --check`; no behavioral test is warranted.

### FIND-TASK-002-22 — Begin-login propagates a raw application pool

- **Wave 1 source ID:** `TD-002`
- **Status:** REVISED
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` permits only
  `TenantConn` and `OperatorPool` in library connection signatures and keeps
  application-pool role selection behind `WyrdPostgres`.
- **Exact location:** `crates/wyrd/wyrd-auth/src/login.rs:84-139` and
  `crates/wyrd/wyrd-sql/src/queries/platform/tenant_resolver.rs:8-35`.
- **Evidence:** every begin-login request calls
  `resolve_by_slug_for_app(self.postgres().app_pool(), ...)`; the resolver's
  public `&PgPool` signature does not encode the required app-role capability.
  The route currently supplies the intended pool, so no current disclosure is
  claimed.
- **Observable consequence:** the new pre-authentication login workflow relies
  on caller discipline for database-role identity and exposes the raw pool past
  its concrete owner.
- **Decision-complete correction:** add a narrow inherent slug-resolution
  method to the existing `WyrdPostgres` owner that internally delegates to the
  existing resolver with its private app pool, and call that method from
  `HumanConnections::begin_login` with only `TenantSlug`. Do not duplicate SQL,
  add a trait/wrapper, or refactor unrelated existing server callers.
- **Focused closure proof:** static inspection shows the changed login module
  no longer imports `resolve_by_slug_for_app` or calls `app_pool`; retain the
  existing unknown/suspended/deleted generic-refusal and successful resolution
  proofs, then run `mise run check:from-pools-allowlist`,
  `mise run check:tenant-isolation`, and the focused login tests.

## Prior-finding closure

| Prior finding | Independent result |
|---|---|
| `FIND-TASK-002-1` — mutable human subject mapping | **CLOSED.** Public validation and stored decode require exact OIDC `sub`; same-email subjects remain distinct. |
| `FIND-TASK-002-2` — OIDC authorized party | **CLOSED.** The tenant callback enforces multi-audience and present-`azp` semantics before persistence. |
| `FIND-TASK-002-3` — missing provider role-change audit | **CLOSED for its diagnosed sequential/audit behavior.** A real change appends one canonical transactional role-sync event and audit failure rolls back. `FIND-TASK-002-18` is the distinct missing serialization before that mutation. |
| `FIND-TASK-002-4` — provider-advertised algorithm | **CLOSED.** Fresh advertised membership precedes shared asymmetric signature/JWKS verification. |
| `FIND-TASK-002-5` — duplicate tenant predicates | **CLOSED.** Login-state transitions rely on forced RLS. |
| `FIND-TASK-002-6` — raw-pool state lookup | **CLOSED.** State-to-tenant resolution remains an inherent `WyrdPostgres` capability. `FIND-TASK-002-22` concerns the separate route-slug lookup newly used by begin-login. |
| `FIND-TASK-002-7` — printable PKCE verifier | **CLOSED.** Durable login state retains `SecretString` and redacted debug proof. |
| `FIND-TASK-002-8` — incomplete Rust documentation inventory | **CLOSED for the exact R2 inventory.** Findings 19 and 21 concern separate materially revised source contracts. |
| `FIND-TASK-002-9` — hidden imports | **CLOSED.** The cited imports remain module-scoped. |
| `FIND-TASK-002-10` — false algorithm-helper rustdoc | **CLOSED.** The helper documents advertised membership and names the shared verifier's HMAC owner. |
| `FIND-TASK-002-11` — optional ID-token binding/time claims | **CLOSED.** Generic verification requires issuer/audience/expiry and valid present `nbf`; OIDC additionally requires numeric non-future `iat`. |
| `FIND-TASK-002-12` — replay/rotation containment race | **CLOSED.** Refresh classification and mutation are serialized by family before the connection lock and commit. |
| `FIND-TASK-002-13` — stale refresh lookup rustdoc | **CLOSED.** Documentation matches lookup-before-lock/classification and its test observation use. |
| `FIND-TASK-002-14` — revocation versus refresh successor | **CLOSED.** Administrative User revocation shares the family lock and contains an overlapping successor. |
| `FIND-TASK-002-15` — revocation versus first issuance | **CLOSED.** The human-session owner takes family then connection locks before status read; both orderings have deterministic proof. |
| `FIND-TASK-002-16` — false CLI printer rustdoc | **CLOSED.** `print_tokens` documents only token output. |
| `FIND-TASK-002-17` — invalid OIDC Subject Identifier | **CLOSED.** OIDC verification rejects missing, non-string, empty, non-ASCII, and oversized subjects while workload mapping remains unchanged. |

## Verification limits

- `git diff --check` passed for the complete immutable range, and `HEAD` still
  matched the candidate after validation.
- This Wave 2 review was static and did not rerun Cargo, Postgres, Docker, or
  provider lanes in the shared checkout. The cumulative implementation records
  report green focused R1-R8 proofs, `test:principals:unit`,
  `test:principals:integration`, `test:sql`, `test:identity:journey` (27/27),
  codegen, docs, tenant/pool/client boundary checks, formatting, and lints.
- No existing proof overlaps two callbacks for one existing User with disjoint
  mapped roles. That deterministic Postgres proof is required to close
  `FIND-TASK-002-18`.
- Findings 19-22 are established directly by source/authority mismatch; green
  runtime lanes cannot substitute for correcting those source contracts and
  mandatory repository boundaries.
- Live provider qualification, TASK-003 BFF completion, and TASK-004 CLI
  handoff persistence remain outside this task and were not treated as gaps.

## Overall validation recommendation

**VALIDATED WITH FINDINGS — FIX_REQUIRED.** Retain
`FIND-TASK-002-18` through `FIND-TASK-002-22`. All five corrections are bounded
to existing owners and approved behavior. No specification revision is
required.
