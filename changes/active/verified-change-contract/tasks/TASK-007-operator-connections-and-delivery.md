---
id: TASK-007
kind: implementation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-097, REQ-098, REQ-099, REQ-138, REQ-139, REQ-140, REQ-141, REQ-142, REQ-143, REQ-145, REQ-146, REQ-147, REQ-148, REQ-149, REQ-150, REQ-152, INV-006, INV-007, INV-011, INV-013, INV-015, AC-029, AC-030, AC-031, AC-033]
depends_on: [TASK-004, TASK-010]
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
and redacted secret handling; the server-owned `OperatorKeys` owner reads
tenant/version KEKs from one configured environment, owner-only file, or
HashiCorp Vault KV v2 source. `wyrd-server` owns CRUD handlers, transactional audit,
registration compatibility checks, the supervised Operator worker, SSRF
screen/pin, provider adapters, limits, and status. `wyrd-client`, SDKs, CLI, and
MCP project the same contract.

Reuse the current administration patterns: typed path identifiers at the wire
boundary, one shared-client capability, ambient/file/environment secret input
that never enters CLI argv or derived `Debug`, and runtime OpenAPI assembled
from the mounted typed routes. Do not restore a checked-in OpenAPI document or
generator.

Do not add environment selectors, plaintext persistence/read-secret, a new
cipher/dependency, credential cache, replica watcher, broker, Alert table,
provider upload lifecycle, or executable Workflow action. Security validation,
SSRF pinning, RLS, and auditing may not be simplified away.

## Approach

1. Add provider-tagged request/update/redacted view contracts and typed IDs,
   then forced-RLS SQL storage with UUIDv7 identities and audit composition.
2. Extend `wyrd-crypt` authenticated associated data and envelope operations;
   wire the approved `OperatorKeys` tenant/version KEK source and rotation
   model.
3. Expose HTTP/shared-client/SDK/CLI/MCP management operations with typed IDs,
   write-only secret handling outside CLI argv/debug, normal permissions, and
   route-owned runtime OpenAPI.
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

**GREEN.** Extend the existing cryptography owner, read KEKs through
`OperatorKeys`, and compose the insert/audit on `TenantConn`.

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

**Behavior.** Multi-tenant production fails startup unless it uses the
HashiCorp Vault KV v2 source over HTTPS and every active tenant's active
32-byte key is readable before readiness. Development may use env and
explicit single-tenant may use restrictive file mounting only as approved.
Publish-before-active rotation makes new writes use the new version, bounded
tenant work rewraps DEKs without decrypting credentials, and old versions stay
until unreferenced.

**RED.** Add configuration, missing/wrong-size/unavailable key, AAD tamper,
wrong-tenant/key-version, publish/activate/rewrap/retire, and cancellation cases
using a local Vault KV v2 fixture.

**GREEN.** Reuse `wyrd-crypt` and the already-installed `reqwest` for the
server-owned `OperatorKeys` env/file/Vault KV v2 source; add only AAD,
wrap/unwrap, and bounded rewrap orchestration. A shared `SecretRef` resolver
and AWS Secrets Manager / Google Secret Manager KEK sources are deferred.

**REFACTOR.** No cloud SDK or generic KMS framework is added to foundational
crates.

### Scenario 4 — Public management surfaces share one typed client

**Behavior.** HTTP CRUD, Rust/Python/TypeScript, CLI, and MCP expose identical
closed operations. Reads require `operators:read`; mutations require
`operators:write`; MCP writes require explicit write scope. Responses/errors,
logs, traces, audit, CLI argv, and parsed-argument `Debug` never expose secret
bytes or secret selectors. Typed IDs reach handlers without stringly reparsing,
and the served OpenAPI document describes the mounted routes and typed IDs.

**RED.** Add real tenant-admin journeys through each surface including
under-privileged and cross-tenant attempts, CLI secret-argument refusal/debug
redaction, malformed typed path IDs, and served OpenAPI contract coverage.

**GREEN.** Add one shared-client capability and thin language/CLI/MCP
projections, register typed routes in the runtime OpenAPI assembly, and
regenerate only repository-owned schema/stub artifacts.

**REFACTOR.** Delete surface-specific transports and serializers; add no
checked-in OpenAPI snapshot or generator.

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
rewrites result or reruns Verifier. PostgreSQL assigns and evaluates dispatch
availability, leases, retry/backoff eligibility, and the five-minute deadline.

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
- CLI secret inputs never enter argv or parsed-argument `Debug`; runtime
  OpenAPI and MCP catalogs match their mounted typed operations.
- HTTP authority/SSRF checks occur before secret attachment on every attempt.
- Delivery status is durable and independent; no Alert resource exists.

## Expected Write Set and Consumer Closure

Likely owners: `wyrd-spec` Operator/connection/API/ID/error contracts,
`wyrd-crypt`, server `OperatorKeys` source/config, `wyrd-sql` migrations/queries,
server handlers/registration/runtime/providers/SSRF/status, shared client,
three SDKs, CLI, MCP, OpenAPI/schemas, and local/live provider journeys.

## Verification and Evidence

