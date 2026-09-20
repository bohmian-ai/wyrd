# Data, tenancy, persistence, and concurrency review

## Review Findings

### Critical

- **DATA-R3-1 — REGRESSION — retained audit publication cannot cross the
  `credential_id` schema change.**

  **Violated obligation.** `FIND-admin-principals-R2-3` requires the
  authenticating credential id to reach retained `vala.system.audit_log`
  without replacing the canonical audit path. The repository also treats a
  built-in table registration as immutable and refuses a changed schema
  fingerprint.

  **Exact evidence and reachable path.**
  `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:44-60` adds the
  nullable `credential_id` field to `AuditLogTable::arrow_fields`, changing the
  user-schema fingerprint. Every audit publication enters Scribe with this
  built-in table; `scribe/ingress.rs:183-195` calls `ensure_builtin`, which
  rebuilds that current fingerprint (`catalog/bifrost_catalog.rs:901-923,
  956-963`). For an already-registered `audit_log`,
  `create_table_locked` compares the stored old fingerprint and returns
  `FingerprintMismatch` at `catalog/bifrost_catalog.rs:977-980`. There is no
  schema-evolution or registration migration in the candidate. The Postgres
  migration at
  `crates/vala/vala-sql/migrations/20260910000027_audit_credential_id.sql:15`
  changes only `vala.audit_staging`; it cannot update the per-tenant catalog or
  Iceberg table.

  **Observable consequence.** A fresh deployment publishes the new column, but
  an upgraded deployment whose audit table already exists rejects its first
  post-upgrade publication. Staging then accumulates while retained audit
  history stops advancing for every affected tenant, including the system audit
  tenant.

  **Required testable correction.** Preserve the one audit table and publisher,
  but provide an approved additive evolution path for the existing built-in
  registration and physical Iceberg table before Scribe validates the new
  fingerprint. Prove it from a pre-change registered `audit_log`: upgrade,
  publish rows with and without `credential_id`, restart/replay, and read both
  historical and new rows. The repository currently declares built-in schemas
  immutable, so if additive evolution is not already an approved persistent-data
  decision this correction needs specification/architecture resolution rather
  than a local bypass of the fingerprint check.

### Important

- **DATA-R3-2 — INCORRECT — the unversioned audit hash encoding makes historical
  rows unreproducible after upgrade.**

  **Violated obligation.** The canonical append documents that an entry hash is
  reproducible from its stored columns
  (`crates/vala/vala-sql/src/queries/audit_staging.rs:345-349`), and the security
  authority requires minimal immutable hash-chain evidence to survive schema
  evolution. `FIND-admin-principals-R2-3` required `credential_id` to be covered
  by the canonical hash, not to invalidate earlier evidence.

  **Exact evidence.** Before this remediation, `entry_hash` encoded
  `principal_kind` immediately followed by `permission`. Current
  `audit_staging.rs:357-373` always inserts `push_opt(credential_id)` between
  them, and `push_opt(None)` writes a byte. Migration 27 leaves every existing
  `entry_hash`, `prev_hash`, and chain head untouched while asserting at lines
  11-14 that old rows are unaffected. Thus a pre-migration row with SQL
  `credential_id IS NULL` cannot be recomputed by the current encoding: the
  current preimage has an extra absence byte. After Iceberg schema evolution,
  an old file's missing column is also projected as null, so absence does not
  identify which hash recipe produced the row.

  **Observable consequence.** Historical rows remain linked by their stored
  bytes, but the repository can no longer reproduce or independently validate
  those hashes from retained columns. A future verifier cannot distinguish a
  legitimate pre-upgrade row from a current row with no credential, weakening
  the immutable chain evidence the audit design promises.

  **Required testable correction.** Version the canonical audit-hash recipe (or
  retain an equally unambiguous existing version signal) so old rows use the
  old preimage and new rows bind `credential_id`. Do not rewrite retained
  history or silently omit the credential from new hashes. Seed a row and chain
  head with the old recipe, apply the upgrade, append both `Some(id)` and `None`
  events, then recompute every entry and verify the continuous chain across the
  boundary. Selecting that durable version representation is a persistent-data
  decision if no approved mechanism already exists.

