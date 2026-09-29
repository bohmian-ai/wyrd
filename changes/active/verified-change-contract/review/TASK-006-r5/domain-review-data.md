# Data-domain review — TASK-006 R5

**Immutable subject:** `f8811ac5035c3aa165d34c38992f9889b3c9081f..6ce9f9bb7e2dea288a1b78081346551d896cb7a8`. Candidate HEAD matched at inspection. **Result: PASS.**

## Boundary and authority coverage

| Boundary | Source and verification inspected | Governing authority |
|---|---|---|
| Owner migration and failure recovery | `wyrd-server/src/main.rs::migrate`; `wyrd-sql/src/lib.rs::SqlStore::migration_lease`, `MigrationLease::apply/release`; both SQL migrators; `pg_migration::migration_lease_serializes_and_bounds_competing_migrators`; recorded startup and SQL lanes | Approved spec revision 43 REQ-154/157/158, AC-035; R4 FIND-20; `architecture/operations/deployment-and-release.md`; `AGENTS.md` §§3, 9, 11; `architecture/agent-rules.md` pool ownership |
| Serving roles, grants, and RLS readiness | `ServerPostgres::connect_from_dsns`; Wyrd/Vala `validate_schema`; `OperatorPool` checks in `wyrd-sql/src/schema_check.rs`; DSN resolution, role bootstrap, `pg_migration::tenant_policy_on_another_column_fails_readiness` | Spec REQ-156/157/158, AC-035; R4 FIND-19; `architecture/wyrd-design.md`; `architecture/operations/deployment-and-release.md`; `architecture/agent-rules.md` tenant boundary |
| Eval durable enqueue and lifecycle | `wyrd-sql/src/queries/verifier_runs.rs` enqueue/claim/trace-wait/release paths; migrations 29/31/32; `pg_verifier_runs` ordinal, replay, activity, trace-deadline tests; original TASK-006 and its recorded server journey | Spec REQ-077/083/084/085, AC-014/016; original TASK-006; `architecture/references/domain/evaluation.md`; `architecture/references/domain/analytical-operations-reliability.md`; `architecture/bifrost-design.md` |

## Result

No material data-domain finding. The R4 wrong-column policy gap is closed at the shared readiness owner: `verify_tenant_isolation` compares both policy expressions to `(data_tenant_id = wyrd.current_tenant())`, while preserving forced RLS, policy-role, and permissive-policy checks. The mutation test proves both owner post-migration and serving checks reject a policy keyed on `id` and accept the restored policy. The R4 raw-pool API gap is closed by `SqlStore::migration_lease`; its detached owner session retains the database-wide advisory lock across Wyrd and Vala migrations and post-validation, with a bounded competing-migrator test.

Serving construction uses only app and platform-admin DSNs, checks both actual login identities, migration ledgers and checksums, schema grants, and tenant policies before readiness. The owner URL is read only by the one-off migration command. Observation runs remain RLS-bound and idempotent on `(tenant, binding, record)`; a per-binding row lock serializes durable sampling ordinals, and the claim path reads the stored ordinal after restart. The recorded SQL, startup, kind, and peer lanes are green.

## Verification limits

This is a source-and-evidence review; I did not rerun the Postgres or image lanes. The R4 evidence records exact focused Postgres test commands and passing `test:sql`, `test:server:startup`, `test:server:kind`, and `test:server:peer`. I found no data-domain reason to reject that evidence. The review did not audit unrelated SQL modules outside the cumulative TASK-006 paths.
