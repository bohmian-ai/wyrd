# System resilience review — TASK-001 R3

## Subject and topology

- Root: `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-principals`.
- Base: `c46afdcac`; candidate: `437205debc628538ba6aa4ec828601c7c40145b4`.
- Authority: approved `changes/active/verification-closeout/spec.md` revision 2, original TASK-001, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`, and `architecture/operations/deployment-and-release.md`.
- The one-off `wyrd-server migrate` process owns DDL under the migration lease. Serving replicas verify the applied versions and checksums before readiness. In the default `all` target, public API, Gate, and local Scribe share a process; in peer targets Gate routes to its Scribe peer. The new principal API and token issuance read tenant Postgres, and Gate's new unbound attribution resolves Cards through that same tenant registry before handing a frame to Scribe.

## Deployed and failure paths

| Path | Failure and affected capability | Recovery and proof assessment |
|---|---|---|
| New principal discovery and direct Role writes: HTTP principal router → authorization and audit outbox → tenant-scoped directory and assignment SQL → transaction commit → next token reads effective Roles | On a Postgres connection or query failure, the request returns an internal error; no partial Role write is committed. A cancelled request drops its transaction. Other process capabilities stay live subject to their own Postgres dependence. Existing issued access tokens retain their bounded lifetime. | Retry after Postgres recovers; idempotent `PUT` and `DELETE` converge. The principal integration and three SDK journeys exercise healthy, duplicate, denial, and IdP coexistence paths. They do not establish migration from an already-applied schema. |
| Unbound native Bifrost and OTLP: authenticated Gate request → record-write authorization → `CardRegistry::attributed_cards` tenant lookup → Scribe admission | A registry outage returns `IngestError::Internal` before Scribe accepts the frame. This fails the affected unbound write request closed; Card-bound writers use signed scope without the new lookup. The request does not crash Gate or the shared server. Pool acquisition has a configured timeout. | Retry the original write when the registry returns. No accepted WAL state exists for a lookup refusal. The OTLP and observe journeys prove normal attribution and invalid Card refusal; they do not simulate a registry outage, but the direct error propagation is evident in `gate/attribution.rs` and Gate call sites. |
| New release against an already-migrated Postgres database: one-off migrator → migration ledger checksum validation → replica readiness | The candidate changes `20260601000001_auth.sql` in place and deletes `20261002000100_workload_role.sql`. The applied ledger retains the old checksum for the first migration, so the migrator refuses the release before applying new schema. If bypassed, replica `SchemaCheck::verify_migrations` also rejects the checksum, and the new assignment SQL needs a `source` column absent from that database. All public-serving capabilities of replacement replicas remain unready, not just principal administration. Existing replicas may continue while present, but rolling replacement cannot finish. | Requires an immutable forward migration that adds/backfills `source`, preserves user assignments, replaces the primary key, and reconciles existing tenant built-in Roles and Card-principal grants. Rehearse migration from the base schema and prove checksum/readiness and role/token behavior after replacement. Fresh-database tests cannot prove this path. |

## Material proposed finding

### SYS-001 — Applied migration rewrite blocks release and recovery

- **Classification:** REGRESSION / VIOLATION.
- **Obligation:** The task requires direct and IdP assignments to coexist and four built-in Roles for deployed tenants. `architecture/operations/deployment-and-release.md` requires immutable ordered migrations, checksum verification, and a release path that permits rolling replacement.
- **Exact locations:** `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql` changes the definition and primary key of `auth_user_roles`; `crates/wyrd/wyrd-sql/migrations/20261002000100_workload_role.sql` is deleted; `crates/wyrd/wyrd-sql/src/schema_check.rs:63-99` enforces applied checksums. `crates/wyrd/wyrd-auth/src/seed.rs:22-47` inserts built-in Roles only when absent, so it cannot reconcile already-seeded role permissions or retire old Role names by itself.
- **Observable consequence:** A normal deployment with base migrations applied cannot start the candidate migrator or make candidate replicas ready. Repair by merely accepting the old checksum would leave `auth_user_roles.source` missing and existing tenant role state stale. Rollback/restart does not create the missing schema.
- **Testable correction:** Restore historical migration files byte-for-byte and add a forward migration for provenance and existing tenant Role/grant reconciliation. Exercise the one-off migrator and replica readiness against a database migrated by the base commit, then verify pre-existing direct/IdP-equivalent grants, updated built-in permissions, next-token behavior, and no retired alias. Keep the migration owner and fail-closed readiness boundaries intact.

## Result

**FAIL** — SYS-001 blocks deployed upgrade. The reported verification is strong for fresh-schema paths but provides no base-schema upgrade or rolling-replacement proof.
