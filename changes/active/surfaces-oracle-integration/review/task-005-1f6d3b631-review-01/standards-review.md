# TASK-005 repository-standards review

## Immutable subject

- Base: `0af5eef72f83a95167f6ee3bdc873df9f9cc4254`
- Candidate: `1f6d3b6316bfd87c8a52e36b623b80d608304e19`
- Result: **FAIL**

## Authority coverage

| Changed surface | Applicable authority | Result |
|---|---|---|
| Audit contracts, schema, OpenAPI | Repository contract/error/generated rules; Wyrd design/doctrine; agent/error/testing references | PASS |
| Tenant SQL, hash chain, staging, watermark | SQL/RLS/audit rules; Bifrost/security authorities; Rust, Vala, OLAP, reliability references | FAIL |
| Gate/server authz, publisher, Oracle relay | Server/audit/tenancy/Rust rules; Wyrd/Bifrost/security authorities | FAIL |
| Scribe/Forge/Oracle lineage | Audit-versus-lineage and Bifrost rules | FAIL |
| Operations/reference docs | Authority router and operations/security authorities | FAIL |
| Tests and tooling | `AGENTS.md` §§11–12; testing workflows | FAIL: placement passes; proof is incomplete |

## Material standards findings

### STD-005-1 — Raw `PgConnection` signature at the tenant boundary

`crates/vala/vala-sql/src/queries/audit_staging.rs:34-37` retains materially modified `append_audit_connection(conn: &mut PgConnection, ...)`. Only `TenantConn` and `OperatorPool` are allowed in library signatures. The deleted operator writer no longer needs this seam; keep the canonical writer on `TenantConn`.

### STD-005-2 — Manual tenant predicates duplicate RLS

Materially modified staging list/drain queries add or retain `data_tenant_id = wyrd.current_tenant()` on `TenantConn` paths. Agent rules prohibit parallel tenant filtering. Rely on RLS and preserve functional/index verification.

### STD-005-3 — Unearned single-production-implementation trait

`vala-bifrost-redux/src/gate/mod.rs:269-278,298,411` adds runtime `Arc<dyn GateAudit>` although only one production implementation exists; the other consumer is a test double. Repository abstraction rules reserve traits/dynamic dispatch for multiple real implementations. Use a concrete or static composition shape that preserves server-owned audit and testability.

### STD-005-4 — Fully-qualified type in a public signature

The new Gate trait uses `wyrd_spec::vala::api::AuditOutcome` in its signature instead of a top-level import and bare name.

### STD-005-5 — Stale outbox/plan vocabulary and contradictory operations guidance

Materially changed runtime and operations prose still describes `audit_outbox`, deleted fields, claimed-range retirement, and ephemeral C2/S3.C5 labels. This contradicts the new staging/watermark authority and required Wyrd-native documentation. Align runtime rustdoc, operations, and routed references with actual staging and lineage behavior.

### STD-005-6 — Dead tenant-isolation exemption

`scripts/check_tenant_isolation.py:83-85` retains an allowlist for deleted `queries/audit_outbox.rs` and `OperatorAudit`. Repository check-retirement rules require deletion of unreachable exemptions.

## Passing rule areas

Durable behavior remains in its Rust owners; `wyrd-spec` remains foundational; authorization-only audit replaces engine-mechanic events in the primary changed paths; Oracle remains WAL-first; generated artifacts were updated; async changes await IO; no dependency/feature/profile or new test file was introduced; and recorded formatting/lint/boundary lanes passed.

## Verification limits

The specialist did not run the candidate's Cargo/mise lanes. Recorded evidence covers format, lints, codegen, SQL, Redux/server, and several journeys, but not `verify:bifrost`, `check:clippy-allow-audit`, or `check:from-pools-allowlist`.
