# TASK-005 R1 security and tenancy domain review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..1fc68f3b78c4dbf82a8f1c518bbc40343c484d65` (cumulative candidate). The subject's HEAD remained at the candidate. `.codegraph/` is absent.

**Boundary reviewed:** tenant-bound SQL transaction acquisition, Gate authentication/write authorization and query dispatch, Oracle admission, and shared metadata-cache tenant identity. This is a security/tenancy review of the cumulative diff, including R1, not an assessment of benchmark capacity or all telemetry semantics.

## Authority and source coverage

| Boundary | Authority | Source and caller path inspected | Result |
|---|---|---|---|
| SQL transaction tenancy and rollback | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` (tenant-scoped SQL through `TenantConn`, RLS, fail closed); `architecture/wyrd-design.md`; approved spec REQ-002/003, AC-017 | `wyrd-sql/src/tenant_conn.rs` entire owner; `postgres.rs::tenant_conn`; SQL tenant query callers and `pg_verification_bindings` isolation test | PASS |
| Gate auth, permission and audit before write | Same security authority; `architecture/bifrost-design.md` Gate and query boundaries; TASK-005 non-goal of unchanged results/security | `gate/mod.rs::authenticate`, `authorize_record_write`, three OTLP paths, native frame, `query_sql`; `gate/error.rs` and HTTP/gRPC error projection | PASS |
| Oracle admission tenant binding | Bifrost design admission/tenant fairness; security posture object decision before admission | `oracle/admission.rs::admit_with_attempt_id` and `OracleTelemetry` changes; Gate query dispatch | PASS |
| Metadata cache isolation | Bifrost design physical tenant identity; security posture tenant-qualified storage; `architecture/references/domain/olap-serving.md` | `storage/cache.rs::ObjectMetadataKey`, `register`, `retain`; `storage/mod.rs` request path | PASS |
| Diagnostic data in telemetry | Security posture prohibition on secrets in logs/traces; `architecture/references/domain/telemetry-observations.md` | Gate failure logging and span fields, Oracle telemetry labels, cache metric labels | PASS |

The cumulative diff makes only one SQL boundary change: `Box::pin(pool.begin_with(AssertSqlSafe(statement)))` at `tenant_conn.rs:116`. It boxes the acquire future without changing the SQL statement, tenant UUID source, SQLx transaction value, error mapping, or drop/commit path. `TenantConn::acquire` still binds the typed `DataTenantId` in the begin statement and returns the same transaction; `TenantConn::commit` and SQLx rollback-on-drop remain unchanged. Its caller, `Postgres::tenant_conn`, still selects the app pool. The existing Postgres test at `tenant_conn.rs:180-209` checks failed begin recovery and transaction-local binding; this review did not rerun it.

Gate OTLP exports still call `authorize_record_write` before projection and Scribe dispatch (`gate/mod.rs:788-887`). That owner still appends the allow/deny audit decision before returning permission (`:487-507`). Native ingest still validates the batch and calls the same owner before Scribe (`:994-1024`). The removed code only recorded Gate telemetry on rejection. Query dispatch still requires an already authorized context and delegates to the same dispatcher; the new span parent at `:750-758` changes trace shape, not permission or admission. Oracle's changed waiter call only closes queue telemetry after `wait_for_grant` returns; the tenant passed to `build_admitted_guard` is unchanged (`oracle/admission.rs:816-837`).

Metadata cache keys still include typed tenant, logical table, object identity and immutable pin (`storage/cache.rs:170-280`). The changed cache branches replace a separate telemetry facade with direct metric publication; the resident and in-flight maps, lookup key, cancellation, and retention decisions remain the same (`:570-626`, `:690-799`). The new cache inspection is test-support-only and reads owner state. Added metric labels are closed outcomes/reasons, not tenant IDs or object paths. Gate's new native span field is a parsed batch UUID, and its failure event logs the existing status message; no bearer material or new tenant authority appears in the diff.

## Verification limits

Available candidate evidence reports targeted SQL tests and all Bifrost journeys passing. I inspected their source and the cumulative diff; I did not run a new security test. The benchmark's one missed latency target and the user's deferred repository gate are outside this domain result. No security conclusion depends on treating them as passing.

## Proposed findings

None. No changed path was found that accepts client-supplied tenant authority, skips the existing permission/audit decision, changes transaction rollback/tenant scope, or shares cached metadata across tenants.

**Overall: PASS.**
