# TASK-003 r1 — Domain review: security, tenancy, and authorization audit

Reviewer: fresh Wave 1 `domain-rev`. Immutable subject: base
`1609e102881dd55b12154248834a8aedbb7a36b2`, candidate
`467ea07d94a5a6d665d24f56afc0bc0a12532304`.

## Reviewed boundary

I traced the TASK-003 security boundary end to end through:

- composite registration authorization, exact reference resolution, binding
  projection, Card-bound principal creation, transaction ownership, and Card
  status reads;
- API-key, workload `jwt-bearer`, delegation, OIDC-login, refresh, Card-free,
  and future SYSTEM issuance classification in the shared tenant issuer;
- machine-principal lifecycle state, exact Card-version activity stamping,
  schedule arming, current binding eligibility, and A/B-version behavior;
- forced-RLS migration policy, `TenantConn` query reachability, cross-tenant
  reads and writes, and owner/principal joins; and
- canonical authorization-audit reachability for Card reads and registration,
  including the additional permission required when a binding freezes an
  `on_failure` Operator for later SYSTEM execution.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Exact credential-to-principal binding | Security posture principal/credential lifecycle; spec REQ-105/106/112, INV-007; `auth/service_accounts.rs`, API-key issuance, workload `jwt-bearer`, delegation, migration 27 | **FAIL.** The migration removes the uniqueness invariant on which the UID-less CardRef lookup explicitly depends, making credential and token issuance ambiguous across spaces. See `SEC-AUTH-001`. |
| Registration RBAC and audit | Agent rules transactional-audit requirements; spec REQ-092/145, INV-007, AC-018/030; Card routes/service and audit helpers | **FAIL.** Registration evaluates only `cards:write`; a binding with `on_failure` never evaluates or audits `operators:invoke`. See `SEC-AUTHZ-001`. |
| Qualifying and excluded activity grants | Security posture token lifecycle; spec REQ-105–108/112, AC-019; shared issuer, API-key exchange, workload assertion path, refresh/delegation paths | **PASS implementation / FAIL proof.** The closed grant gate admits only API-key and `jwt-bearer`, and the SQL update further requires an active Card-bound Service/Agent. Required real-boundary exclusion evidence is incomplete. See `SEC-VER-001`. |
| Current eligibility and lifecycle | Spec REQ-107/108; `verification.rs`; principal and Card lifecycle queries/tests | **PASS.** Eligibility derives on read from exact owner Card UID, active principal, active Card, and an exclusive timeout cutoff. Suspension/deletion and A/B versions are independently covered at the SQL seam. |
| Tenant isolation and transaction coupling | Agent rules; security posture tenant/data isolation; spec REQ-078/112 and INV-007; migration, query owners, registration transaction, SQL/server tests | **PASS.** The new table carries tenant identity, forced RLS, the canonical policy, and only `TenantConn` runtime operations. Projection, principal creation, Card write, and the existing `cards:write` audit row share the caller-owned transaction. |
| Card status reads | Spec REQ-134/145, AC-028/030; Card routes/service and server journey | **PASS.** Existing `cards:read` authorization/audit remains before the tenant-scoped read; binding IDs are derived under the caller tenant and cross-tenant reads remain indistinguishable from absence. |

Primary authority coverage included `AGENTS.md`,
`architecture/agent-rules.md`, `architecture/wyrd-security-posture.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, approved spec
revision 33, TASK-003, the complete base-to-candidate diff, and the surrounding
auth, registry, SQL, audit, migration, and test owners named above.

## Security Audit

### Critical

None.

### High

- **SEC-AUTH-001 — classification: VIOLATION.**
  **Violated obligation:** a credential or workload assertion must resolve one
  exact Card-bound principal; ambiguity must fail closed, and another Card
  version or identity must not activate the owner (REQ-105/106/112 and the
  security posture's credential lifecycle).
  **Location:** `crates/wyrd/wyrd-sql/migrations/20260601000027_verification_bindings.sql:19-29`,
  `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:20-38`,
  `crates/wyrd/wyrd-auth/src/issue_api_key.rs:92-99`, and
  `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:149-158`.
  **Evidence:** the existing CardRef lookup deliberately relaxes every optional
  field, including `space`, and its own security comment says the former
  `UNIQUE (data_tenant_id, name)` constraint is what bounded the query to one
  intended row and that relaxing it makes the predicate match multiple rows.
  This candidate drops exactly that constraint. `ORDER BY created_at, id LIMIT
  1` then selects the oldest matching principal rather than refusing ambiguity.
  Both API-key issuance and workload `jwt-bearer` use this lookup, while public
  `CardRef.space` remains optional.
  **Plausible exploit path:** a tenant has two Service Cards with the same kind,
  name, and version in different spaces—now valid after the migration. A
  workload-binding or key request supplies the permitted space-less CardRef for
  the intended Service. The lookup can select the older other-space principal,
  and the server returns a credential/token carrying that principal's roles and
  signed Card scope and stamps activity on that other exact Card UID. If that
  older principal is more privileged, the recipient receives unintended
  authority; otherwise the wrong workload identity is still authenticated and
  activated.
  **Impact:** cross-principal credential confusion, possible privilege
  escalation within a tenant, and activation of the wrong verification owner.
  **Testable correction:** resolve credential and workload targets by an
  unambiguous exact identity—prefer resolving the Card to its tenant-scoped UID
  and looking up `(principal_kind, card_kind, card_uid)`. Require `space` at
  these external boundaries or reject a partial CardRef that matches more than
  one principal; never select one with `LIMIT 1`. Add Postgres/API tests with
  same kind/name/version in two spaces proving exact-space success and
  space-less fail-closed behavior for API-key issuance and workload
  `jwt-bearer`.

- **SEC-AUTHZ-001 — classification: VIOLATION.**
  **Violated obligation:** REQ-145 requires `cards:write` for binding changes
  and additionally `operators:invoke` when `on_failure` is non-empty; every
  evaluated decision must append its allow or deny through the canonical
  transactional audit path before mutation. The frozen dispatch is later
  executed by SYSTEM without reevaluating end-user authority, so registration
  is the only end-user enforcement point.
  **Location:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:385-405`
  and `crates/wyrd/wyrd-server/src/components/cards/service.rs:1120-1125`.
  **Evidence:** `register_card_http` obtains only `allow_card_write`; no
  `Permission::operator_invoke()` check exists anywhere in the Card
  registration path. The service then freezes every inline or referenced
  Operator and persists it in `verification_bindings`. The current negative
  route test uses a caller with no roles, so it proves only the existing
  `cards:write` denial and cannot detect this missing permission split.
  **Plausible exploit path:** a custom tenant role grants `cards:write` but not
  `operators:invoke`. Its holder registers a Service/Agent binding with an
  HTTP or Notify `on_failure`. Registration succeeds and stores a frozen
  dispatch that the SYSTEM worker will execute after a failed verdict without
  another user authorization decision.
  **Impact:** unauthorized outbound Operator actions and missing allow/deny
  audit evidence at the sole principal authorization boundary.
  **Testable correction:** inspect the validated registration request for any
  effective non-empty `on_failure`, evaluate `operators:invoke` in addition to
  `cards:write`, and commit both allowed decisions with the registration or the
  denial without Card/binding writes. Add a real route test using a custom
  `cards:write`-only role: an Operator-free binding succeeds, an inline and a
  referenced Operator binding each return the stable 403 with a canonical
  `operators:invoke` denied row, and a caller holding both permissions commits
  both allowed audit rows atomically with the projection.

