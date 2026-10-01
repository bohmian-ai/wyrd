# Domain review: tenancy, persistent data, and concurrency

## Boundary and authority

- Immutable base `3fc085acf5b3a710d5dc80892bd2e664b3db6174`; candidate `2d57a6605da9bc1cee61140d7c09742cc4636efa`; latest fix starts at `bae424cc647dad4be80e7debec976d0b7b3e4cf8`.
- Authority: approved `spec.md` revision 5, original `TASK-002-tenant-login.md`, R11 remediation and verdict, `AGENTS.md`, `architecture/agent-rules.md`, and `architecture/wyrd-security-posture.md`.
- Reviewed the cumulative candidate for tenant SQL capabilities, canonical audit and transaction boundaries, slug resolution, and refresh-family serialization; examined the latest fix and all production callers of both standalone audit forms. `.codegraph/` is absent, so caller tracing used source search. No other reviewer's conclusions were used.

## Source and verification coverage

| Boundary | Source evidence | Result |
|---|---|---|
| Keyless activation audit and mutation | `HumanConnections::activate` now calls `begin_locked` before `require_keyring`; `begin_locked` opens a `TenantConn`, takes the connection-slot lock and appends the caller decision. `commit_refusal` commits that decision on the missing-key path; no candidate or recovery-key read or promotion occurs. A failed append rolls back. The new focused test checks one redacted staged row and no Active connection. | PASS |
| Standalone audit and tenant isolation | `record_audit(&ValaPostgres, tenant, event)` acquires through `ValaPostgres::tenant_conn`, calls the existing `append_audit`, then commits. `TenantConn::acquire` transaction-locally binds `app.current_tenant`; `append_audit` serializes the tenant chain head and writes only `vala.audit_staging`. No new sink or transaction owner was introduced. | PASS |
| Owned and nonblocking audit | `record_audit_owned(ValaPostgres, ...)` runs the same `record_audit` on a spawned, joined task, preserving its original `Send` boundary and error return. Gateway invocation still uses its tracked task and logs/counts a failed append without blocking the invocation. | PASS |
| Callers and SQL capability | All live `record_audit` / `record_audit_owned` call sites now pass `state.postgres.vala()` or a cloned `ValaPostgres`. The amended existing tenant-isolation check scans the server audit module for `PgPool` and raw-pool accessors. The implementation record reports a successful negative injection check and green `check:tenant-isolation` and `check:from-pools-allowlist`. | PASS |
| Prior tenancy and concurrency closure | The cumulative source retains the single `WyrdPostgres::resolve_tenant_slug` operator lookup and tenant-scoped login queries. Refresh, first issuance, login callback, and admin revocation retain the refresh-family lock before User authority reads/writes. R11 edits do not alter these paths. | PASS |

The implementation record reports the focused keyless and rotation tests, identity journey 27/27, principals integration, gateway native, Bifrost server journey 16/16, format and lints all green. This review inspected source and the recorded results; it did not rerun those suites. The unchanged, uncalled eval resolver and `ServerPostgres::vala_pool()` remain outside this remediation's live audit boundary and are not approved exceptions to the repository's raw-pool rule.

## Findings and result

No material tenancy, persistent-data, or concurrency finding in the reviewed R11 correction. **PASS.**
