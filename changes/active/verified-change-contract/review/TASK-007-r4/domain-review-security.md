# TASK-007-R4 Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `e2da27694e8a3d057f2ff863c5d17429adc0a495`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: `TASK-007-R1-operator-delivery-corrections.md`,
  `TASK-007-R2-operator-delivery-corrections.md`, and
  `TASK-007-R3-operator-delivery-corrections.md`

The candidate was `HEAD` before and after inspection. The complete cumulative
base-to-candidate security boundary was reviewed; no reviewed source was
modified.

## Reviewed boundary

This review traced Operator connection secrets from typed HTTP, MCP, SDK, and
CLI inputs through permission evaluation, canonical audit, tenant transaction,
forced-RLS persistence, envelope encryption, deployment-key resolution and
rotation, Card registration, dispatch resolution, per-attempt decryption,
outbound DNS screening and address pinning, credential attachment, redirect
handling, and public error/log projection. It also reassessed the cumulative
closure of the prior security findings and the R3 oversized-key-version
correction.

## Authority and source coverage

| Boundary | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| RBAC and transactional audit | `REQ-145`, `REQ-148`, `INV-007`, `AC-030`; `AGENTS.md` and `architecture/agent-rules.md` audit rules | `wyrd-runtime/src/permission.rs`; `components/operators/{routes,service}.rs`; `mcp/operators.rs`; `audit/mod.rs`; route journeys | **PASS** — reads evaluate and record `operators:read`; writes evaluate `operators:write`, record denials before refusal, and append allowed decisions in the mutation transaction. Direct MCP calls re-enter the same owner and cannot bypass authorization. |
| Tenant isolation and privileged discovery | `REQ-139`, `REQ-147`, `INV-007`, `AC-031`; `TenantConn` and `OperatorPool` rules | migration `20260601000032_operator_connections.sql`; `wyrd-sql/src/queries/operator_connections.rs`; management, registration, delivery, and rewrap callers | **PASS** — tenant CRUD and delivery resolution use caller- or run-derived `TenantConn` under forced RLS. The only cross-tenant read returns tenant/key-version pairs for bounded rewrap and has column-limited SQL privileges. |
| Envelope encryption and key rotation | `REQ-139`, `REQ-147`, Scenario 3; security-posture cryptography rules | `wyrd-crypt/src/lib.rs`; `components/operators/keys.rs`; SQL row mapping and fencing; rotation proofs | **PASS** — fresh DEK/nonces, AES-256-GCM, domain-separated AAD binding tenant/connection/provider/name/secret version, exact key versions, zeroizing plaintext/key buffers, and fenced DEK-only rewrap are retained. Postgres stores ciphertext and wrapped DEKs, not plaintext credentials or KEKs. |
| Key-provider configuration and readiness | `REQ-147`, Scenario 3; security-posture production-composition and secret-file rules | `config.rs:1776-2017`; `boot/mod.rs:1335-1342,1444-1476,1489-1503`; `components/operators/keys.rs:218-650`; focused config/key tests | **PASS** — API-bearing multi-tenant production requires HTTPS Vault and verifies each active tenant's exact 32-byte active key before readiness. Vault redirects are disabled, TLS verification remains enabled, token/key files require owner-only permissions, and file reads are offloaded. Forge neither validates nor constructs the API-only owner. |
| R3 exact-version correction | `FIND-TASK-007-10`, `REQ-147` | `OperatorKeys::{for_role,new}` and `attach_config_fields`; all constructor callers; focused oversized-version tests | **PASS** — direct unvalidated construction returns selector-free `KeyError::VersionOutOfRange` instead of unwinding; API roles still fail closed; Forge retains the unused default owner and performs no provider access. `i32::MAX` remains exact. |
| Secret input, redaction, and public errors | `REQ-139`, `REQ-148`, `REQ-150`, Scenario 4, `AC-031`; security-posture diagnostic rules | provider-tagged request/view contracts; `routes.rs::decode_body`; CLI body-file/stdin input; `keys.rs::{KeyError,KeyFailure}`; config and sealed-secret `Debug` implementations | **PASS** — write values stay in redacting types, read views expose only approved authority metadata, malformed secret-bearing bodies report positions rather than values, CLI accepts no secret-valued argv option, and key errors/logs expose stable classes only. The restored derived `OperatorKeys` debug traverses redacting config and a Vault client with no credential-bearing default headers; reqwest's client debug also omits proxy authentication. |
| Registration and per-attempt authority | `REQ-149`, `REQ-143`, `AC-029`, `AC-031` | `wyrd-spec/src/card/operator.rs`; `components/cards/resolve.rs::check_operator`; `verification/operators.rs::{resolve,credential}`; registration/disable/rotation journeys | **PASS** — registration and every delivery attempt share exact provider/name/origin/auth/header compatibility before decryption. Workflow remains non-executable; disabled, wrong-provider, wrong-origin, missing, and other-tenant connections fail closed. |
| SSRF, redirects, and credential ordering | `REQ-142`, `REQ-149`, Scenario 7; `architecture/agent-rules.md` resolve-screen-pin rule | `wyrd-auth-oidc/src/screening.rs`; `verification/operators.rs::{http,post_json,client,Credential}`; focused ordering tests | **PASS** — all DNS answers are screened, connections are pinned to screened addresses, metadata/link-local is always blocked, production blocks internal ranges, and redirects are manual, same-origin, bounded, and re-screened. Credentials are attached only after the effective hop has a screened client. |
| Provider disclosure boundary | `REQ-138`, `REQ-140`, `REQ-141`, `REQ-142` | `verification/operators.rs`; private Slack/PagerDuty adapters; frozen failure context; dispatch persistence | **PASS** — payloads derive only from bounded frozen context; secrets are not stored in dispatch errors; response parsing is bounded where needed; terminal/retry diagnostics carry statuses or stable codes rather than request credentials or provider bodies. |

