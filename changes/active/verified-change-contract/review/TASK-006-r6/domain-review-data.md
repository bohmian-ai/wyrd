# Data-domain review — TASK-006 R6

**Immutable subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f..3f93886489a1d95be1a3eb2382fe059bb9856988` (candidate HEAD matched). **Result: PASS.**

## Boundary and authority coverage

| Boundary | Source and verification inspected | Governing authority |
|---|---|---|
| One-off owner migration, lease, and recovery | `wyrd-server/src/main.rs:185-224`; `wyrd-sql/src/lib.rs:58-122,183-234`; `vala-sql/src/lib.rs:35-64`; `wyrd-sql/tests/pg_migration.rs::migration_lease_serializes_and_bounds_competing_migrators`; R5 evidence | Approved spec rev 44 REQ-154/157/158, AC-035; R5 FIND-20; `architecture/agent-rules.md` pool boundary; `AGENTS.md` §§5, 9, 11 |
| Serving role and schema isolation | `wyrd-sql/src/dsn.rs`; `wyrd-sql/src/postgres.rs:63-113`; `wyrd-sql/src/schema_check.rs`; `vala-sql/src/postgres.rs`; role bootstrap/migration SQL; R5 SQL and startup evidence | REQ-156/157/158, AC-035; `architecture/wyrd-design.md`; `architecture/operations/deployment-and-release.md`; `architecture/agent-rules.md` RLS boundary |
| Iceberg catalog TLS and external database identity | `vala-bifrost-redux/Cargo.toml:72-75`; `catalog/iceberg_sql.rs:15-100`; `docs/src/content/docs/self-hosting/kubernetes-production.svx:75-125,185-220`; recorded guide walk | REQ-154/156, AC-035; R5 FIND-28; `architecture/operations/deployment-and-release.md` |
| Eval durable state in cumulative candidate | `wyrd-sql/src/queries/verifier_runs.rs`, verifier migrations and `pg_verifier_runs.rs`; prior R5 data report and current diff | Original TASK-006; spec REQ-077/083/084/085, AC-014/016; `architecture/bifrost-design.md` |

## Assessment

No material data-domain finding. `SqlStore` is removed. The one-off `migrate` command builds an `OperatorPool` from the owner DSN, holds its detached session lock across ordered Wyrd and Vala migrations plus both schema validations, and explicitly releases and closes it. Serving builds separate app and platform-admin pools and validates their actual logins, migration checksums, privileges, and tenant policies before readiness; the owner DSN is absent from that construction. The exact competing-migrator test and recorded `test:sql` and startup lanes support the lease and role boundary.

The Iceberg catalog's separate SQLx 0.8 dependency now has a Rustls backend. Its focused test proves a TLS-required connection reaches the PostgreSQL SSL negotiation and rejects a server that declines TLS. The production guide supplies `sslmode=verify-full` and a mounted CA for owner, app, and platform-admin connections; the recorded guide walk reports successful valid connections and rejection of wrong CA and hostname. No R5 change alters Eval persistence or its RLS boundary.

## Verification limits

I reviewed source, the complete cumulative diff, R5 delta, and recorded evidence; I did not rerun database or image lanes. The catalog unit test proves TLS negotiation, while the guide walk is the evidence for certificate and hostname verification against a live database. No material proposed finding.