- **DATA-R3-3 — VIOLATION — stable `FIND-admin-principals-2` remains open
  because live workflow structs still retain `WyrdPostgres`.**

  **Violated obligation.** `architecture/agent-rules.md:6` permits exactly
  `TenantConn` and `OperatorPool` in library function signatures and struct
  fields; it reserves `WyrdPostgres` for constructing pools. The R2 remediation
  task repeats this exact acceptance condition and requires broad
  `WyrdPostgres` fields to be replaced.

  **Exact evidence and callers.**
  `crates/wyrd/wyrd-server/src/components/platform/provisioning.rs:93-116`
  stores `postgres: WyrdPostgres` on `TenantProvisioning`, and
  `components/platform/recovery.rs:36-58` does the same on `TenantRecovery`.
  Both are live dependency-owning server workflows, constructed from `AppState`
  and used by the served tenant-provisioning and administrator-recovery routes.
  Commit `3dc2397bc` correctly made raw SQLx transactions crate-private and
  converted exported query functions to `TenantConn`, but it did not close this
  retained-owner half. The remediation evidence itself acknowledges the fields
  remain while marking the finding PASS.

  **Observable consequence.** These changed library owners retain a connection
  constructor broader than either approved capability, so future methods can
  acquire arbitrary tenant transactions and the repository's type boundary no
  longer constrains their authority. The current source directly contradicts
  the claimed acceptance result even if each current method happens to call
  only `tenant_conn`.

  **Required testable correction.** Reconcile the authority before changing the
  code. Under the current wording there is no long-lived approved field that can
  dynamically acquire a `TenantConn`: storing `WyrdPostgres` is forbidden, a
  raw pool/new wrapper is forbidden, and a transaction cannot be retained
  across requests. Use an already-approved composition boundary if one exists;
  otherwise revise the connection-owner rule rather than inventing a third
  abstraction or weakening the source check. The closure proof must inspect
  struct fields and function signatures, not only raw SQLx transaction text.

### Suggestions

None. Optional cleanup is outside this acceptance audit.

## Open Questions

- What is the approved additive-evolution/versioning mechanism for an existing
  built-in Bifrost table and its canonical audit hash? The current catalog
  explicitly rejects a changed fingerprint, and the reviewed authorities do
  not authorize a migration escape hatch.
- Is `WyrdPostgres` intended to be an allowed dependency-owning field for a
  server workflow that must open tenant transactions dynamically? If yes,
  `architecture/agent-rules.md:6` and the R2 acceptance criterion must be
  corrected; if no, an existing narrower acquisition owner must be named.

## Authority and boundary coverage

