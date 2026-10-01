# TASK-007-R2 Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation task: `changes/active/verified-change-contract/review/TASK-007-r1/TASK-007-R1-operator-delivery-corrections.md`

The candidate remained `HEAD` throughout this review. The complete cumulative
base-to-candidate diff was inspected; reviewed source was not modified.

## Reviewed boundary

This review traced the security-sensitive path from typed connection input
through HTTP/MCP authorization and canonical audit, forced-RLS persistence,
envelope encryption and rewrap, deployment KEK/Vault configuration and boot
readiness, per-attempt credential resolution and authority matching, outbound
resolve-screen-pin behavior and redirects, and public error/log redaction. It
also reassessed every prior security-domain finding against the cumulative
candidate.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| RBAC and transactional audit | `REQ-148`, `INV-007`, `AC-030`; `AGENTS.md` and `architecture/agent-rules.md` canonical-audit rules | `wyrd-runtime/src/permission.rs`; `components/operators/{routes,service}.rs`; `mcp/operators.rs`; `audit/mod.rs`; `pg_operator_connection_routes.rs` | PASS: reads require `operators:read`; mutations require `operators:write`; denied decisions use the canonical standalone path; allowed writes append before mutation in the same tenant transaction; read decisions are recorded; MCP writes remain scope-gated |
| Tenant isolation and persistence | `REQ-139`, `REQ-147`, `INV-007`, `AC-031`; `TenantConn`/forced-RLS rules | migration `20260601000032_operator_connections.sql`; `wyrd-sql/src/queries/operator_connections.rs`; `pg_operator_connections.rs`; management and delivery callers | PASS: tenant paths use `TenantConn`, the table enables and forces RLS, foreign IDs are indistinguishable from missing, cross-tenant key-version discovery is the narrow `OperatorPool` exception, and stored secret material is ciphertext/wrapped-DEK only |
| Envelope encryption and authority binding | `REQ-139`, `REQ-147`, Scenario 3 | `wyrd-crypt/src/lib.rs`; `components/operators/keys.rs`; `operator_connection.rs`; SQL row mapping and rotation tests | PASS: fresh DEK/nonce, distinct domains, canonical tenant/connection/provider/name/secret-version AAD, authenticated open, secret-version fencing, and per-pass-only old-key reuse are present; plaintext is zeroizing and is not returned |
| Production key readiness and Vault boundary | `REQ-147`, Scenario 3; `architecture/wyrd-design.md:1383-1396`; security-posture secret-provider rules | `config.rs:1830-1930`; `boot/mod.rs:1336-1343,1445-1478`; `keys.rs:240-388,571-593`; readiness/config/key tests | PASS: multi-tenant production requires Vault over HTTPS; the bounded no-redirect client must build; every active tenant's active key is read and decoded before readiness; Vault token files reuse the owner-only reader; development and explicit single-tenancy retain the approved allowance |
| Secret redaction and diagnostics | `REQ-139`, `REQ-148`, `REQ-150`, Scenario 4, `AC-031` | `operator_connection.rs`; `routes.rs::decode_body`; `keys.rs::{KeyError,KeyFailure,log}` and public conversion; management journeys | PASS: write secrets use redacted wrappers, read views contain only nonsecret authority, parse failures omit values, and key failures retain only source kind/version/stable class internally while public detail is constant and captured logs exclude selector, tenant, provider response, and secret sentinels |
| Registration and per-attempt credential authority | `REQ-149`, `AC-029`, `AC-031` | `components/cards/resolve.rs::check_operator`; `verification/operators.rs::{attempt,credential}`; registration and rotation journeys | PASS: tenant/provider/name/status plus HTTP origin/auth/header authority are checked before decryption at registration and again immediately before every attempt; missing, disabled, wrong-provider, wrong-authority, and wrong-tenant states fail closed |
| HTTP SSRF and credential sequencing | `REQ-142`, `REQ-149`, Scenario 7; `architecture/agent-rules.md` resolve-screen-pin rule | `wyrd-auth-oidc/src/screening.rs`; `verification/operators.rs:671-893`; focused unit tests and delivery journey | PASS: all DNS answers are screened, clients are pinned to the screened set, metadata/link-local is always blocked, production also blocks internal ranges, redirects are manual and same-origin, every permitted hop is re-screened, and credential headers are constructed only after that hop's screened client exists |
| Provider secret transport | `REQ-140`, `REQ-141`, `REQ-142` | `verification/operators.rs::{slack,pager_duty,post_json}`; delivery tests | PASS: Slack bearer headers are sensitive and attached after screening; PagerDuty's key is attached as provider-required JSON only to a screened request; provider responses and durable failures do not echo credential bytes |

