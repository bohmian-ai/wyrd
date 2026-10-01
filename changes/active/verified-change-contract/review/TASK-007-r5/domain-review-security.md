# Security Domain Review

## Immutable subject

- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `1e857c89f116fc7901e365bc456db6a20cbea684`
- Candidate remained `HEAD` throughout this review.
- Approved authority: `changes/active/verified-change-contract/spec.md`, revision 36, explicitly approved 2026-09-24.

## Reviewed boundary

The review traced the cumulative base-to-candidate security boundary for:

- Operator-connection HTTP and MCP CRUD authorization, tenant derivation, transactional audit, redacted responses, and error projection;
- forced-RLS persistence, ciphertext-only rows, cross-tenant key-version discovery, and leased dispatch access;
- AES-256-GCM envelope encryption, canonical AAD, DEK wrapping, per-attempt opening, rewrap fencing, and selector-free key failures;
- environment, owner-only file, and Vault KV v2 key sources, production configuration validation, active-tenant readiness, and role ownership;
- registration and delivery connection-authority checks, forbidden headers, secret lifetime, provider payloads, and durable error/status data;
- effective-URL rendering, DNS resolution, address screening, connection pinning, redirect handling, and credential-header ordering.

## Authority and source coverage

| Boundary | Authority | Source traced | Result |
|---|---|---|---|
| Identity, RBAC, tenancy, and audit | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` Authorization, Tenant isolation, Audit integrity; REQ-139, REQ-145, REQ-148; AC-030/031 | `components/operators/{routes,service}.rs`, `audit/mod.rs`, `wyrd-runtime/src/permission.rs`, `mcp/operators.rs`, connection SQL and migration | PASS |
| Encrypted connection persistence | Security posture Cryptography/secret handling; REQ-147, REQ-150; TASK-007 scenarios 1-2 | `wyrd-crypt/src/lib.rs`, `components/operators/keys.rs`, `wyrd-spec/src/operator_connection.rs`, `wyrd-sql/.../operator_connections.rs`, migration 32 | PASS |
| Deployment KEK source/readiness | Security posture Production composition and Cryptography/secret handling; approved revision-36 REQ-147; TASK-007 scenario 3; R4 approved decision | `config.rs:1776-2018`, `boot/mod.rs:1442-1474`, `components/operators/keys.rs:252-427` | **FAIL** (`SEC-TASK-007-1`) |
| Registration and attempt-time authority | REQ-149/150; TASK-007 scenario 5 | `card/operator.rs:64-222`, `components/cards/resolve.rs:112-148`, `verification/operators.rs:250-384` | PASS |
| SSRF and credential attachment | Security posture Source credentials and SSRF defense; agent rules external-network rule; REQ-149, REQ-142; TASK-007 scenario 7 | `wyrd-auth-oidc/src/screening.rs:94-217`, `verification/operators.rs:636-857` | PASS |
| Secret/redaction behavior | Security posture secret handling; REQ-138/139/147/148/150 | request and secret contract types, body decoders, key errors/logging, SQL row views, provider adapters, CLI body-file projection | PASS |

## Prior-finding closure

Source inspection confirms the security portions of the earlier remediation are present: selector-free key errors and logs (`FIND-TASK-007-1`), active-tenant Vault readiness (`-2`), screen/pin before credential header construction on every HTTP hop (`-3`), a bounded non-redirecting Vault client (`-5`), blocking-pool mounted-secret reads (`-7`), pass/tenant rewrap deadlines (`-9`), typed exact key-version refusal without unwind (`-10`), redacted key-owner debug (`-11`), and provider-local wire construction (`-12`). Revision 36 closes the former missing key-source decision (`-16`) as authority, but that newly approved authority exposes the production-environment-source gap below.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- `SEC-TASK-007-1` — **VIOLATION** — [`crates/wyrd/wyrd-server/src/config.rs:1914`](../../../../../crates/wyrd/wyrd-server/src/config.rs) and [`crates/wyrd/wyrd-server/src/config.rs:1921`](../../../../../crates/wyrd/wyrd-server/src/config.rs): revision-36 REQ-147 says environment-sourced Operator KEKs are development-only, but `OperatorKeysConfig::validate` rejects `Env` only for *multi-tenant* production and returns `Ok(())` for `Env` in an explicitly single-tenant production deployment. `verify_operator_keys` also intentionally skips single-tenant production (`boot/mod.rs:1457-1461`), so this is a reachable accepted boot configuration, not dormant code. A production deployment with `auth.tenant_slug` can therefore create and open Operator credentials under a process-environment KEK, exposing the KEK to environment capture paths the approved file/Vault boundary was meant to exclude (for example, inherited process environments, support dumps, or host process inspection). **Required correction:** make production validation reject `OperatorKeySource::Env` regardless of tenancy; retain owner-only file as the explicitly single-tenant production option and Vault-over-HTTPS as the multi-tenant requirement. Add a focused configuration test proving production + single tenant + env is refused, development + env remains accepted, production + single tenant + file is accepted, and production + multi tenant still requires Vault.

### Low / Defense In Depth

- None material to task acceptance.

### Positive Controls

- Every connection-management permission decision is audited before proceeding; writes append the allowed row in the operation transaction and record evaluated decisions standalone when that transaction cannot commit.
- Tenant identity comes from `Caller`, all tenant CRUD/delivery reads use `TenantConn`, the connection table enables and forces RLS, and the privileged key-version query is limited to the two nonsecret columns it needs.
- Postgres stores ciphertext, nonces, wrapped DEKs, and versions only. Both secret and wrapped-DEK layers authenticate length-prefixed tenant/connection/provider/name/secret-version context under separate domain tags.
- Key failures and key-owner `Debug` values omit selectors, provider response text, paths, addresses, tokens, tenant IDs, and raw key material.
- Delivery re-reads the latest connection each attempt, rechecks active provider/origin/auth/header authority before decrypting, and does not cache plaintext credentials between attempts.
- HTTP delivery renders only nonsecret request state first, resolves and rejects every forbidden address, pins the approved resolution, disables automatic redirects, repeats screening on same-origin redirects, and attaches sensitive credential headers only to the screened client's request builder.

## Verification limits

- This was a read-only cumulative static audit. No test lane was rerun by this reviewer; the task packet records prior green focused and broad lanes, but those records were treated as supporting evidence rather than source truth.
- Credentialed Slack/PagerDuty smoke evidence remains gated and was not available. Local mock-provider coverage establishes request construction and classification, not vendor-side acceptance.
- No live production Vault, production filesystem mount, or hostile DNS infrastructure was exercised. Vault TLS/redirect policy, file-mode behavior, and DNS pinning were inspected in source and against local tests only.

## Overall result

**FAIL** — `SEC-TASK-007-1` is a bounded, reachable configuration-policy violation of the user-approved revision-36 Operator key-source contract.
