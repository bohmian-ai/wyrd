# TASK-002 R12 structured Ponytail validation

## Subject and evidence

- Original base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`.
- Candidate: `2d57a6605da9bc1cee61140d7c09742cc4636efa`.
- Latest fix: `bae424cc647dad4be80e7debec976d0b7b3e4cf8..2d57a6605da9bc1cee61140d7c09742cc4636efa`.
- Authority: approved spec revision 5, original TASK-002, R11 verdict and remediation, current user SQL-capability direction, `AGENTS.md`, and `architecture/agent-rules.md`.

I read all four R12 Wave 1 reports, the cumulative and latest changed-file inventories, the R11 findings and remediation, and the relevant full source bodies and callers. The candidate stayed at the stated commit; `.codegraph/` is absent. `git diff --check` on the cumulative range passed. This is static validation of source and recorded verification, not a fresh Postgres or provider test run.

## Wave 1 proposals

All four reports return `PASS` with **no proposed findings**. I independently validated that empty union. No new finding meets this task review's reachable, changed-boundary threshold.

## Closure and minimum-mechanism checks

| Prior finding | Independent source check | Disposition |
|---|---|---|
| `FIND-TASK-002-25` | `HumanConnections::activate` enters `begin_locked` before `require_keyring`; `begin_locked` takes the existing slot lock and appends the caller's decision. The missing-key branch uses existing `commit_refusal`, before reading candidate or recovery key. Sibling candidate, stamp, deactivate and remove writers use the same transaction entry; the existing recovery-key permission decision remains separate. The changed Postgres test checks one redacted staged row and no Active connection; the recorded audit-failure journey covers failed append. | **CLOSED**. One existing audit path, lock and refusal mechanism; no extra helper or sink. |
| `FIND-TASK-002-26` | `pg_resolvers.rs` now names its real `wyrd-auth` owner and `WyrdPostgres`/`TenantConn` RLS boundary. The R11 subdiff changes only this header. | **CLOSED**. No code or documentation checker needed. |
| `FIND-TASK-002-27` | `record_audit(&ValaPostgres, ...)` obtains a `TenantConn` through `ValaPostgres::tenant_conn` and calls the existing `append_audit`; `record_audit_owned(ValaPostgres, ...)` delegates to that same function in its existing spawned `Send` boundary. All production audit callers now pass `state.postgres.vala()` or its clone; gateway remains tracked and nonblocking. The existing tenant-isolation check now scans the audit module. | **CLOSED**. No second audit writer, pool wrapper or boundary checker. |

The cumulative source retains the previously reviewed shared OIDC/JWT verifier profile, single `WyrdPostgres::resolve_tenant_slug` implementation, connection-bound renewal and refresh-family serialization; the R11 diff does not reopen `FIND-TASK-002-1` through `-24`. The server's local workload `resolve_tenant_slug` function delegates to the one `WyrdPostgres` operation and is not a second SQL resolver. The R11 fix adds no new abstraction, dependency, configuration surface or test machinery.

## Raw-pool boundary limit

The user's no-raw-pool direction remains binding; this review grants **no exception**. `ServerPostgres::app_pool()` and `vala_pool()` still expose `&PgPool` in the unchanged `wyrd-server/src/postgres.rs`. The unchanged health probe at `components/health/mod.rs:313` uses `app_pool().acquire()` in production. The unchanged `components/eval/resolver.rs:92-104` has a `&PgPool` signature, with callers only in `tests/pg_eval_v1_protocol.rs`. A repository-wide claim that raw pools are gone would therefore be false.

These sites were outside the original TASK-002 write set, the R10 human-directed SQL correction's explicit auth/boot/query boundary, and R11's live audit correction. The eval resolver has no production caller. The health probe and server handle are live but unchanged by the cumulative TASK-002 candidate and unrelated to tenant login or its audit path. Under the task-review rule against importing unrelated pre-existing debt into remediation, neither is a new `FIND-TASK-002-*`; neither is endorsed. They require a separate, scoped repository-rule correction if the user wants repo-wide enforcement. The current `check:tenant-isolation` proves its selected production boundaries, not the entire repository.

## Final validated ledger and recommendation

**Validated finding ledger: empty.** No correction to this TASK-002 candidate is recommended. The three R11 findings close through existing owners and recorded focused and broader checks. R12 result: **PASS for TASK-002 acceptance**, with the explicit repository-wide SQL limitation above. Browser redemption (TASK-003) and CLI claim (TASK-004) remain downstream and were not credited here.
