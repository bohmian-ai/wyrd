# TASK-005 security and tenancy domain review

**Subject:** `05d7d741304af3b0b4e667e7e18f93dec16b897b..885d16c11ecc7a3eda73b5f1b27dd40c0a2cece2` (excludes `1f1cbcf5f`). **Result: PASS.**

## Boundary and authority

Reviewed the changed Gate and OTLP transport error paths, Oracle query admission/stream instrumentation, and shared metadata-cache/request telemetry against `AGENTS.md` §§2, 9, 10–12; `architecture/agent-rules.md` (tenant SQL, audit, typed security boundaries); `architecture/wyrd-design.md` (Bifrost and tenant authority); `architecture/wyrd-security-posture.md` (credential-derived tenancy, authorization and audit); `architecture/bifrost-design.md` (Gate, Oracle and storage owners); `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/domain/telemetry-observations.md`; the approved spec's INV-002, INV-006 and INV-009; and the original TASK-005. The changed source is read from the immutable range above. `.codegraph/` is absent.

## Source and path coverage

| Boundary | Source and reachable path | Assessment |
|---|---|---|
| OTLP HTTP/gRPC identity | `wyrd-server/src/http/otlp.rs` derives `AuthContext` from verified `Caller`; `grpc/otlp.rs` authenticates metadata before `Grpc::unary` and decode. The diff adds `report_internal_at_edge` only at each error projection. | Token-derived tenant and pre-decode authentication remain intact. |
| Gate writes and audit | `gate/mod.rs` `ingest_decoded_resource_{spans,metrics,logs}` still calls `authorize_record_write` before projection/dispatch. That method still obtains a required audit sink, computes the table-scoped verdict via `record_write_verdict`, appends the allowed/denied decision, and then returns the verdict. `dispatch_native_frame` still authorizes before Scribe admission. The diff removes only obsolete event/row counters and moves internal-failure logging. | No authorization bypass or audit weakening. Missing audit still refuses the write. |
| Oracle query and admission | `gate/mod.rs::query_sql` still passes `AuthorizedQueryContext` to `dispatch_sql`; `oracle/mod.rs::AuthorizedQueryContext::try_new` still rejects principal/tenant mismatch. `oracle/planner.rs` still calls `authorize_resolved_tables` before reader guard/revalidation. `oracle/mod.rs::run_sql_attempt` still admits before `audit_and_bind` and source execution; the new `telemetry.admitted()` follows successful admission. `oracle/query_stream.rs` changes span polling and terminal labels, not query authority. | Query authorization, tenant binding, and admission ordering remain intact. |
| Cache and physical reads | `storage/cache.rs::ObjectMetadataKey` still hashes tenant, table, object and immutable pin. `register` still keys resident and in-flight lookup on the full key. The diff replaces the telemetry ledger with direct cache/request metrics and test-only inspection. `oracle/exec.rs` retains the authenticated context and footer tenant proof; it removes a duplicate pruning pass. | No cross-tenant cache collision or removed footer check found. |
| Public proof | `wyrd-testing/tests/bifrost/oracle/published.rs` uses two active tenants with one logical table name and asserts each public query returns only its own row. `scribe/write_read.rs` contains a second-tenant canonical-span isolation path. `oracle/capacity.rs` checks bounded labels against tenant IDs. | Existing negative/edge paths remain present; the published journey is listed as passing in task evidence. |

## Findings

None proposed for this domain. The changed error reporting logs only the existing `IngestError::Internal` detail at the responding edge; status/error projection remains Gate-owned. The change does not add credential, payload, tenant, table, or request labels to metrics.

## Verification limits

This is a source audit, not an independent rerun of the Postgres journeys. The task records passing exact Scenario 1–4 commands, module tests, the full Scribe write/read journey, format and lints. Its standard benchmark, broad gate and whole journey lanes were not run, so this report does not assert their results. Those limits do not expose a distinct security or tenancy defect in the changed range.
