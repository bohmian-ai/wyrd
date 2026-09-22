# Data, tenancy, and audit domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 13
- Review scope: Postgres connection ownership, forced RLS, transaction
  ownership, authorization-decision audit durability and attribution,
  delegation audit, tenant provisioning/no-effect paths, audit publication,
  and the retained `FOR UPDATE NOWAIT` behavior.

## Review Findings

### Critical

None.

### Important

- **`DATA-R9-01` · VIOLATION** —
  [`crates/wyrd/wyrd-auth/src/issuance.rs:533`](../../../../../../crates/wyrd/wyrd-auth/src/issuance.rs)
  Successful credential-backed delegation commits an allowed
  `delegation:issue` row with `credential_id = NULL`: the verified caller still
  has its non-secret credential id in `Principal::credential_id`, and the
  denied and allowed-no-effect path preserves it at
  `exchange_api_key.rs:331-345`, but `DelegateToken::mint` projects the caller
  into `DelegationCaller` without that field and the successful
  `exchange_audit_event` returns without calling `with_credential_id`; this
  violates `REQ-037` and `AC-009`, makes a successful authorization decision
  unattributable among the principal's concurrently valid credentials, and is
  irreversible once `append_audit` hashes and stores the null field. Preserve
  the authenticated caller credential id only in the successful decision's
  audit context and attach it to the reused token-exchange event; do **not** put
  it into the newly delegated JWT, whose credential attribution must remain
  `None`. Prove closure with a real-Postgres successful delegation from a
  verified caller token carrying a known credential id: exactly one allowed
  `delegation:issue` staging row must name that caller and credential, while
  the resulting delegated token still carries no credential id.

### Suggestions

None.

## Authority and source coverage

| Boundary | Authority | Source/evidence inspected | Result |
|---|---|---|---|
| Tenant SQL isolation | `AGENTS.md` §§2, 9, 11; `architecture/agent-rules.md`; `REQ-031`, `INV-007` | `TenantConn`; auth principal/key/role queries; `dc7e6cc31`; `tenant_principal_queries_are_confined_by_row_level_security` | PASS — the four R8 predicates are removed, inserts retain tenant keys, composite joins retain tenant equality, and cross-tenant invisibility is proved under forced RLS |
| Connection ownership | `architecture/agent-rules.md` two-boundary rule | Changed platform and tenant stores, `TenantConn`, `OperatorPool`, provisioning/auth owners | PASS for the changed admin-principal paths |
| Transaction ownership and no-effect decisions | `architecture/agent-rules.md`; `REQ-037`; `AC-009`; `REQ-012c`; `AC-020` | `DelegateToken::execute`, `TenantTokenIssuer::issue`, platform authorization/provisioning, principal/credential no-effect paths, canonical append | PASS except `DATA-R9-01` — callers own commit/rollback; denied and allowed-no-effect delegation decisions commit once; audit failure fails closed |
| Delegation attenuation | `REQ-012c`, `INV-013a`, `AC-020` | `PermissionSet::intersection`; `TenantGrant::Delegation`; issuer and Postgres tests | PASS — resulting authority is the caller/target semantic intersection |
| Audit persistence and hash | Audit doctrine and security posture; `REQ-037`, `AC-009` | `AuditEvent`, `append_auth_audit`, `append_audit`, staging row, entry hash, retained projection | FAIL only for `DATA-R9-01`; the persistence path correctly stores and hashes whatever credential attribution the producer supplies |
| Audit publication concurrency | Audit doctrine; analytical reliability authority; explicit human disposition | `freeze_publication_range`, `AuditPublisher::publish_logged`, frozen-range replay journey, commit/settle sequencing | PASS — retain `67b4d0ba`; `NOWAIT` makes contention fail only that tenant's cycle, the sweep continues, and the committed frozen bound preserves replay identity |
| Audit schema compatibility | Explicit human disposition: Wyrd is unshipped | Current migrations, staging/hash/projection shape | PASS — no predecessor-schema or predecessor-hash compatibility mechanism is required |

## Open Questions

None. The correction does not require a new product, schema, public API,
tenancy, transaction, or concurrency decision.

## Verification Notes

- The R8 evidence records passing focused Postgres proofs, `test:sql`, tenant
  isolation, platform journeys, audit-failure closure, and delegation
  allow/deny/no-effect behavior on candidate `84b7f4ad6`.
- Existing delegation tests query only the decision outcome and mint helper
  tokens with `credential_id: None`, so they cannot detect `DATA-R9-01`.
- Manual tenant predicates that predate the review base were not broadened into
  this remediation; the R8-owned four-statement correction and its RLS proof
  were audited directly.
- Candidate HEAD was rechecked as
  `84b7f4ad6eeaf2f20006761c35e34ce8d51ce7f6` after inspection.

## Overall result

**FAIL** — one bounded audit-attribution defect remains (`DATA-R9-01`).
