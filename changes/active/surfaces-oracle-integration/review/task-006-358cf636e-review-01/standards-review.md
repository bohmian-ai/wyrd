# TASK-006 repository-standards review

## Immutable subject

- Base: `32a0aafecbda96a86103cd389e42728ea33e0c54`
- Candidate: `358cf636eda48ea31a3416e2c5b21b30873f83dd`
- Result: **FAIL**

## Authority coverage

| Changed surface | Applicable authority | Result |
|---|---|---|
| Audit staging schema and SQL | SQL/RLS/audit rules; Bifrost/security; Rust/OLAP/reliability references | FAIL |
| Gate removal and projection identity | Bifrost ingest/dedup/audit; Rust visibility and ownership rules | FAIL |
| Server AuditPublisher and role composition | Server, tenancy, async, ownership, and documentation rules | FAIL |
| SQL/server/Scribe journeys | Test placement and journey rules | PASS for placement; FAIL for required proof |
| Contract comments and generated schema | Contract/generated-artifact rules | PASS with later recorded codegen evidence |
| Architecture, operations, and docs | Authority hierarchy; Bifrost/security/recovery; docs verification | FAIL |
| Python/TypeScript | No behavioral public SDK change | N/A |

## Material standards findings

### STD-006-1 — TenantConn queries duplicate RLS predicates

`audit_staging.rs:232-248,265-268,296-298,336,345-351` adds manual tenant equality to new `TenantConn` queries, contrary to the load-bearing RLS rule. Remove the duplicate predicates and retain RLS-backed tests.

### STD-006-2 — Materially modified publisher retains raw `PgPool`

`audit/publication.rs:59-74` stores `PgPool` and acquires directly from it. Compose the existing Postgres owner/capability instead.

### STD-006-3 — Signature/import style violations

`publication.rs:240` and `source_boundary_recovery.rs:220` use fully-qualified types in signatures. Import and use bare names.

### STD-006-4 — Required rustdoc is missing

New fallible `AuditPublisher::freeze` and `read_range` lack `# Errors`; materially changed panic-capable helper/tests lack `# Panics`; the publisher module lost its module rustdoc. Document actual error, panic, cancellation, and partial-progress behavior.

### STD-006-5 — Internal durable stages became public API

Publicly exported `AuditPublisher::publish_range` and `settle` expose partial durable workflow stages; `settle` accepts an arbitrary upper sequence and can delete staging outside freeze→publish→settle. Keep them internal and use sanctioned test-only fault support.

### STD-006-6 — Documentation contradicts governing authority

Touched recovery/reference/docs surfaces still describe watermark-only replay or Scribe/Forge transition audit, including `operations/reliability-and-recovery.md:175-187`, `references/languages/rust-core.md:601-605`, and Bifrost architecture/Forge docs. Align them with frozen-bound publication and authorization-only audit.

### STD-006-7 — Required docs verification is absent

Three `.svx` files changed, but no `mise run docs:check` result is recorded.

## Passing rule areas

Durable behavior remains in Rust owners; direct local Scribe composition deletes the Gate path; async additions await IO; callers own `TenantConn` commit; no new trait/service/dependency/feature/test binary/harness was added; production unwrap audit passed; and generated schema source matches its snapshots.

## Verification limits

The specialist did not rerun Cargo/Postgres/journey/codegen/docs commands. Later evidence reports format, lints, codegen, SQL 220/220, Scribe 21/21, server 7/7, and boundary checks. `docs:check` remains absent.
