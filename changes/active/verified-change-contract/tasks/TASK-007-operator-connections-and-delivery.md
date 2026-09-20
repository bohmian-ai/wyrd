---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 32
requirements: [REQ-097, REQ-098, REQ-099, REQ-138, REQ-139, REQ-140, REQ-141, REQ-142, REQ-143, REQ-145, REQ-146, REQ-147, REQ-148, REQ-149, REQ-150, INV-006, INV-007, INV-011, INV-013, AC-029, AC-030, AC-031]
depends_on: [TASK-004]
---

## Outcome and Value

Tenant administrators manage Slack, PagerDuty, and HTTP credentials through one
typed, redacted, encrypted Postgres control plane. A failed binding result fans
out durable independent dispatches whose worker resolves the latest credential,
sends a bounded Notify/HTTP action, and exposes delivery status without an
Alert resource or secret leakage.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-spec` owns closed connection/Operator/API/error contracts. `wyrd-sql`
owns forced-RLS connection/dispatch persistence. `wyrd-crypt` owns AES-256-GCM
and redacted secret handling; the existing external secret resolver supplies
tenant/version KEKs. `wyrd-server` owns CRUD handlers, transactional audit,
registration compatibility checks, the supervised Operator worker, SSRF
screen/pin, provider adapters, limits, and status. `wyrd-client`, SDKs, CLI, and
MCP project the same contract.

Do not add environment selectors, plaintext persistence/read-secret, a new
cipher/dependency, credential cache, replica watcher, broker, Alert table,
provider upload lifecycle, or executable Workflow action. Security validation,
SSRF pinning, RLS, and auditing may not be simplified away.

## Approach

1. Add provider-tagged request/update/redacted view contracts and typed IDs,
   then forced-RLS SQL storage with UUIDv7 identities and audit composition.
2. Extend `wyrd-crypt` authenticated associated data and envelope operations;
   wire the approved external tenant/version KEK resolver and rotation model.
3. Expose HTTP/shared-client/SDK/CLI/MCP management operations with write-only
   secrets and normal permissions.
4. Validate connection/provider/HTTP authority during registration without
   decrypting and freeze bounded failure context/destination per dispatch.
5. Implement independent leased delivery with approved provider protocols,
   SSRF controls, retry/deadline rules, supervision, and status.

## Ordered Implementation Scenarios

### Scenario 1 — Connections persist ciphertext and redacted metadata only

**Behavior.** Creating each provider mints UUIDv7 identity, validates immutable
name/provider-specific config, generates fresh DEK/nonce, authenticates the
canonical context, wraps under the exact tenant/version KEK, and stores no
plaintext. List/get returns only the approved redacted union. Cross-tenant and
under-privileged access fails closed and audits allow/deny transactionally.

**RED.** Add Postgres/handler cases for all providers, row inspection, redacted
responses, UUID version, RLS, auth failures, and injected audit/encryption
failure rollback.

**GREEN.** Extend existing cryptography and secret-resolver owners and compose
the insert/audit on `TenantConn`.

**REFACTOR.** Keep secret-bearing request values in redacted types and remove
duplicate provider maps or debug output.

### Scenario 2 — Update, rotation, disable, and re-enable preserve identity

**Behavior.** PATCH omits-to-preserve and supplied-to-replace semantics follow
the provider-specific union; provider/name never change. DELETE disables rather
than removes. Secret rotation changes ciphertext/version on the same ID and is
visible to every replica's next attempt without Card/server rollout.

**RED.** Add metadata-only, secret-only, combined, wrong-provider, immutable-
field, disable/re-enable, concurrent/multi-replica, and no-read-secret cases.

**GREEN.** Implement atomic typed updates and per-attempt reads from Postgres.

**REFACTOR.** Reuse one SQL row/version lifecycle rather than append-only
connection records or caches.

### Scenario 3 — KEK startup and rotation obey the external boundary

**Behavior.** Multi-tenant production fails startup without its configured
external provider/active 32-byte tenant key. Development may use env and
explicit single-tenant may use restrictive file mounting only as approved.
Publish-before-active rotation makes new writes use the new version, bounded
tenant work rewraps DEKs without decrypting credentials, and old versions stay
until unreferenced.

**RED.** Add configuration, missing/wrong-size/unavailable key, AAD tamper,
wrong-tenant/key-version, publish/activate/rewrap/retire, and cancellation cases
using a local resolver.

**GREEN.** Reuse `SecretRef::Vault` resolver and `wyrd-crypt`; add only AAD,
wrap/unwrap, and bounded rewrap orchestration.

**REFACTOR.** No cloud SDK or generic KMS framework is added to foundational
crates.

### Scenario 4 — Public management surfaces share one typed client

**Behavior.** HTTP CRUD, Rust/Python/TypeScript, CLI, and MCP expose identical
closed operations. Reads require `operators:read`; mutations require
`operators:write`; MCP writes require explicit write scope. Responses/errors,
logs, traces, and audit never expose secret bytes or secret selectors.

**RED.** Add real tenant-admin journeys through each surface including
under-privileged and cross-tenant attempts.

**GREEN.** Add shared-client capability and thin language/CLI/MCP projections,
then regenerate contracts.

**REFACTOR.** Delete surface-specific transports and serializers.

### Scenario 5 — Registration binds exact credential authority

**Behavior.** Slack/PagerDuty/HTTP Operator binding verifies an active exact
tenant/provider/name without decryption. HTTP Card auth variant/header and
every effective URL origin must equal stored authority; forbidden credential/
transport headers and Workflow on_failure fail before persistence. Missing,
disabled, wrong-provider, and wrong-tenant failures are safe and indistinguishable.

**RED.** Add composite-registration cases for all matches/mismatches, header
case handling, normalized origins, templates, and disabled records.

**GREEN.** Read redacted connection authority inside the existing registration
transaction and validation path.

**REFACTOR.** Keep one compatibility predicate reused immediately before each
delivery attempt.

### Scenario 6 — Failed results fan out independently

**Behavior.** A failed binding settlement creates one unique dispatch per
distinct effective Operator in the same transaction. Passed/inconclusive/
noncompleted/direct runs create none. Workers acquire global/per-tenant permits
before short leased claims; sibling success/failure is independent and never
rewrites result or reruns Verifier.

**RED.** Add settlement concurrency/idempotency, duplicate Operator, lease
expiry, retry exhaustion, sibling, restart, fairness, and status cases.

**GREEN.** Reuse TASK-004 settlement and existing skip-locked/fenced SQL
patterns under the generic Operator capability.

**REFACTOR.** Keep dispatch as the sole durable handoff and remove direct
Verifier calls/brokers.

### Scenario 7 — Provider delivery is bounded, pinned, and truthful

**Behavior.** Slack posts to authored channel and checks JSON `ok`; PagerDuty
sends route/custom detail with stable dedup key; HTTP renders only bounded
failure context, re-resolves/screens/pins each effective URL/redirect before
attaching matching credentials, and owns Idempotency-Key. The worker applies
30-second attempts, three attempts, 30s/2m backoff, five-minute deadline,
bounded Retry-After, terminal/transient classification, at-least-once ambiguity,
bounded response bodies, shutdown drain, restart, metrics, and health.

**RED.** Add local mock journeys for accepted and provider-declared failure,
rate limit, timeout, connection reset after send, forbidden redirect/network,
malformed template, credential-store outage, terminal credential error,
shutdown, and restart.

**GREEN.** Implement focused provider adapters behind the one worker and reuse
the repository's SSRF resolve-screen-pin and HTTP bounds.

**REFACTOR.** Keep provider wire parsing local and common retry/context logic on
the owning dispatch worker; no trait unless the real adapters share a stable
capability.

## Acceptance Criteria

- `AC-029` and `AC-031` pass, including multi-replica rotation and live-smoke
  release evidence outside credential-free fast lanes.
- No plaintext or key material reaches Postgres/public/diagnostic surfaces.
- HTTP authority/SSRF checks occur before secret attachment on every attempt.
- Delivery status is durable and independent; no Alert resource exists.

## Expected Write Set and Consumer Closure

Likely owners: `wyrd-spec` Operator/connection/API/ID/error contracts,
`wyrd-crypt`, external secret resolver/config, `wyrd-sql` migrations/queries,
server handlers/registration/runtime/providers/SSRF/status, shared client,
three SDKs, CLI, MCP, OpenAPI/schemas, and local/live provider journeys.

## Verification and Evidence

```bash
mise run test:sql
mise run test:shared
mise run test:wyrd
mise run test:e2e
mise run test:bifrost:journey:server
mise run test:bifrost:journey:mcp
mise run py:test:integration
mise run py:typecheck
mise run ts:test:integration
mise run ts:typecheck
mise run codegen:check
mise run check:tenant-isolation
mise run check:client-tier
mise run check:pyo3-scope
mise run check:unwrap-audit
mise run fmt
mise run py:format
mise run lints
mise run py:lints
git diff --check
```

Run new provider/crypto/SQL tests with exact focused selectors once named. The
credentialed Slack/PagerDuty smoke is gated release evidence, not a fast lane.

## Material Stop Conditions

Stop for a different key provider/derivation/rotation contract, new crypto
dependency, plaintext export, env selectors in Cards, widened HTTP authority,
new provider, changed retry ceilings, executable Workflow action, exactly-once
delivery claim, or weaker SSRF/audit/tenancy behavior.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `architecture/wyrd-security-posture.md`
- `architecture/agent-rules.md`
- `AGENTS.md`