## Prior-finding closure

| Finding | Closure evidence | Status |
|---|---|---|
| `FIND-TASK-007-1` | `KeyError` no longer stores selectors or raw causes; `From<KeyError>` returns constant detail and `KeyError::log` emits only bounded fields. `key_failures_disclose_no_selector` covers env, file, inline/file-token Vault, provider response, tenant, and secret sentinels in problems and captured logs. | CLOSED |
| `FIND-TASK-007-2` | `build_state` calls `verify_operator_keys` before readiness; multi-tenant production enumerates active tenants and calls `OperatorKeys::verify_active`. The architecture fail-start statement is restored. `production_boot_requires_every_active_tenant_key` covers transport, missing, malformed, wrong-length, and valid keys plus development/single-tenant allowances. | CLOSED |
| `FIND-TASK-007-3` | `HttpRequest` contains no credential. Each initial/redirected URL first passes `ScreenedHttp::client_for`; only then does `Credential::attach` build a sensitive header on that pinned client's request builder. The two focused HTTP tests cover blocked/unresolved starts, allowed redirects, and rejected cross-origin redirects. | CLOSED |
| `FIND-TASK-007-5` | Production configuration rejects plaintext Vault addresses; non-production local HTTP remains allowed. KEK and Vault-token files both use `read_owner_only`; loose token files fail before a request. | CLOSED |
| `FIND-TASK-007-7` | The shared owner-only reader moves metadata/read work through `tokio::task::spawn_blocking`; the FIFO test proves a stalled mount does not block unrelated executor work. | CLOSED |
| `FIND-TASK-007-9` | Rewrap now runs independently beside the claim loop with pass and per-tenant deadlines, tenant transactions, cancellation, and one old-key read per version per pass. This removes key-provider delay from the delivery claim path while preserving RLS and secret-version fencing. | CLOSED |

`FIND-TASK-007-4`, `FIND-TASK-007-6`, and `FIND-TASK-007-8` are outside this
domain review's security boundary and are left to the task and standards
reviewers.

## Material findings

None.

## Verification evidence and limits

- Independently run in this review: the five focused server unit tests for
  selector-free key failures, owner-only Vault token files, production HTTPS
  enforcement, blocked/unresolved credential ordering, and per-hop redirect
  ordering; all five passed.
- Independently run in this review: `mise run check:tenant-isolation`; passed.
- Inspected recorded candidate evidence for the Postgres-backed readiness,
  RBAC/audit/RLS, rotation, delivery, and slow-rewrap journeys. Those heavier
  Postgres lanes were not rerun in this time-bounded domain review.
- The production readiness test exercises the exact `verify_operator_keys`
  function called by `build_state`, rather than assembling all unrelated
  production TLS/peer boot material. The call site and rollback path were
  inspected directly.
- Credentialed live Slack and PagerDuty smoke remains gated release evidence
  and was not run. Local mock-provider coverage proves the reviewed protocol
  and secret-handling boundaries without live credentials.
- Owner-only mode enforcement is Unix-specific in the current source and proof;
  no non-Unix production support obligation was established by the approved
  task, so this review does not claim ACL-equivalent proof on other platforms.

## Overall result

**PASS** — no material security, RBAC, tenancy, secret-boundary, SSRF, or
diagnostic-leakage finding remains in the immutable cumulative candidate.
