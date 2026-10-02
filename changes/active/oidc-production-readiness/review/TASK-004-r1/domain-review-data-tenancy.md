# Persistent data and tenancy domain review — TASK-004 R1

## Subject and boundary

Reviewed immutable base `06f134dc14164c040c0e5014d21de29c240f4116`
through candidate `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84` for
persistent data, SQL transactionality, tenant isolation, lifecycle state, and
local saved-record identity. Authority was approved
`changes/active/oidc-production-readiness/spec.md` revision 7,
`TASK-004-laptop-clients.md`, the three supplied TASK-003 human directions,
`AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-security-posture.md`, the Postgres-layout and architecture
pattern references, and the applicable source and tests.

The review traced the handoff migration and query owner through
`HumanConnections::begin_login`, callback completion,
`CliLogins::{begin,claim,cancel,end}`, the HTTP adapters, refresh-token query
owners, `SavedLogins`, `ClientConfig::resolve_credential`, every credential
source, and the Rust/Python/TypeScript and concurrent-process journeys.

## Authority and source coverage

| Boundary | Authority and source evidence | Result |
|---|---|---|
| Handoff schema, expiry, RLS, and deletion | TASK-004 packet-local handoff contract; `20261002000000_auth_cli_handoffs.sql`; `queries/auth/cli_handoffs.rs` | PASS — tenant-qualified rows use forced RLS; verifier hashes and the five-minute ceiling are constrained; successful claim and cancellation delete the handoff and its bound login state. |
| SQL capability and transaction ownership | `AGENTS.md` SQL rules; `agent-rules.md` rules 6, 7, 11; Postgres layout; `CliLogins` and handoff query signatures | PASS — production handoff and refresh operations use `TenantConn`; query callees do not commit or roll back; no new raw pool or `OperatorPool` escape entered these paths. |
| One-use completion coupling | TASK-004 Scenario 1; login-state query owner; `CliLogins::claim_in` lines 220-285 | PASS — the verifier-bound row is locked, the sealed completion and handoff are consumed in the same tenant transaction, the connection binding is compared, and the allowed claim audit and consumption commit together. Replay, wrong verifier/tenant, cancellation, and expiry have journey coverage. |
| Logout rotation serialization and scope | TASK-004 renewal/logout contract; binding human direction `FIND-TASK-003-18`; `CliLogins::end` lines 333-355; `revoke_refresh_chain` | PASS for chain scope — hash lookup under RLS establishes the row, the existing principal-family lock serializes rotation, and recursive revocation affects the presented chain and descendants only. `logout_revokes_only_its_own_chain` and the CLI journey preserve another login. Audit closure fails separately as `DATA-TEN-002`. |
| Saved-record identity and lifecycle | REQ-012; TASK-004 lines 47-91; `saved_login.rs`; CLI login/logout; concurrent renewal journey | PASS for the saved-login tier — filename identity is derived from canonical origin plus stable tenant id; selection accepts the stored tenant key or id, ambiguity and mismatch fail closed, and renewal/logout share the stable record lock and durable pending/tombstone transitions. Cross-source selector enforcement fails as `DATA-TEN-001`. |
| Tenant selector across all credential tiers | TASK-004 lines 57-72; REQ-012/REQ-015; `ClientConfig::resolve_credential`; `CredentialChain` | FAIL — `DATA-TEN-001`. |
| Revocation audit transaction | Security posture lines 430-439; `AGENTS.md` and architecture audit rules; `CliLogins::end` | FAIL — `DATA-TEN-002`. |

## Material findings

### DATA-TEN-001 — tenant selector is bypassed by explicit, environment, and file credentials

- **Classification:** INCORRECT / tenant-isolation violation.
- **Violated obligation:** TASK-004 requires that, when a tenant selector is
  provided, *every* resolved authority match it at exchange or authorization
  or fail; REQ-012 requires a client to use only the login selected for its
  intended server and tenant; REQ-015 forbids one tenant's authority from
  granting access to another.
- **Location:** `crates/shared/wyrd-client/src/config.rs:194-212` and
  `crates/shared/wyrd-client/src/transport/credential.rs:232-286,298-338`.
