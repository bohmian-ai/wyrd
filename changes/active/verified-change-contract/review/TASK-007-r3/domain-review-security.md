# TASK-007-R3 Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: `TASK-007-R1-operator-delivery-corrections.md` and
  `TASK-007-R2-operator-delivery-corrections.md`

The candidate was `HEAD` before and after inspection. The complete cumulative
base-to-candidate security boundary was reviewed; no reviewed source was
modified.

## Reviewed boundary

This review traced Operator connection secrets from typed HTTP, MCP, SDK, and
CLI inputs through permission evaluation, canonical audit, tenant transaction,
forced-RLS persistence, envelope encryption, key resolution, rotation, Card
registration, dispatch resolution, per-attempt decryption, outbound DNS
screening and address pinning, credential attachment, redirect handling, and
public error/log projection. It also reassessed the security-relevant closure
of all prior `FIND-TASK-007-*` identities against the cumulative candidate.

## Authority and source coverage

| Boundary | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| RBAC and transactional audit | `REQ-148`, `REQ-145`, `INV-007`, `AC-030`; `AGENTS.md` and `architecture/agent-rules.md` audit rules | `wyrd-runtime/src/permission.rs:389-408`; `components/operators/{routes,service}.rs`; `mcp/{mod,operators}.rs`; `audit/mod.rs:206-270`; `pg_operator_connection_routes.rs:382-495` | **PASS** — reads evaluate and record `operators:read`; writes evaluate `operators:write`, append the allow in the mutation transaction, record denials before refusal, and fail closed when audit cannot commit. MCP write discovery is scope-gated and the owner re-authorizes direct calls. |
| Tenant isolation and privileged discovery | `REQ-139`, `REQ-147`, `INV-007`, `AC-031`; `TenantConn`/`OperatorPool` rules | migration `20260601000032_operator_connections.sql:11-50`; `wyrd-sql/src/queries/operator_connections.rs`; management, registration, delivery, and rewrap callers; SQL and route journeys | **PASS** — connection CRUD and resolution use caller/run-derived tenant `TenantConn` under forced RLS. Cross-tenant `OperatorPool` access is limited to `(data_tenant_id, key_version)` discovery for rewrap. Foreign IDs and names cannot select or widen tenant state. |
| Envelope encryption and key rotation | `REQ-139`, `REQ-147`, Scenario 3; security-posture cryptography rules | `wyrd-crypt/src/lib.rs:18-223`; `components/operators/keys.rs:164-569`; SQL row mapping/fencing; key and multi-replica rotation proofs | **PASS** — fresh OS-random DEK and nonces, AES-256-GCM, domain-separated canonical AAD, tenant/connection/provider/name/secret-version binding, exact key version, zeroizing key/plaintext buffers, per-attempt reads, and fenced DEK-only rewrap are present. Postgres stores ciphertext and wrapped DEKs, never a KEK or plaintext secret. |
| Key-provider configuration and readiness | `REQ-147`, Scenario 3; `architecture/wyrd-security-posture.md` production-composition and secret-file rules | `config.rs:1774-2015`; `boot/mod.rs:1335-1342,1444-1476`; `components/operators/keys.rs:302-405,596-645`; readiness/config tests | **PASS** — multi-tenant production accepts only Vault over HTTPS and checks every active tenant's exact active 32-byte key before an API-serving process becomes ready. Vault redirects are disabled, TLS verification remains enabled, token/key files require owner-only permissions, file reads are offloaded, and development/single-tenant allowances remain confined to the approved profiles. |
| Secret input, redaction, and public errors | `REQ-139`, `REQ-148`, `REQ-150`, Scenario 4, `AC-031`; security-posture diagnostic rules | `wyrd-spec::{operator_connection,auth/secret_bearer,error}.rs`; HTTP body decoder; CLI body-file/stdin path; `keys.rs::{KeyError,KeyFailure,log}`; custom `Debug` implementations; SDK/MCP projections and tests | **PASS** — write fields use redacted wrappers, read views contain only approved authority metadata, malformed secret-bearing bodies report positions rather than values, CLI accepts no secret-valued argv option, key-owner debug output is selector-free, and complete public key failures carry constant detail/remediation without provider selectors or raw causes. |
| Registration and per-attempt credential authority | `REQ-149`, `REQ-143`, `AC-029`, `AC-031` | `wyrd-spec/src/card/operator.rs:63-221,492-527`; `components/cards/resolve.rs:112-148,243-292`; `verification/operators.rs:247-379`; registration, disable, and rotation journeys | **PASS** — registration and every attempt share the exact provider/name/origin/auth/header compatibility predicate before decryption. Workflow dispatch is rejected, missing/disabled/wrong-provider/wrong-origin/wrong-tenant connections fail closed, and rotation is observed from Postgres on the next attempt. |
| SSRF, redirects, and credential ordering | `REQ-142`, `REQ-149`, Scenario 7; `architecture/agent-rules.md` resolve-screen-pin rule | `wyrd-auth-oidc/src/screening.rs:94-217`; `verification/operators.rs:493-858`; focused ordering tests and mock delivery journey | **PASS** — every DNS answer is screened, mixed allowed/blocked results are rejected, the connection is pinned to the screened addresses, metadata/link-local is always blocked, production blocks loopback/private/CGNAT/ULA, and redirects are manual, same-origin, bounded, and re-screened. HTTP and Slack credential headers are attached only after the effective hop has a screened client; blocked, unresolved, and origin-changing destinations receive no credential. |
| Provider delivery and bounded disclosure | `REQ-138`, `REQ-140`, `REQ-141`, `REQ-142`; security-posture privacy rules | `verification/operators.rs`, private `operators/{slack,pager_duty}.rs`, frozen-context contract, dispatch SQL, provider journeys | **PASS** — payloads derive only from the bounded frozen failure context, provider wire details remain private to their adapters, secret-bearing values are not persisted in dispatch errors, response bodies are bounded where parsed, and terminal/retry messages contain status or stable provider codes rather than request credentials. |

