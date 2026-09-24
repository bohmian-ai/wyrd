# TASK-007 Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `2bd4ded8f212f7885b0dd78bcd450fa0ff118f3e`
- Task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35

The candidate identity was resolved before inspection. The review covered the complete base-to-candidate diff and did not modify the reviewed source.

## Reviewed boundary

This review traced the security-sensitive TASK-007 path from typed connection inputs through HTTP, MCP, CLI, Rust, Python, and TypeScript projections; RBAC and canonical audit calls; forced-RLS persistence; envelope sealing/opening/rewrap; deployment KEK configuration and Vault reads; Card registration compatibility; per-attempt tenant-scoped credential resolution; HTTP authority/template validation; DNS resolution, address screening, connection pinning, and redirect handling; provider request construction; redacted responses and error projection; and the integration evidence supplied for those paths.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Secret and KEK lifecycle | `REQ-139`, `REQ-147`, Scenario 3; `architecture/wyrd-security-posture.md` secret-provider and production-composition rules | `wyrd-crypt/src/lib.rs`; `operators/keys.rs`; `config.rs`; `boot/mod.rs`; connection and delivery tests | FAIL: SEC-001, SEC-002, SEC-003 |
| Connection tenancy and persistence | `REQ-147`, `REQ-148`, `INV-007`, `AC-031`; `AGENTS.md` SQL/RLS rules | migration `20260601000032_operator_connections.sql`; `wyrd-sql` connection queries/tests; service handlers | PASS: forced RLS, `TenantConn`, tenant-qualified uniqueness, ciphertext/wrapped-DEK storage, and redacted views are present |
| RBAC and audit | `REQ-145`, `INV-007`, `AC-030`; canonical audit authority | `operators/service.rs`, routes, MCP catalog/dispatch, permissions, HTTP journey assertions | PASS: reads use `operators:read`, mutations use `operators:write`, MCP write discovery is scoped, denials are audited, and write allows compose with the mutation transaction |
| Public secret handling | Scenario 4, `REQ-148`, `REQ-150`, `AC-031`; errors and agent-harness references | contracts, shared client, CLI, MCP, Python and TypeScript bindings/tests, route decoder | FAIL: SEC-003. Otherwise secret request values use redacted types, CLI uses file/stdin rather than argv, and read shapes contain no credential fields |
| Registration authority | `REQ-149`, `AC-029`, `AC-031` | `components/cards/resolve.rs`; `OperatorSpec::matches_authority`; registration journey | PASS: exact tenant/provider/name/status plus HTTP origin/auth authority are checked without decrypting |
| HTTP SSRF and credential sequencing | `REQ-149`, `REQ-142`, Scenario 7, task acceptance criteria; security-posture resolve-screen-pin rule | `wyrd-auth-oidc/screening.rs`; `verification/operators.rs`; delivery journey | FAIL: SEC-004. DNS answers are otherwise screened as a set, pinned into the client, redirects are manual/same-origin/re-screened, and time/response bounds are present |
| Provider credential use | `REQ-140`, `REQ-141`, `REQ-142` | Slack, PagerDuty, and HTTP request paths plus mock-provider journey | PASS for reviewed security controls: sensitive HTTP credential header values are marked sensitive, Slack checks `ok`, and provider errors do not echo credential bytes |

## Material findings

### SEC-001 — VIOLATION: multi-tenant production can start without a readable active tenant KEK

- **Violated obligation:** Scenario 3 and `REQ-147` require multi-tenant production to fail startup when its configured external provider or active 32-byte tenant key is unavailable. Security posture requires missing production security capabilities to prevent readiness.
- **Exact location:** `crates/wyrd/wyrd-server/src/config.rs:1872-1917`; `crates/wyrd/wyrd-server/src/boot/mod.rs:1445-1456`; contrary proof in `crates/wyrd/wyrd-server/tests/pg_operator_connection_routes.rs:497-535`.
- **Evidence:** configuration validation checks only that the source is tagged `Vault` and that address/token fields are present. Boot then constructs `OperatorKeys` synchronously without reading the external provider or validating any tenant's active 32-byte key. The test deliberately starts a server with no active key and expects only a later credential write to fail.
- **Reachable consequence:** a production deployment reports successful startup/readiness while every connection creation/rotation and every dispatch needing an unavailable key fails. This is the fail-open deployment state the approved startup gate was meant to prevent, and can turn key-provider misconfiguration or key retirement into an apparently healthy but non-delivering service.
- **Testable correction:** in multi-tenant production boot, use the existing tenant inventory and `OperatorKeys` owner to read and decode the configured active version for every provisioned tenant before the server becomes ready; abort startup on provider, transport, missing-value, or wrong-length failure. Replace the current missing-key-starts-successfully proof with a production-profile boot refusal and a positive boot case backed by readable 32-byte tenant keys. Development can retain the approved deferred-failure behavior.

### SEC-002 — VIOLATION: production Vault permits plaintext HTTP for the token and KEK

