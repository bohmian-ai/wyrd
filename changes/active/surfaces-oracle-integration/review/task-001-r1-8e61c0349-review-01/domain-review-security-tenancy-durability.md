# TASK-001-R1 security, tenancy, and durability review

## Reviewed boundary

- Base: `8377fff9f03cc60de4be3e088569e38984382dc4`
- Candidate: `8e61c03493f5c0ebcc12cdb4a9998df284f7fc8f`
- Domain: RBAC verdict ownership, tenant identity/RLS, transactional audit,
  audit staging/publication, replay, and durable catalog writes
- Result: **FAIL**

The review traced the changed permission paths from handlers through their
configured checker, audit staging/transactions, publisher projection, Scribe
ingress, tenant binding, and retained audit table. It also traced the changed
Bifrost and Card mutation paths through their authoritative transactions.

## Authority and source coverage

| Boundary | Authority | Source coverage | Result |
|---|---|---|---|
| Received permission verdicts | REQ-026, REQ-027C, Wyrd security posture | server audit helpers, admin, Bifrost, Card, Eval, MCP, peer/tail paths | FAIL on reachable post-verdict exits |
| System-tenant attribution and retention | REQ-026B, REQ-027, Bifrost design | tenant directory, audit projection, publisher, Scribe ingress, tenant table binding | FAIL |
| Transactional coupling | INV-008C and transaction authority | Bifrost catalog and Card service transactions | FAIL for three Card writes and Bifrost alternate exits |
| Tenant isolation/RLS | INV-008D and SQL capability rules | `OperatorPool`, `TenantConn`, active-tenant queries, RLS predicates | PASS |
| Replay and chain integrity | REQ-027 and reliability authority | frozen range, publisher identity, Scribe batch fence, settlement | PASS for ordinary tenants |
| Authorization ownership | configured permission-check authority | shared audit helper and service-account callers | FAIL |
| Engine lineage versus security audit | Wyrd design and REQ-026 | Scribe, Forge, reconciliation, storage, login paths | PASS |

## Material proposed findings

### DOMAIN-R1-01 — System-owner audit is permanently unpublishable

- Obligation: system-attributed peer/tail security decisions must retain in the
  canonical tenant hash chain.
- Evidence: the active directory maps nil to `SYSTEM_OWNER`, while
  `tables/audit/projection.rs`, `scribe/ingress.rs`, and
  `catalog/tenant_table.rs` reject nil. Peer/tail unverified rejection owners
  append using that system tenant.
- Consequence: required security evidence accumulates in staging and never reaches
  retained history.
- Testable correction: allow the sentinel only for trusted internal canonical
  audit publication; publish one real system rejection exactly once, drain its
  staging rows, and retain a negative non-audit nil test.

### DOMAIN-R1-02 — Bifrost create has unconsumed post-verdict exits

- Obligation: one verdict per request, coupled to the catalog mutation where its
  transaction exists.
- Evidence: `register_table` evaluates before catalog creation; catalog validation,
  physical IO, and the concurrent-existing return occur before its late append.
- Consequence: an Allowed registration can fail or race without a durable decision.
- Testable correction: keep a single event through the existing catalog owner;
  consume it in a usable transaction or once at the standalone failure boundary.

### DOMAIN-R1-03 — Trusted issuer performs outbound IO before audit

- Obligation: the received permission decision must be durable before proceeding.
- Evidence: trusted-issuer create performs OIDC discovery and fallible sealing
  after evaluation and before append.
- Consequence: reachable external-IO failure loses the verdict.
- Testable correction: append Allowed standalone before discovery, create no issuer
  on discovery/sealing failure, and verify exactly one decision. Do not span the
  network call with a database transaction.

### DOMAIN-R1-04 — Card writes use a separate audit transaction

- Obligation: Allowed audit shares an authoritative operation transaction where
  one exists.
- Evidence: Card registration and both deletion routes use standalone authorization
  before their service-owned SQL transactions.
- Consequence: audit can commit while the mutation rolls back.
- Testable correction: pass the event into those existing transactions and force
  rollback failures; leave reads, completion, and storage sagas standalone.

### DOMAIN-R1-05 — Service-account helper bypasses configured RBAC

- Obligation: the configured `PermissionCheck` owns runtime authorization.
- Evidence: the shared helper calls `require_service_accounts_write` directly.
- Consequence: an injected checker can disagree with the recorded/executed verdict.
- Testable correction: evaluate through `state.authz.permission_check`, retain the
  current public denial details, and verify effect and audit follow that verdict.

## Passed high-risk properties

- Ordinary-tenant reads and writes retain tenant-scoped RLS and repository SQL
  capability owners.
- Frozen upper bounds, monotonic settlement, deterministic batch identity, and
  Scribe durable dedup remain intact.
- Gate composition is statically typed; no dynamic audit sink entered production.
- R1 removes engine mechanics from canonical security audit without replacing
  their operational lineage.
- Oracle query authorization still requires local WAL acceptance before rows may
  be returned.

## Verification limits

Passing SQL, Redux/server integration, and journey lanes exercise ordinary tenants
and happy/fail-closed audit injections. They do not exercise the system sentinel,
the Bifrost pre-append/race exits, OIDC failure after permission, Card operation
rollback coupling, or disagreement with the configured checker.