## Prior-finding closure

| Prior finding | Cumulative-candidate result |
|---|---|
| `FIND-TASK-007-1` | **CLOSED** — key detail, remediation, logs, and custom debug output contain no concrete or template secret-provider selector (`error.rs:1697-1707`; `keys.rs:101-161,869-1000`). |
| `FIND-TASK-007-2` | **CLOSED** — multi-tenant production readiness enumerates active tenants and verifies each active key before readiness (`boot/mod.rs:1444-1476`). |
| `FIND-TASK-007-3` | **CLOSED** — each initial or redirected HTTP hop is screened and pinned before credential attachment (`verification/operators.rs:636-697,819-858`). |
| `FIND-TASK-007-4` | **CLOSED / non-security seam** — the cumulative diff contains no unrelated workflow-skill policy edits. |
| `FIND-TASK-007-5` | **CLOSED** — production Vault requires HTTPS and key/token files share owner-only reads (`config.rs:1930-1951`; `keys.rs:333-382,596-625`). |
| `FIND-TASK-007-6` | **CLOSED / contract seam inspected** — precise provider-tagged Python and TypeScript request/view contracts do not create an alternate secret-reading surface. |
| `FIND-TASK-007-7` | **CLOSED** — mounted key and Vault-token reads use `spawn_blocking` (`keys.rs:596-607`). |
| `FIND-TASK-007-8` | **CLOSED / standards seam** — the repaired imports do not hide a second security owner or path. |
| `FIND-TASK-007-9` | **CLOSED** — rewrap is independent of delivery and both cross-tenant discovery and tenant work are inside the existing finite pass budget (`keys.rs:511-569`). |
| `FIND-TASK-007-10` | **CLOSED** — configuration rejects values above `i32::MAX`, and the owner preserves the validated value exactly (`config.rs:1893-1911`; `keys.rs:210-290`). |
| `FIND-TASK-007-11` | **CLOSED** — `VaultKeysConfig`, `OperatorKeysConfig`, and `OperatorKeys` have selector-free custom `Debug` implementations (`config.rs:1820-1825,1864-1873`; `keys.rs:225-235`). |
| `FIND-TASK-007-12` | **CLOSED / provider-security seam** — Slack/PagerDuty request construction and response interpretation are private provider modules; common screening, transport, bounds, and credential-order controls remain in `OperatorDelivery`. |

## Material proposed findings

None.

No reachable path was found that bypasses Operator RBAC, selects another
tenant's connection, commits an allowed mutation without its audit decision,
exports plaintext credentials or KEKs, sends authenticated HTTP to an
unscreened address, follows a credential-bearing cross-origin redirect, or
returns selector-rich key errors.

## Verification evidence and limits

- Independently ran the six exact server unit tests covering selector-free key
  failures, redacted key-owner `Debug`, exact key-version validation, blocked
  and unresolved destinations, and per-hop redirect credential ordering: all
  six passed.
- Independently ran `mise run check:tenant-isolation`: passed.
- Inspected the candidate's recorded green SQL, server, route, delivery, CLI,
  MCP, SDK, codegen, lint, and boundary-lane evidence and traced the relevant
  production owners and callers. The heavier Postgres-backed journeys were not
  rerun in this bounded domain pass.
- Credentialed live Slack/PagerDuty smoke remains gated release evidence and
  was not run. The local mock-provider journeys cover the reviewed protocol,
  credential, SSRF, retry, and disclosure boundaries but cannot prove external
  account policy or provider availability.
- Owner-only file-mode enforcement is Unix-specific. The approved task does
  not establish a non-Unix production deployment obligation, so this report
  does not claim equivalent ACL validation on other operating systems.
- This domain pass does not accept or reject non-security task completeness or
  stylistic repository findings except where they create a security-boundary
  seam; those remain the task, standards, delivery, and Wave-2 reviewers'
  authority.

## Overall result

**PASS** — the immutable cumulative candidate closes the prior security,
RBAC, tenancy, key-boundary, redaction, and SSRF findings, and no new material
security finding was identified.