### Medium

- **SEC-VER-001 — classification: MISSING verification.**
  **Violated obligation:** TASK-003 Scenario 2, AC-019, and repository journey
  rules require real client/server proof for both qualifying authentication
  paths and the excluded grant/use paths that must never touch activity.
  **Location:** `crates/wyrd/wyrd-auth/src/issuance.rs:1003-1090` and
  `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs:2757-3022`.
  **Evidence:** the issuer-level Postgres test calls `TenantTokenIssuer::issue`
  directly for `JwtBearer`, delegation, and Card-free grants, bypassing workload
  assertion verification and route transaction ownership. The real server
  journey asserts only API-key exchange. No activity assertion drives a real
  workload `jwt-bearer`, human refresh/OIDC login, cached-token request, or
  ordinary observation path; SYSTEM is deferred because its issuance path does
  not yet exist.
  **Impact:** a routing or transaction regression that classifies a real
  workload assertion incorrectly, or touches activity during refresh, cached
  bearer use, or observation handling, can leave all TASK-003 tests green even
  though the owner is spuriously activated or renewed.
  **Testable correction:** extend the narrow real-server auth journey to assert
  `last_authenticated_at` and cursor behavior after one successful configured
  workload `jwt-bearer` exchange and after representative existing excluded
  paths (human refresh, cached bearer request, and ordinary observation). Keep
  the direct issuer tests as supporting exhaustive grant-enum checks; add the
  SYSTEM case when its server-only mint path lands in TASK-004.

### Low / Defense In Depth

None.

### Positive Controls

- `TenantGrant::records_owner_activity` is a closed grant classification that
  admits only API-key and workload `jwt-bearer`; delegation, OIDC login, and
  refresh cannot accidentally inherit activity from a broad “machine” check.
- `record_machine_authentication` updates only an active Card-bound Service or
  Agent, derives the owner UID from that row, and arms only null schedule
  cursors. Later exchanges renew activity without postponing existing work.
- Issuance activity, token-exchange audit, Card-scope mint audit, credential
  bookkeeping, and the caller-visible token response remain transactionally
  coupled: callers return the token only after `TenantConn::commit` succeeds.
- `binding_activity` re-reads current principal and Card lifecycle state, so a
  still-valid five-minute token cannot bypass immediate suspension or deletion
  for new binding work.
- `verification_bindings` uses forced RLS and `wyrd.current_tenant()` for both
  visibility and writes; cross-tenant tests prove read absence and failed
  projection onto a foreign owner.
- Card status hydration reuses the existing audited `cards:read` routes and
  derives binding IDs from tenant-scoped state instead of trusting authored
  status or persisting duplicate status JSON.

## Verification evidence and limits

The immutable TASK-003 record reports the cards, principals, SQL, Wyrd,
journey, tenant-isolation, transaction-coupling, pool-boundary, codegen, format,
lint, Hakari, and diff checks green at the candidate. I inspected the changed
tests and relevant pre-existing auth/route tests but did not repeat those broad
suites. No existing test constructs same-identity Card-bound principals across
spaces, a `cards:write`-without-`operators:invoke` registration caller, or the
real-boundary excluded activity cases described above; aggregate green results
therefore cannot close these findings.

This review did not require the TASK-004 SYSTEM mint path, scheduler/runner,
Eval enqueue, result tables, or Operator worker implementation. It reviewed
the frozen authority those later consumers will use and the admission query
they will call, not their not-yet-delivered mechanics.

## Overall result

**FAIL.** Tenant RLS, current lifecycle eligibility, Card-read authorization,
and qualifying-grant classification are structurally sound, but the candidate
can resolve a partial CardRef to the wrong Card-bound principal after removing
the lookup's uniqueness invariant, and it persists future SYSTEM-executed
Operators without the required `operators:invoke` authorization or audit.
