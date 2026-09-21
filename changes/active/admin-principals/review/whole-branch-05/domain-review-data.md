# Data, Tenancy, Durability, and Concurrency Review

## Reviewed Boundary

Fresh Wave-1 review of candidate
`c9e1092bbdb4df3781eb91b0eb33150e00df7623` against base
`c5c20754a167e8f4d74a555a720bd51df6179a6f`, limited to tenancy,
persistent data, durability, transaction ownership, concurrency, and Bifrost
audit retention.

The approved authority is `SPEC-admin-principals` revision 10, status
`approved`, SHA-256
`05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.
I read all original `TASK-001` through `TASK-008` packets, the complete
whole-branch-04 verdict and data findings, its validated ledger, and
`TASK-001-008-R4-close-cumulative-findings.md`. I received no other
whole-branch-05 review conclusion or intended verdict.

## Authority and Source Coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Tenant SQL isolation | `AGENTS.md` §§2–6 and §§9–12; `architecture/agent-rules.md`; `architecture/v1/00-foundations/{tenancy,sql-foundation,postgres-layout}.md`; `architecture/wyrd-security-posture.md`; `TenantConn`; `OperatorPool`; new Wyrd migrations and auth/platform query modules; tenant-isolation checks and Postgres tests | PASS. Tenant rows remain behind `TenantConn` and RLS; platform rows remain behind the explicit BYPASSRLS `OperatorPool`; no third connection abstraction or reachable cross-tenant tenant-data query was found. |
| Initialization | `REQ-020`–`REQ-024`, TASK-003, R4-6; `boot/init.rs`, `main.rs`, platform principal/grant/credential transaction queries, and `platform_admin_e2e` initialization failure/concurrency proofs | PASS. The unique root, grant, and verifier share one transaction. Disclosure is written and flushed before commit; writer failure rolls back, and concurrent initialization has one durable winner. |
| Provisioning and admission | `REQ-025`–`REQ-028`, `AC-007`, TASK-004, R4-8; platform routes, `TenantProvisioning`, provisioning SQL, tenant lifecycle/admission migration, and the real all-stage failure/retry and racing-slug journeys | PASS. The platform claim commits only a non-admitted row; tenant principal/role/grant/credential commit together; active promotion follows; every error path retires the directory row to failed when possible; retry adopts the original tenant id and revokes any undisclosed prior credential. |
| Tenant-admin recovery | `REQ-032`, TASK-006; platform route, `TenantRecovery`, directory status check, tenant-admin lookup, credential insert, and recovery journeys | PASS. Recovery is separately authorized, accepts only an active directory row, uses a tenant-bound transaction, reuses the existing administrator and grants, and creates only a replacement credential. |
| Audit staging and attribution | AGENTS canonical-audit rules; `AC-009`; `architecture/bifrost-design.md` audit-retention contract; `AuditEvent`, `append_audit`, staging row/migration, platform and tenant authorization writers, and credential-attribution tests | PASS apart from the migration finding below. One append owns the per-tenant `FOR UPDATE` chain serialization; the current preimage includes nullable `credential_id`; the row and chain head share the deciding transaction; platform events use `SYSTEM_OWNER` through `begin_platform_audited`. |
| Publication, concurrency, and recovery | `architecture/bifrost-design.md` §§audit publication and platform sentinel; analytical reliability and OLAP references; `AuditPublisher`, freeze/read/settle SQL, projection, Scribe batch fence, active-tenant/system-sentinel directory enumeration, and publication/restart/competing-publisher journeys | PASS. A durable frozen upper bound produces one deterministic tenant/range batch identity; Scribe dedup absorbs replay; settlement advances the monotonic watermark, conditionally clears the matching bound, and deletes the staged prefix in one transaction. The active system sentinel is included in sweeps. |
| Arrow/Iceberg audit schema | `architecture/references/domain/{iceberg,olap-serving}.md`; catalog registration and strict schema matching; `AuditLogTable`; audit projection; reordered-schema and current publication proofs | PASS. Projection order exactly matches the current 14-field declaration, nullable credential values retain their shape, and generic physical matching is positional and fail-closed. No pre-release Iceberg evolution or legacy hash branch remains. |
| Migration durability | `architecture/v1/00-foundations/sql-foundation.md`; `architecture/operations/deployment-and-release.md`; base-to-candidate migration diff; Wyrd/Vala migration boot code | **FAIL — `DATA-05-1`.** |

The complete base-to-candidate diff was inventoried. Detailed source tracing
covered every changed file in this sensitive boundary and the reachable owners,
callers, migrations, and tests needed to judge it. Changes outside this domain
were not reviewed for acceptance.

## Material Proposed Findings

### `DATA-05-1` — VIOLATION / persistent migration checksum regression

- **Violated obligation:** The active SQL foundation requires migration files
  to be immutable, ordered, and checksum-verified, and requires restore/release
  tooling to reject a modified applied migration
  (`architecture/v1/00-foundations/sql-foundation.md:117-129`). The deployment
  authority repeats that migrations are immutable and that startup/release
  verifies their checksums
  (`architecture/operations/deployment-and-release.md:128-151`). This is a live
  persistent-data invariant, not legacy-audit compatibility.
- **Exact location:**
  `crates/vala/vala-sql/migrations/20260802000000_vala_audit_staging.sql:58-63`.
- **Evidence:** Migration `20260802000000` already exists in the immutable base
  without `credential_id`. The candidate edits that old file to add the column,
  while deleting the later additive migration. Both production construction
  paths run the embedded migration set before opening runtime pools:
  `crates/wyrd/wyrd-sql/src/postgres.rs:69-85` and
  `crates/vala/vala-sql/src/postgres.rs:72-91`. SQLx records the old migration
  checksum in any database migrated by the base, so the candidate presents a
  different checksum for the same version. The R4 packet's statement that the
  audit shape was not released removes the need for application/Iceberg legacy
  compatibility, but it does not make an already-committed, checksum-verified
  migration mutable under the active SQL authority.
- **Observable consequence:** Any persistent database that applied the base
  migration refuses candidate startup/migration with checksum drift before the
  new principal, audit, provisioning, or recovery behavior can serve. The
  recorded fresh-database SQL and journey lanes cannot expose this because they
  create the database from the candidate migration set rather than upgrade a
  base-migrated database.
- **Required testable correction:** Restore
  `20260802000000_vala_audit_staging.sql` byte-for-byte to its base content and
  add `credential_id` through a new forward-only Vala migration. Keep the
  deleted application/Iceberg legacy-fingerprint, schema-evolution, old-field-ID,
  and alternate-hash machinery deleted; none is needed for this SQL correction.
  Add one focused upgrade proof that migrates a database through the immutable
  base Vala set, runs the candidate migrations without checksum drift, and
  verifies both nullable `credential_id` and current audit append/publication.

## Verification Limits

- This was a static, review-only audit. I did not modify candidate source or
  rerun builds, migrations, or tests.
- The R4 implementation packet records green format/lint, tenant-isolation,
  SQL, platform journey (twice), Bifrost Redux/server/SQL integration, current
  audit hash/publication, initialization writer-failure, and all-stage
  provisioning proofs. Those results credibly support the paths they execute.
- Those lanes build fresh schemas from the candidate files. No supplied result
  upgrades a database whose `_sqlx_migrations` row contains the base checksum
  for `20260802000000`, so they do not close `DATA-05-1`.
- The approved spec checksum was rechecked. Candidate HEAD was rechecked before
  writing this report and remained
  `c9e1092bbdb4df3781eb91b0eb33150e00df7623`.

## Overall Result

**FAIL**

The tenancy, transaction, initialization, provisioning, recovery, audit-chain,
publication-watermark, and current Iceberg-schema paths satisfy this review's
boundary. The candidate nevertheless mutates an existing checksum-verified
Vala migration, making upgrades from the immutable base fail before the system
can start.