| Boundary | Authority and source traced | Result |
|---|---|---|
| Canonical audit staging, hash, and retained publication | `AGENTS.md` single audit path; `agent-rules.md:12-14`; REQ-037/AC-009; staging migration/query, row mapping, projection, `AuditLogTable`, Scribe built-in registration | **FAIL — DATA-R3-1, DATA-R3-2** |
| SQL capability ownership | `agent-rules.md:6-7`; R2 `FIND-admin-principals-2`; `OperatorPool`, platform query signatures, provisioning/recovery owners and callers | **FAIL — DATA-R3-3** |
| Platform same-plane audit/effect coupling | canonical transaction rule; `PlatformAuthorization`; identity, credential, status, and suspension handlers | PASS: allowed mutations use the returned `TenantConn` and commit once; denied decisions commit independently |
| Tenant-plane audit/effect coupling | REQ-037; admin issuer/binding routes; principal revoke and credential routes | PASS for inspected routes: decision append and effect share one `TenantConn` |
| Audit actor attribution | REQ-013/037; runtime principal/session/token claims; platform and tenant event builders; staging/projection fields | PASS in fresh-state source: verified kind and optional credential id reach the canonical event and projection; upgrade durability fails separately above |
| Provisioning retry and concurrency | REQ-025-027/INV-006; claim state machine, admin transaction, credential cleanup, failure/race journeys | PASS: retry reuses the principal, revokes pre-existing credentials in the same tenant transaction, and returns one new usable credential |
| Last-admin guard | R2 `FIND-admin-principals-8`; status advisory lock and `USABLE_ADMINISTRATORS_SQL`; SQL/journey proof | PASS for the required suspension boundary: only active, granted principals with a live credential or pinned identity on the current connection count, and concurrent status changes serialize |
| Tenant lifecycle admission and RLS | REQ-026/028/031, INV-007; SECURITY DEFINER admission gate, per-request resolver, TenantConn paths, lifecycle journey | PASS: tenant admission is outside the principal-epoch cache and is resolved on every request |
| Tenant provisioning/recovery cross-plane durability | REQ-018/025-027/032; provisioning/recovery services and platform/tenant transactions | PASS under the approved resumable-boundary decision; no distributed transaction is required |
| Operator root recovery | REQ-033; binary subcommand, root lookup and credential transaction | PASS for inspected data path: it reuses the fixed root, creates no principal/grant, and commits only the new verifier |
| Tenant isolation | INV-007; RLS migrations, `TenantConn` queries, operator-only platform schema | PASS for inspected paths; no tenant query was widened to the operator role |

## Prior-finding disposition

| Stable finding | Disposition in candidate `a9a7c9c1e` |
|---|---|
| `FIND-admin-principals-1` | **CLOSED for this domain.** The parallel audit table is gone and same-plane platform allowance/effect transactions are coupled. |
| `FIND-admin-principals-2` | **OPEN / REVISED.** Raw SQLx transactions no longer escape, but the explicitly included `WyrdPostgres` struct-field half remains (`DATA-R3-3`). |
| `FIND-admin-principals-8` | **CLOSED.** Usability and concurrent suspension checks match the remediation boundary. |
| `FIND-004-3` | **CLOSED.** Resumed provisioning retires abandoned credentials and proves one usable returned credential. |
| `FIND-005-1` | **CLOSED.** Tenant issuer/binding/revoke mutations now share their audit transaction. |
| `FIND-admin-principals-R2-2` | **CLOSED.** Stored platform principal kind reaches session, context, and audit. |
| `FIND-admin-principals-R2-3` | **OPEN / REVISED.** Fresh rows carry `credential_id`, but the retained-table and hash upgrade is not compatible (`DATA-R3-1`, `DATA-R3-2`). |
| `FIND-admin-principals-R2-5` | **CLOSED.** Tenant admission is read on every request outside the epoch cache. |

The owner explicitly waived all `FIND-TASK-001-10` provenance concerns and
approved inclusion of the verified-change-contract work; neither is a finding
in this report. The pre-existing Card-name constraint, stale testing comment,
base-red auth test, and ungated rustdoc lane remain outside this domain verdict.

## Verification Notes

This was a static review. Per the orchestrator's instruction, I ran no Cargo or
`mise` command. I inspected the committed R2 evidence and its focused test
claims. Those fresh-database tests can prove current-row attribution, retry,
transaction coupling, lifecycle admission, and last-admin behavior, but they do
not construct a pre-change registered `vala.system.audit_log` or pre-change
audit hash and therefore cannot detect DATA-R3-1 or DATA-R3-2. The existing
tenant-isolation source check detects raw SQLx transaction signatures but does
not reject the two retained `WyrdPostgres` fields, so it also cannot close
DATA-R3-3.

The candidate remained
`a9a7c9c1e8502ccf3befa74c4283c78e5d70c132` throughout inspection.

## Overall result

**FAIL**

Proposed stable IDs for validation: `FIND-admin-principals-R2-3` for
DATA-R3-1/DATA-R3-2 and `FIND-admin-principals-2` for DATA-R3-3.
