---
id: TASK-002-R10-HUMAN
kind: remediation
status: ready
parent_task: TASK-002
spec: SPEC-oidc-production-readiness
spec_revision: 5
requirements: [REQ-005, REQ-007, REQ-009, REQ-011, REQ-013, INV-001, INV-004, AC-004, AC-007]
directed_by: human, 2026-09-26
---

# Human-directed TASK-002 drift remediation

This direction supplements the R10 documentation corrections. It is not a
validated R10 finding ledger. Preserve the original TASK-002 base and task
contract; review the cumulative candidate after implementation.

## 1. One slug resolver and only scoped SQL capabilities

Login calls `WyrdPostgres::resolve_tenant_slug`, while workload exchange and
boot call `resolve_by_slug_for_app` through `&PgPool`. Boot also passes raw
pools to issuer and binding seeding. `wyrd-auth` passes raw pools to failure
audit and card-scope audit and stores one in `PgWorkloadBindingResolver`.
`check:from-pools-allowlist` guards pool construction sites, not these SQL
capabilities; `check:tenant-isolation` still describes raw pools as valid for
some query signatures. This permits another auth caller to bypass the intended
boundary while the checks remain green.

Use one slug-resolution operation owned by `WyrdPostgres`, backed by the
existing `platform.resolve_tenant_by_slug` SQL function. Pre-tenant lookup
uses `OperatorPool`; tenant work uses `TenantConn`. If the operator connection
is absent, fail closed rather than using an app-pool fallback. Remove the
second Rust resolver and all raw-pool signatures or fields in the touched
auth, boot, and SQL paths. Reuse existing `WyrdPostgres::tenant_conn` for
tenant transactions and the canonical audit append; do not introduce another
pool wrapper, slug query, audit path, or lock service. Keep the login endpoint's
generic unknown/inactive-tenant refusal and workload tenant binding.

Update the existing `check:tenant-isolation` boundary so production query and
auth signatures/fields accept only `OperatorPool` or `TenantConn`. Reconcile
its stale raw-pool allowlists with `architecture/agent-rules.md`. Keep
`check:from-pools-allowlist` for its distinct construction property. Prove
login, workload, and boot resolve the same active slug and refuse an inactive
or missing slug; run both existing boundary checks. Pool construction inside
the SQL connection owner remains its private implementation, never a
capability handed to domain code.

## 2. Accept the shared JWT correction

`ExternalVerifier::verify_external_against` now requires `exp`, `iss`, and
`aud` and validates a present `nbf`. This is an accepted security correction,
not a defect to remediate. A trusted signature without a required audience
does not show that the token was meant for Wyrd. RFC 7523 requires `iss`,
`sub`, `aud`, and `exp` for JWT bearer assertions and forbids accepting a
present future `nbf`; OIDC Core requires the corresponding ID-token checks.
Keep the one shared signature/JWKS, issuer-key, audience, and algorithm
verification path. Keep ID-token-only `iat`, subject-format, `azp`, and nonce
rules on the existing ID-token path. Do not relax the shared verifier or build
a second workload verifier. The only follow-up is to retain focused regression
proof that a valid workload assertion succeeds and missing `iss`/`aud` or
future `nbf` is refused; reuse existing tests where they already prove it.
Use OIDC Core §3.1.3.7, RFC 7523 §3, and RFC 8725 §3 as the checklists.

## 3. One refresh-family serialization rule

The refresh, revocation, initial issuance, and callback-role fixes all share
one cause: a writer of User session or role authority can race another
writer unless both take the existing family lock. List every production writer
of User status, roles, sessions, and refresh rows, then make each use the
existing family lock before the connection lock and before reading authority.
Keep transaction ownership with the caller and RLS on `TenantConn`; do not
add a new lock mechanism. Prove the two issuance/revocation orders and
concurrent callbacks cannot union roles. Reuse one existing lock-wait test
helper for equivalent Postgres observations and delete duplicate helpers.

## 4. Enforce approved sealing-key and handoff contract

Approved spec revision 5 resolves the former REQ-005 conflict: the deployment
keyring protects provider secrets and recoverable login/session credentials,
including secretless-provider login. Keep the existing encrypted, two-minute,
one-use completed-credential handoff for browser and CLI, using the same
keyring and tenant-scoped SQL path. Do not store the token pair in plaintext,
put it in a redirect or browser page, introduce a second key or auth path, or
make machine authentication depend on human-login key material.

For TASK-002, verify callback issuance and sealed completion remain atomic,
the completed row expires, redemption consumes it only once, tenant and
initiator bindings are retained, activation and login with a missing or
unusable key fail closed even for a secretless provider, and retained keys
can open in-flight completions during rotation. Preserve the existing
provider-secret rotation proof and independent machine authentication. The
browser BFF's authenticated flow-cookie redemption and the CLI's
verifier-held claim are owned by
TASK-003 and TASK-004 respectively; TASK-002 cannot claim their end-to-end
security before those tasks pass. Do not build those later surfaces in this
remediation.

## 5. Keep aligned behavior and isolate unrelated fixes

Keep email non-unique within a tenant: `(issuer, sub)` is the identity and a
same-email replacement-provider login creates a separate User. Preserve the
provider-switch journey. Keep unrelated gateway, Forge, and router test fixes
only where needed for required green gates; identify them in separate commits
and evidence, and remove incidental changes. Complete the two R10 rustdoc
findings in the parent remediation packet.

## Completion evidence

- A source inventory of every slug-resolution caller, raw-pool signature or
  field, and User authority writer, with the surviving owner for each.
- Focused OIDC ID-token, RFC 7523 workload, slug refusal, and concurrency
  checks, using the existing test lanes and exact focused commands for named
  Rust tests.
- `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation`,
  `mise run check:from-pools-allowlist`, and the affected identity journeys.
- Evidence for REQ-005 under approved spec revision 5: missing-key refusal,
  sealed one-time completion, expiry, and key rotation without losing
  in-flight completions. Record TASK-003/004 initiator checks as pending
  until their real browser and CLI journeys pass; do not count earlier green
  gates or the R10 verdict as proof of this new work.