- **Evidence:** `ClientConfig::resolve_credential` passes `self.tenant` only
  into `CredentialChain::env_only`, then immediately returns the first
  explicit/environment source. `CredentialChain::resolve` returns bearer and
  API-key authorities without retaining or checking the selector. The
  credentials-file API-key floor is likewise resolved without it. Even the
  workload arm lets ambient `WYRD_TENANT` override the supplied
  `tenant_override` (`credential.rs:314-321`). The only post-exchange tenant
  consistency check is private to saved-login refresh
  (`saved_login.rs:436-441`). The implementation record explicitly lists
  this as a ceiling: “The tenant selector is not enforced on the env-variable
  credential tiers.” Existing override journeys use a machine key from the
  same selected tenant and therefore do not exercise the mismatch.
- **Reachable consequence:** a Rust, Python, or TypeScript caller can construct
  a client for tenant A while an explicit credential, `WYRD_ACCESS_TOKEN`,
  `WYRD_API_KEY`, ambient workload tenant, or credentials-file key belongs to
  tenant B. The higher-priority authority wins and the resulting requests act
  as tenant B rather than refusing the caller's stated tenant intent. This is
  the exact cross-tenant confused-client outcome the packet prohibits.
- **Required testable correction:** make the shared `wyrd-client` credential
  owner carry the configured selector through resolution and enforce it for
  every credential tier at the existing exchange/authorization boundary,
  while preserving the approved precedence (including explicit credential
  wins). A mismatch must return the stable tenant-mismatch refusal and must not
  fall through to another source. Reuse the shared Rust path for all three
  language SDKs; do not add binding-specific checks. Focused proof must present
  tenant-B explicit bearer, explicit/API-key, each applicable environment
  tier, conflicting ambient workload tenant, and credentials-file authority
  to a tenant-A-configured client and show refusal before a tenant-B operation,
  plus preserve a matching override and saved-login selection.

### DATA-TEN-002 — CLI logout revokes durable refresh rows without canonical audit evidence

- **Classification:** MISSING / audit-integrity violation.
- **Violated obligation:** `architecture/wyrd-security-posture.md:430-439`
  classifies credential revocation as a security event requiring an audit
  record when an authoritative tenant is available. `AGENTS.md` and the audit
  pattern require the canonical `vala.audit_staging` append in the deciding
  transaction for non-exempt decisions. The TASK-004 logout path has an
  authoritative tenant and principal after the RLS hash lookup, and none of
  the named non-blocking exceptions applies.
- **Location:** `crates/wyrd/wyrd-auth/src/cli_logins.rs:333-355`.
- **Evidence:** `CliLogins::end` derives only routing context from the
  unverified JWT, establishes authority with `refresh_by_hash` under the
  tenant's RLS transaction, takes the family lock, calls
  `revoke_refresh_chain`, and commits. It never builds or appends an audit
  event. Its caller `POST /auth/revoke` adds none. The existing logout tests
  assert row state and sibling-chain survival but never inspect
  `vala.audit_staging` or inject an audit failure.
- **Reachable consequence:** every successful `wyrd auth logout` changes
  durable credential authority with no retained accountable evidence. An
  audit-path failure cannot roll back revocation because the path is never
  invoked, contrary to the repository's single transactional audit authority.
- **Required testable correction:** after the hash lookup establishes the
  stored tenant/principal and while the existing family lock is held, append
  one redacted revocation event through the existing canonical auth audit
  helper in the same `TenantConn` transaction as `revoke_refresh_chain`, then
  commit both. Do not add another audit writer or broaden revocation beyond the
  presented chain. Focused Postgres proof must show exactly one canonical
  event and only that chain revoked on success, and show an injected audit
  append failure rolls back the revocation while the CLI can still tombstone
  and delete its local record with the approved warning behavior.

## Verification limits

I inspected the cumulative diff, full owning functions, sibling refresh and
browser-session consumers, migrations, and all task-listed journey sources.
`git diff --check` passed and HEAD remained the immutable candidate. I did not
rerun Postgres, Keycloak, Rust, Python, or TypeScript lanes; the task records
their prior green results. Those results do not cover either finding: the
implementation evidence acknowledges `DATA-TEN-001`, and no logout test
asserts audit persistence or audit-failure rollback for `DATA-TEN-002`.

The requested `mise.toml` production-wheel boundary assessment is outside this
data/tenancy review and is left to the independently assigned repository and
validation reviewers.

## Result

**FAIL** — `DATA-TEN-001` and `DATA-TEN-002` remain material, bounded failures.