```bash
mise run test:sql
mise run test:shared
mise run test:wyrd
mise run test:wyrdstate:journey
mise run test:platform:journey
mise run test:cli:journey
mise run test:principals:integration
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
delivery claim, a checked-in OpenAPI snapshot/generator, secrets accepted in
CLI argv/debug, or weaker SSRF/audit/tenancy behavior.

## Authority Links

- `changes/active/verified-change-contract/spec.md`
- `changes/active/verified-change-contract/architecture/verification-control-flow.html`
- `architecture/wyrd-security-posture.md`
- `architecture/agent-rules.md`
- `AGENTS.md`

## Implementation Evidence

Decision recorded with the user: Wyrd stores Operator credentials itself in
Postgres, envelope-encrypted under a versioned key-encryption key read from an
environment variable, owner-only files, or HashiCorp Vault KV v2 (via the
existing `reqwest`). Multi-tenant production requires Vault; without a readable
key only connection create/update refuses. AWS/GCP key providers are deferred.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-029 fan-out, Slack channel + bot token + JSON `ok`, PagerDuty route/key/dedup, HTTP bounded context, Idempotency-Key, effective-URL SSRF | `crates/wyrd/wyrd-server/src/verification/operators.rs` (`OperatorWorker`, `OperatorDelivery`) | `pg_operator_delivery::failed_verdict_fans_out_to_every_provider_independently` | PASS |
| AC-029 terminal provider error, rate limit + Retry-After, origin-changing redirect, independent statuses, Verifier result unchanged | same | same test | PASS |
| AC-029 revoked/missing connection fails closed; key outage retries; no provider call without credential | `OperatorWorker::credential` | `pg_operator_delivery::revoked_connection_fails_closed_and_key_outage_retries` | PASS |
| AC-029 unsupported Workflow / wrong-provider / wrong-origin / wrong-scheme / disabled refused at registration | `components/cards/resolve.rs::check_operator` | `pg_operator_connection_routes::registration_binds_exact_connection_authority` | PASS |
| AC-029 gated live Slack/PagerDuty smoke through the same runner | `pg_operator_delivery::live_smoke_delivers_to_slack_and_pagerduty` (`#[ignore]`, `WYRD_LIVE_*`) | Release evidence; not run (no credentials in fast lanes) | GATED |
| AC-030 30s attempt timeout, 3 attempts, 30s/2m backoff, terminal after budget, no Verifier rerun | `RuntimeLimits::operator_*`, `OperatorWorker::settle` | `pg_operator_delivery::slow_endpoint_exhausts_the_budget_and_shutdown_releases` | PASS |
| AC-030 worker crash restarts via health; shutdown drain releases in-flight with attempt refunded | `Capability::OperatorWorker`, `OperatorWorker::run` | same test | PASS |
| AC-030 4 per-tenant / 16 global Operator permits; other tenant progresses | `OperatorWorker` permits | `pg_operator_delivery::operator_permits_cap_each_tenant_without_starving_another` | PASS |
| AC-030 `operators:read` vs `operators:write` separation with audit | routes, MCP `may_manage` | `pg_operator_connection_routes::read_write_separation_and_tenant_isolation`, MCP `operators` journey, TS journey | PASS |
| AC-031 admin CRUD, redaction, rotate, disable/re-enable, UUIDv7, RLS, ciphertext-only rows, key version | connection service/routes/SQL | `pg_operator_connection_routes::admin_manages_redacted_encrypted_connections` | PASS |
| AC-031 every SDK + CLI + MCP projects the same contract | `wyrd-client::OperatorConnections`; Python `wyrd.operators`; TS `OperatorConnections`; CLI `operator-connection`; MCP `operator_connections.*` | Rust SDK `operator_connections`, `test_operator_connections_journey.py`, `operator-connections.test.ts`, `test:cli:journey`, `test:bifrost:journey:mcp` | PASS |
| AC-031 multi-replica rotation observed on next attempt without Card revision; rewrap to new key version | `OperatorKeys::rewrap_pass` from the worker | `pg_operator_delivery::next_attempt_on_another_replica_uses_the_rotated_credential` | PASS |
| CLI secrets never in argv/Debug | `wyrd-cli/src/operator_connection.rs` body files only | `cli.rs` refusal cases, `body_refusal_never_echoes_values` | PASS |
| AC-033 retry/lease timestamps from `statement_timestamp()`; tests move DB rows, not clocks | `OperatorDispatchQueue` SQL | delivery journeys use `make_retries_due` | PASS |

Verification (all run in this session, exit 0): `test:sql`, `test:shared`,
`test:wyrd`, `test:wyrdstate:journey`, `test:platform:journey`,
`test:cli:journey`, `test:principals:integration`,
`test:bifrost:journey:server`, `test:bifrost:journey:mcp`,
`py:test:integration`, `py:typecheck`, `ts:test:integration`, `ts:typecheck`,
`codegen:check`, `check:tenant-isolation`, `check:client-tier`,
`check:pyo3-scope`, `check:unwrap-audit`, `fmt`, `py:format`, `lints`,
`py:lints`, `git diff --check`. Focused:
`scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_operator_delivery --test-threads=1` (5 passed, 1 ignored).

Non-goals held: no Alert resource, no executable Workflow action, no new
provider, no cloud KMS SDK, no checked-in OpenAPI snapshot, no exactly-once
claim. `architecture/wyrd-design.md` Operator section updated to the approved
connection and delivery contract.
