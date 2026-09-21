# Data, tenancy, and audit domain review

## Subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 12
- Reviewed boundary: `TenantConn`/RLS and `OperatorPool`, administrative transactions, provisioning and recovery durability, migrations and checksum behavior, canonical audit staging and retained-history publication, and the R7 audit-publisher contention change.

## Authority and source coverage

| Boundary | Governing authority | Source traced | Result |
|---|---|---|---|
| Tenant SQL and transaction ownership | `AGENTS.md` sections 2, 3, 9, 11; `architecture/agent-rules.md`; `architecture/v1/00-foundations/{tenancy,postgres-layout,sql-foundation}.md` | New/changed `wyrd-sql` auth and platform query modules; principal, credential, provisioning, recovery, OIDC-role, and token-issuance callers | **FAIL** — `DATA-WB08-02` |
| Platform SQL and audited authorization | `REQ-018`, `REQ-025`–`REQ-033`, `REQ-037`; canonical audit rules | `OperatorPool::begin_platform_audited`, `PlatformAuthorization`, platform identity/credential/provisioning/recovery owners, platform migrations | PASS |
| Tenant provisioning and recovery durability | `REQ-021`–`REQ-033`, `INV-005`–`INV-008`; SQL transaction discipline | Initialization, claim/establish/promote/fail/resume flow, recovery flow, durable uniqueness and lifecycle constraints, corresponding Postgres/journey evidence | PASS |
| Audit staging, projection, and retirement | Canonical append/publisher rules; `architecture/operations/reliability-and-recovery.md`; `architecture/bifrost-design.md` | `append_audit`, staging migrations and row type, `AuditPublisher`, `freeze_publication_range`, projection, Scribe append, settlement and retirement | **FAIL** — `DATA-WB08-01` |
| Publication contention and replay | Audit frozen-bound and idempotent replay authority | `FOR UPDATE NOWAIT` freeze, bounded concurrent sweep, frozen-range replay and stalled-tenant journeys | PASS for the changed behavior; see verification limit below |
| Migration ordering/checksums | Postgres layout and deployment migration authority | Wyrd migrations `20`, `22`–`25`; Vala migrations `26`–`27`; migration tests and shipped-staging checksum pin | PASS except for the retained-audit compatibility gap in `DATA-WB08-01` |

## Material findings

### `DATA-WB08-01` — REGRESSION — credential attribution makes existing retained audit history incompatible

- **Violated obligation:** `REQ-037`, the one canonical durable audit-history contract, and the recovery requirement that existing audit chains remain verifiable and publishable across an upgrade.
- **Exact location:** `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:37-63`; `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:901-923,956-980`; `crates/vala/vala-sql/src/queries/audit_staging.rs:353-385`; `crates/vala/vala-sql/migrations/20260910000027_audit_staging_credential_id.sql:1-16`.
- **Evidence and reachability:** The candidate appends nullable `credential_id` to the built-in `vala.system.audit_log` schema, changing its user-schema fingerprint. Every already-created audit table has the predecessor fingerprint, while `BifrostCatalog::ensure_builtin` rejects any existing row whose fingerprint differs before publication. The next audit publication for such a tenant therefore fails with `FingerprintMismatch`, leaves its frozen staging range owed, and repeats forever. Separately, the candidate changes the hash preimage even when `credential_id` is absent by appending `push_opt(None)`; pre-upgrade rows do not contain that segment, and both old and new credential-free rows store `NULL`, so the stored columns no longer identify which preimage reproduces the row. The SQL migration test proves only that an old staging row survives `ALTER TABLE`; it never opens an existing 13-column Bifrost audit table or reproduces its hash.
- **Observable consequence:** Upgrading a deployment that already retained audit history stops further retention for that tenant, and its pre-upgrade hash-chain entries are no longer reproducible under the candidate's documented canonical encoding.
- **Required correction:** Add an explicit, one-way evolution for the built-in audit table that recognizes the predecessor fingerprint, adds the optional `credential_id` field to its Iceberg schema, and advances the catalog fingerprint without accepting arbitrary schema drift. Keep the predecessor hash encoding for rows without a credential and append a credential segment only when one is present, so old and new `NULL` rows have the same reproducible preimage. Prove the correction by creating the predecessor audit table and old hash-chained rows, upgrading, publishing both old credential-free and new credential-attributed decisions, and querying both from one retained history without a fingerprint conflict or unverifiable hash.

### `DATA-WB08-02` — VIOLATION — new tenant queries duplicate the RLS tenant predicate

- **Violated obligation:** `REQ-031`, `AC-010`, `AGENTS.md`, and `architecture/agent-rules.md` require `TenantConn` paths to use forced Postgres RLS as the tenant boundary and prohibit parallel hand-written tenant filters.
- **Exact location:** `crates/wyrd/wyrd-sql/src/queries/auth/api_keys.rs:64-79,93-108`; `crates/wyrd/wyrd-sql/src/queries/auth/role_assignments.rs:34-50`; `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:231-247`.
- **Evidence and reachability:** These candidate-added statements are reached by served credential listing/revocation, OIDC role replacement, tenant provisioning retry, and tenant-admin recovery, and each runs on a tenant-bound `TenantConn` while also filtering with either `data_tenant_id = $n` or `data_tenant_id = wyrd.current_tenant()`. Inserts still need to supply their durable tenant key and composite joins still need tenant-qualified join conditions; the redundant `WHERE` predicates do not.
- **Observable consequence:** Tenant scope has two independently maintained expressions on new production paths, directly violating the repository's load-bearing RLS boundary even though both expressions agree today.
- **Required correction:** Remove only the redundant tenant `WHERE` predicates and their now-unused tenant binds from the four new statements, preserving principal/credential/role predicates, required inserted tenant keys, and composite tenant joins. Prove same-tenant success and cross-tenant invisibility through `TenantConn`, then run `mise run check:tenant-isolation`, `mise run test:sql`, and the owning principal/identity journeys.

## Verification limits

- I inspected the cumulative source and diff and the appended implementation evidence; I did not rerun the already-recorded broad lanes in this review-only role.
- The evidence has no upgrade journey beginning with an already-registered predecessor `vala.system.audit_log`; all retained-history proof starts from a fresh candidate schema, so it cannot detect `DATA-WB08-01`.
- The `FOR UPDATE NOWAIT` contention proof covers a held lock followed by release and exact frozen-range replay. It does not establish progress under continuously contended chain-head writes, but I found no additional task-scoped defect strong enough to retain as a finding from that limit alone.
- HEAD was rechecked after review and remained `eb9b2f69cb883fa508ed168f21cb868451e61b82`.

## Overall result

**FAIL** — two material, reachable data/audit findings remain.