## Prior-finding closure

| Prior finding | Cumulative-candidate result |
|---|---|
| `FIND-TASK-007-1`–`FIND-TASK-007-3` | **CLOSED** — selector-free failures/readiness and screen-pin-before-attachment remain intact. |
| `FIND-TASK-007-5`, `FIND-TASK-007-7`, `FIND-TASK-007-9` | **CLOSED** — production HTTPS and owner-only secret files, offloaded reads, and independently bounded rewrap remain intact. |
| `FIND-TASK-007-10` | **CLOSED** — the R3 correction removes the remaining oversized-version panic while preserving API-role validation and Forge role ownership. |
| `FIND-TASK-007-11`, `FIND-TASK-007-12` | **CLOSED** — selector-free debug and private provider-wire ownership remain intact. The restored derived `OperatorKeys` debug was checked against its complete field graph and current reqwest debug behavior. |
| `FIND-TASK-007-4`, `FIND-TASK-007-6`, `FIND-TASK-007-8`, `FIND-TASK-007-13`–`FIND-TASK-007-15` | **No open security issue** — these are task, contract, provider-documentation, or repository-style findings; their R3 changes do not weaken the reviewed security boundary. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task or security authority.

### Positive Controls

- Permission decisions fail closed and use the canonical audit path.
- Forced RLS is the load-bearing tenant boundary for all connection secrets.
- Envelope ciphertext is context-authenticated and plaintext is zeroizing.
- External delivery uses resolve-screen-pin before credential attachment on
  every effective URL.
- Public responses, durable errors, logs, and debug output remain secret- and
  selector-free.

## Material proposed findings

None. No reachable path was found that bypasses Operator RBAC, selects another
tenant's connection, commits an allowed mutation without its audit decision,
exports plaintext credentials or KEKs, sends authenticated HTTP to an
unscreened address, follows a credential-bearing cross-origin redirect, or
turns an invalid active key version into a panic or narrowed persisted value.

## Verification evidence and limits

- Independently ran seven exact `wyrd-server` unit tests covering selector-free
  key failures, redacted key configuration debug, exact and oversized key
  versions across roles, blocked/unresolved destinations, and per-hop redirect
  credential ordering: **7 passed**.
- Independently ran `mise run check:tenant-isolation`: **passed**.
- Inspected recorded candidate evidence for Postgres-backed RBAC/audit/RLS,
  production readiness, rotation, connection management, delivery, CLI, MCP,
  SDK, codegen, and provider journeys. Those heavier lanes were not rerun in
  this bounded domain pass.
- Credentialed live Slack/PagerDuty smoke remains gated release evidence and
  was not run. Local mock-provider coverage exercises the reviewed protocol,
  secret, retry, SSRF, and disclosure boundaries but cannot prove external
  account policy or provider availability.
- Owner-only mode enforcement is Unix-specific. The approved task establishes
  no non-Unix production deployment obligation, so this review does not claim
  equivalent ACL validation elsewhere.
- CodeGraph was unavailable because this checkout has no `.codegraph/` index;
  callers and source were traced directly.

## Overall result

**PASS** — the immutable cumulative candidate satisfies the reviewed
security, RBAC, tenancy, secret-encryption/redaction, SSRF, and audit
obligations, closes the security-relevant prior findings, and introduces no
new material security finding.