- **Violated obligation:** `REQ-147` requires the KEK to remain in the external secret boundary; the security posture requires credentials and key material to be protected at receiving boundaries. Multi-tenant production must not transmit its Vault credential or KEK without transport encryption.
- **Exact location:** `crates/wyrd/wyrd-server/src/config.rs:1903-1905`; `crates/wyrd/wyrd-server/src/components/operators/keys.rs:213-223`.
- **Evidence:** `OperatorKeysConfig::validate` accepts both `http` and `https` regardless of production profile. `vault_key` sends `X-Vault-Token` to that address and accepts the returned base64 KEK.
- **Reachable consequence:** an operator typo or malicious configuration using `http://vault...` in the required multi-tenant production path exposes the Vault token and tenant KEKs to a network observer or active intermediary. The observer can retrieve or replace key material, compromising every Operator credential protected by those keys.
- **Testable correction:** reject non-HTTPS Vault addresses in production configuration before boot (retaining HTTP only for an explicitly non-production local fixture if needed), and add configuration tests proving production refuses `http://` while accepting a valid `https://` Vault address.

### SEC-003 — VIOLATION: public errors and logs expose deployment secret selectors and raw provider/filesystem diagnostics

- **Violated obligation:** Scenario 4 forbids secret selectors in responses, errors, logs, traces, audit, and parsed CLI diagnostics. The repository error authority forbids leaking raw filesystem/provider errors through public `WyrdError` payloads.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/operators/keys.rs:57-63`, with selector construction at `:150-173` and `:187-227`; the route test at `crates/wyrd/wyrd-server/tests/pg_operator_connection_routes.rs:497-535` positively asserts selector detail reaches the response.
- **Evidence:** `From<KeyError> for WyrdError` both logs `%error` and copies `error.to_string()` into the public message. `KeyError::Unavailable` contains the environment-variable name, exact key-file path, or Vault mount/prefix/tenant/version plus raw I/O, transport, or HTTP-status reason.
- **Reachable consequence:** any principal allowed to manage connections can force a missing-key error and learn internal environment names, filesystem layout, or Vault secret paths and status. The same selectors are emitted to logs, expanding sensitive deployment topology into diagnostic sinks.
- **Testable correction:** keep detailed causes inside the owner only as non-public sources, project a constant safe `WYRD_OPERATOR_503_KEY_UNAVAILABLE` message, and log only non-selector fields needed for operation (source kind and a stable failure class, not env names, paths, Vault paths, URLs, or raw I/O text). Add HTTP and logging assertions using sentinel selectors to prove none appear.

### SEC-004 — VIOLATION: HTTP credentials are attached before effective-URL SSRF screening and pinning

- **Violated obligation:** Scenario 7, `REQ-149`, and the task acceptance criteria require every effective URL/redirect to retain authority and pass resolve-screen-pin before matching credentials are attached.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/operators.rs:854-927` attaches bearer/basic/custom-header credentials in `HttpRequest::render`; only afterward does `OperatorDelivery::http` call the screening/pinning client at `:749-762`.
- **Evidence:** the rendered request's `HeaderMap` already contains the decrypted credential before DNS resolution or network-policy validation starts. Redirects are re-screened before send, but retain that already credential-bearing header map.
- **Reachable consequence:** a forbidden destination correctly receives no request, but decrypted credential material is copied into transport state before the security decision that is required to precede attachment. This breaks the approved containment boundary and leaves screening/error/cancellation paths handling credential-bearing request state they were specified never to receive.
- **Testable correction:** render and authority-check the effective URL and nonsecret request first, resolve/screen/pin it, and only then attach the matching credential to the builder used for that screened send; repeat that order after every redirect. Add a focused proof at the delivery boundary that blocked initial and redirected destinations are rejected before the credential-attachment step is reached.

## Positive controls

- Envelope encryption uses AES-256-GCM with OS randomness, a fresh DEK and nonce, domain-separated length-prefixed AAD, zeroized key/plaintext buffers, and context binding across tenant, connection, provider, name, and secret version.
- The connection table stores ciphertext and wrapped-DEK material only, forces RLS, and exposes cross-tenant key-version discovery through a column-limited privileged capability.
- Missing, disabled, wrong-provider, wrong-tenant, and mismatched-authority connections converge on safe delivery/registration refusals before decryption.
- HTTP Operator URL origins are normalized, templates are excluded from origins, server-owned headers are rejected case-insensitively, authored `Idempotency-Key` is forbidden, and credential header values are marked sensitive.
- The DNS implementation rejects mixed allowed/blocked answers and pins the screened result into reqwest, closing the ordinary DNS-rebinding check/use race.
- CLI secret input is file/stdin-only and malformed body errors report position rather than the offending value; shared SDK read results use the redacted view contract.

## Verification limits

- The task's recorded green command results were available as candidate evidence, but commands were not rerun during this time-bounded static domain review. Green tests do not cover SEC-001 through SEC-004; one existing test explicitly locks in SEC-001/SEC-003 behavior.
- The credentialed Slack/PagerDuty live smoke remains gated and was not run. Provider acceptance was assessed from source and local mock-journey coverage only.
- Generated schemas/declarations were checked for their connection secret/redacted shapes through their source contracts and representative projections, not line-by-line across every duplicate generated JSON artifact.
- This review did not audit unrelated verification runtime, scheduling, or result-publication behavior except where it directly carried an Operator credential, permission decision, audit record, or tenant identity.

## Overall result

**FAIL**

TASK-007 does not satisfy its approved security boundary while SEC-001 through SEC-004 remain. Each correction is bounded within the existing approved behavior and does not require a specification revision.
