---
id: TASK-012
kind: implementation
status: in_progress
spec: SPEC-verified-change-contract
spec_revision: 45
requirements: [REQ-097, REQ-098, REQ-099, REQ-138, REQ-139, REQ-140, REQ-141, REQ-142, REQ-143, REQ-145, REQ-146, REQ-147, REQ-148, REQ-149, REQ-150, REQ-152, INV-006, INV-007, INV-011, INV-013, INV-015, AC-029, AC-030, AC-031, AC-033]
depends_on: [TASK-004, TASK-007, TASK-010]
---

# Replay completed TASK-007 onto the current change branch

## Outcome and Value

Integrate the completed TASK-007 Operator connections and delivery capability
into the up-to-date `vcc/task-007-replay` worktree. Tenant administrators submit
write-only Slack, PagerDuty, and HTTP credentials to Wyrd; Wyrd encrypts them
in tenant-scoped Postgres and delivers failed-verification reactions. Preserve
the current Tasks 001–006 and 010 behavior and the approved specification.

This task is proposed until specification revision 45 is explicitly approved
and recorded. The existing `tasks/TASK-007-operator-connections-and-delivery.md`
and its source branch remain the completed implementation input, not a task to
rewrite or reapprove in place.

## Owners, Scope, Consumers, and Prohibited Changes

The current worktree is the integration target; do not create another worktree
or rewrite `vcc/task-007`. Source tip `c6301fd83` contains the completed work;
its R6 PASS reviewed candidate `cd002ab13` under specification revision 36.
Neither that verdict nor its tests approve the integrated revision-45 tree.
Record the actual source and target heads again before merging.

`wyrd-server` owns Operator connection administration, key selection, delivery,
startup checks, and rotation. `wyrd-sql` owns forced-RLS connection and dispatch
state; `wyrd-crypt` owns encryption. `wyrd-spec` owns typed public contracts.
`wyrd-client`, the three SDKs, CLI, and MCP project them. Gateway owns the
Vault KV v2 reader capability and its behavior tests. Move its concrete reader
out of `wyrd-gateway` into a narrow shared crate, `crates/shared/wyrd-vault`;
both `wyrd-gateway` and `wyrd-server` import that client directly. Do not put
it in server-tier `wyrd-auth`, which would add auth/SQL dependencies to Gateway.
The one client owns Vault network, authentication, TLS,
address-screening/pinning, timeout, and response bounds. Gateway uses it to
read provider credentials; Operator delivery uses it to read 32-byte
tenant/version key-encryption keys. Those are different values with separate
configuration, authorization, validation, lifecycle, and error semantics in
their existing domain owners. No general secret resolver or `SecretRef`
Operator-key contract is introduced.

Keep write-only credential input, encrypted Postgres storage, redacted reads,
transactional authorization audit, tenant isolation, SSRF controls, bounded
delivery, and at-least-once reporting. Do not add a provider, cloud SDK, new
cipher, broker, credential cache, Alert resource, executable Workflow action,
plaintext export, or compatibility route. Keep the source branch's historical
review files as prior evidence only.

## Approach

1. After revision 45 is approved and the target worktree is clean, record both
   heads and merge `vcc/task-007` into this worktree with
   `git merge --no-ff --no-commit vcc/task-007`. Preserve the approved current
   spec and current Tasks 001–006/010 contracts during conflict resolution.
2. Inspect the entire merge result against the pre-merge target, including
   automatically merged files. A preview from `bcabb59a8` reports 22 textual
   conflicts across a 171-file source diff; those counts are orientation, not
   a substitute for a fresh inventory. Do not choose branch-wide `ours` or
   `theirs`, cherry-pick the stale series, or discard nonconflicting source work.
3. Extract the current Gateway Vault KV v2 reader into `wyrd-vault` and make
   Gateway and Operator delivery import it. Keep Gateway credential reads and
   Operator KEK reads distinct. Preserve the
   approved env-development, restricted-file-single-tenant, and Vault
   multi-tenant production policy, exact tenant/version path, active-key
   readiness, and rewrap behavior. Remove Task 007's duplicate Vault HTTP
   reader. Keep `SecretRef` out of Operator configuration and contracts.
4. Reconcile current verification settlement, Postgres coordination clock,
   boot/supervision, registration, SQL, provider delivery, and all public
   projections. Update active architecture text that still describes Operator
   environment selectors; regenerate schemas and stubs from source.
5. Run focused tests during iteration and the final aggregate below.
   Audit every changed file, commit the integrated candidate, and obtain a
   fresh immutable task review against approved revision 45.

## Ordered Implementation Scenarios

### Scenario 1 — Gateway-owned Vault reader shared by two consumers

**Behavior.** Gateway still retrieves provider credentials, while Operator
delivery retrieves the exact 32-byte KEK for one tenant and key version. Both
import the same bounded, screened and pinned Vault KV v2 client from
`wyrd-vault`; neither performs a separate Vault HTTP read. An unavailable,
wrong-sized, wrong-tenant, or wrong-version Operator key fails closed; multi-
tenant production cannot become ready without every active tenant's active
key. Gateway credential behavior remains intact.

**RED.** Bring forward source Operator key tests and run the existing Gateway
Vault journey against the current tree. Add the smallest focused local-Vault
regression proving both callers use the same reader and preserve their distinct
value validation and failure behavior; observe the missing Operator path or
duplicate-reader behavior before changing it.

**GREEN.** Move the current Gateway Vault reader into the narrow shared crate;
make Gateway and Operator delivery import it. Keep provider credential and KEK
interpretation in their respective owners, and preserve the source
encryption/rewrap behavior. Rerun both callers' focused proof.

**REFACTOR.** Remove the old Gateway-local and Task 007 Vault request code and any
Operator-only `SecretRef` plumbing. Keep provider-credential and KEK policy in
their respective owners rather than introducing a generic secret framework.

### Scenario 2 — Public upload and metadata; server-only credential reads

**Behavior.** Tenant admins create or replace Slack/PagerDuty/HTTP credentials
through HTTP, Rust/Python/TypeScript SDKs, CLI, or MCP. Those are write-only
inputs; CLI takes secret material from a file or stdin, never argv. Wyrd
encrypts each value under the exact tenant/version key and stores no plaintext.
Public list/get operations on every surface return connection metadata only
(ID, provider, name, nonsecret configuration, status, timestamps), never the
credential or ciphertext. No public read-secret or export operation exists.
Only server delivery logic reads and decrypts the latest active credential,
immediately before an attempt. Update, disable, re-enable, rotation, under-
privileged denial, cross-tenant denial, and transactional audit remain correct.

**RED.** Bring forward the source SQL, handler, and real-server client journeys;
assert create/replace succeeds on each public surface, list/get returns only
metadata, no surface can retrieve a credential, and server delivery uses the
latest value. Run the focused relevant cases on the integrated tree and
identify missing or regressed behavior. Add only a regression needed for a new
merge seam.

**GREEN.** Resolve contract, SQL, boot, client, and generated-artifact seams
without changing the approved public shapes or secret handling. Rerun earlier
scenarios after each affected owner is reconciled.

**REFACTOR.** Reuse current typed route, audit, tenant connection, and shared
client patterns; remove stale source-side duplicates rather than adding
compatibility paths.

### Scenario 3 — Failed results produce bounded independent deliveries

**Behavior.** Only a failed, binding-created verification result atomically
creates one dispatch per effective Operator. Independent leased attempts use
PostgreSQL time for availability, fencing, retries, and deadlines. Registration
and each attempt enforce exact connection/provider/HTTP authority; user URLs
are screened and pinned before credentials attach. Provider results, retry
bounds, shutdown, restart, and status stay truthful without altering the
Verifier result.

**RED.** Bring forward source dispatch, registration, provider, and real-server
journeys, including negative and replay cases. Run them against the integrated
current runtime and capture failures at changed Task 004/010 seams.

**GREEN.** Adapt the source delivery worker and SQL to the current settlement,
clock, supervision, and registration owners, preserving current behavior for
non-Operator verification paths. Rerun connection and Vault proofs.

**REFACTOR.** Keep the durable dispatch as the handoff and reuse the current
security and provider patterns; remove stale alternate timing or delivery
paths from the source branch.

## Acceptance Criteria

- Both Gateway and Operator delivery import the Gateway-owned Vault KV v2
  reader from `crates/shared/wyrd-vault`. Gateway returns provider credentials;
  Operator validates tenant/version KEKs. Neither caller has a private Vault
  HTTP reader or can use the other's value as its own.
- Multi-tenant production uses Vault and fails startup without its active
  tenant keys. Env keys remain development-only, restrictive files remain
  explicitly single-tenant, and rotation preserves decryptability until old
  key versions are unreferenced.
- HTTP, SDKs, CLI, and MCP can create or replace Operator credentials and can
  list/get nonsecret connection metadata. None can read or export a credential
  or ciphertext. Only server delivery code reads and decrypts it for an
  attempt; tenant/permission denials and rotation journeys pass.
- Failed-only dispatch, independent bounded delivery, audit, SSRF pinning,
  retries, and lifecycle behavior pass against the updated Tasks 001–006/010
  tree. Gateway journeys and existing verification journeys do not regress.
- The final diff retains no obsolete specification or architecture text,
  duplicate Vault HTTP reader, generated-artifact hand edits, unrelated source
  changes, or claim that the source R6 verdict covers the integration.

## Expected Write Set and Consumer Closure

Likely owners are the source Task 007 connection/dispatch migration and queries,
`wyrd-crypt`, server Operator/verification/boot/config/routes, the extracted
`wyrd-vault` reader and Gateway import, `wyrd-spec` contracts, shared client,
Rust/Python/TS SDKs,
CLI, MCP, architecture documentation, generated schemas/stubs, and real-server
journeys. This inventory is guidance, not a file allowlist. Inspect all 171
source-changed paths and every target-side overlap, including automatic merges.

## Verification and Evidence

Run newly introduced or modified named tests by exact `mise exec -- cargo
nextest run --locked -p <package> --lib|--test <target> -E 'test(=<name>)'`
selectors during Red-Green iteration, confirmed from the merged source. Use
repository-managed setup for Postgres and Vault tests; do not invent selectors
or count a zero-test pass. Final non-credentialed verification is:

```bash
mise run gate
git diff --check
```

The broad gate is required because this replay crosses contracts, SQL,
security, server runtime, Gateway, all client languages, CLI, MCP, and shared
build artifacts without one complete Operator capability gate. Confirm the
merged Operator and Gateway journeys are selected by that gate; if required
proof is outside it, record and run only that explicit exception. Use local
provider/Vault mocks and real-server journeys without production credentials.
The credentialed Slack/PagerDuty smoke remains gated release evidence; prior
smoke is not proof of the integrated candidate. Record commands and results,
then request a fresh immutable review of the committed integrated tree.

## Material Stop Conditions

Stop the affected seam if revision 45 remains unapproved, an architecture
authority contradicts its approved Operator contract, or satisfying the merge
requires a new provider, public/persisted shape, key source, crypto dependency,
tenant/audit/SSRF policy, retry ceiling, or weaker proof. Report the precise
conflict and required authority. A merge conflict, missing local test service,
or repairable red gate alone is not a material stop condition.

## Authority Links

- `changes/active/verified-change-contract/spec.md` (revision 45 when approved)
- `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md` (completed source task)
- `changes/active/verified-change-contract/review/TASK-007-r6/verdict.md` on `vcc/task-007` (historical source verdict)
- `architecture/wyrd-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/agent-rules.md`
- `AGENTS.md`

## Implementation Evidence

Status: `IMPLEMENTED` — routed to a fresh `$wyrd-task-review`. This record does
not approve the task. Source R6 PASS covered `cd002ab13` under revision 36 and
does not cover this integration.

Heads: target before merge `fd886e7f8` (revision 45 approved); source
`vcc/task-007` tip `c6301fd83`; merge `ee93903ff` (`git merge --no-ff
--no-commit`, conflicts resolved per file, no branch-wide `ours`/`theirs`).
Integrated candidate: the commit that records this evidence.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Gateway and Operator delivery import one Vault KV v2 reader from `crates/shared/wyrd-vault`; no private reader in either; values stay distinct | `crates/shared/wyrd-vault/src/lib.rs` (`VaultKv2::read_field`, screened/pinned resolver, bounds); `wyrd-gateway/src/vault.rs` (`VaultBackend { reader: VaultKv2 }`, provider credential mapping); `wyrd-server/src/components/operators/keys.rs` (`vault: Option<VaultKv2>`, 32-byte KEK validation, tenant/version path); Task 007 reqwest Vault reader removed | `wyrd-vault` lib tests (3), `wyrd-gateway vault::tests` (2), `wyrd-server components::operators::keys::tests` (6), `test:gateway:vault` journey — all in `mise run gate` | PASS |
| Multi-tenant production uses Vault and fails startup without active tenant keys; env dev-only; restrictive files single-tenant; rotation keeps old versions decryptable | `wyrd-server/src/config.rs` `OperatorKeysConfig::validate`; `boot/mod.rs` `verify_operator_keys` + `ServerBootError::OperatorKeys`; rewrap in `verification/operators.rs` | `config::tests::operator_key_source_follows_deployment`, `production_vault_requires_https`; `pg_operator_delivery` boot-gate, rotation, and rewrap journeys (wyrd family lane) | PASS |
| HTTP, SDKs, CLI, MCP create/replace credentials and list/get redacted metadata only; no read/export; tenant and permission denials; rotation | `wyrd-spec/src/operator_connection.rs`; `components/operators/routes.rs`; `wyrd-client/src/operator_connections.rs`; Rust/Python/TS SDK projections; CLI `operator-connection`; MCP `operators.*` (catalog reconciled with Gateway tools, 31 tools) | `pg_operator_connection_routes` (5), `wyrd-sql::pg_operator_connections` (2), MCP connectivity/discovery tests, Rust `sdks/wyrd-sdk-rust/tests/operator_connections.rs`, `py:test:integration` (`test_operator_connections_journey.py`), `ts:test:integration` (`operator-connections.test.ts`), `pg_openapi_contract` | PASS |
| Failed-only dispatch, bounded independent delivery, audit, SSRF pinning, retries, lifecycle pass on the current tree; Gateway and verification journeys do not regress | `verification/claims.rs` shared `ClaimLoop` adopted by Runner and OperatorWorker; `verification/mod.rs` composes Scheduler, Fitter, OperatorWorker, Runner; `FeatureDriftReport.evidence` seam | `pg_operator_delivery` (8 + 2 ignored live smokes), `pg_verification_runtime` (24), `wyrd-testing --test server` Eval journeys, `test:gateway:gate`, `test:bifrost:gate` | PASS |
| Analytical follower grants never outlive their leader stream; held grants are visible; an abandoned plan returns the follower to baseline | `dispatcher.rs` `HeldGraphGrant`/`ParticipantGrant`, `peer_service.rs` streaming `ReserveSlots`, `analytical.rs` `AnalyticalParticipantGrants` and `GraphLease.grant_closed`, `OracleRuntimeInspection.held_grants`; `PENDING_TTL`, `ReleaseSlots`, retained release removed; `STAGE_PROTOCOL_VERSION` 2 (`3322efcaf`) | `vala-bifrost-redux` oracle unit tests; `peer_network::security::peer_context_refusals` (abandoned-grant journey); `capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants`; `test:bifrost:gate` in `mise run gate` | PASS |
| No obsolete spec/architecture text, duplicate reader, generated hand edits, unrelated changes, or R6 coverage claim | `architecture/wyrd-design.md` Operator connections paragraph names `wyrd-vault` and drops the deferred `SecretRef` resolver; schemas/stubs/napi declarations regenerated (`codegen:regen`, `ts:build`) | `codegen:check`, `ts:napi:check`, `docs:check`, `check:*` in gate; `git diff --check` | PASS |

### Integration seams found and fixed

- **Migration version collision:** source `20260601000032_operator_connections.sql`
  duplicated the target's drift-baselines version; renumbered to
  `20261001000000_operator_connections.sql` (`c8d830fa9`).
- **Eval journey Operator shape:** target-side fixture used the pre-connection
  Slack shape that Task 007 made invalid; the journey only counts dispatches,
  so it now uses an unauthenticated HTTP Operator (`d79a58a5e`).
  Independent diagnosis confirmed no other stale Operator fixtures.
- **Production config fixtures:** target-side peer-pod test failed the
  approved Vault key-source rule; three calibration rejection tests were
  passing on that rule instead of their own condition. All four now carry
  Vault keys (`3b767972b`).
- **Mock scope:** relocated Vault tests and Task 007's test-only wiremock
  seams added through the check's allowlist (`ecb2b8d49`).
- **Migration lease stranded reconnect (latent, exposed):**
  - **Symptom:** nine `pg_router_smoke` SIGABRTs plus a ~61 s cluster-wide stall of wyrd-sql fixture tests in `test:wyrd`.
  - **Evidence:** Postgres log `still waiting for backend with PID … to accept ProcSignalBarrier` on every `DROP DATABASE … WITH (FORCE)` until `canceling authentication due to timeout`.
  - **Cause:** `OperatorPool::migration_lease` detached a pooled connection, so sqlx began a background reconnect; a fixture drop blocking the current-thread runtime stranded it mid-login.
  - **Fix site:** the lease now opens its own `PgConnection` from the pool's options (`fdfaa94ca`). Every lease caller routes through it.
  - **Note:** an earlier field-order hypothesis (`166f85f5d`) was unproven and reverted (`32702dc5a`).
- **Keycloak host port:** the identity journey lane failed because another
  worktree's server held host port 8080. The test Keycloak first moved to
  `127.0.0.1:18080` (`b24505671`), which then took a reserved `WyrdTestServer`
  port mid-gate: the harness reserves from `[ip_local_port_range.low/2, low)`
  and 18080 is inside it. It now publishes on `127.0.0.1:8180` (`3dc98bfa7`).
  Dex is unchanged.
- **Analytical follower grants outlived their leaders (latent, exposed):**
  - **Symptom:** `capacity::lowest_rung_analytical_contention_preserves_two_interactive_tenants` found a follower above its ownership baseline (an extra spill directory and envelope) after its queries ended.
  - **Evidence:** the Analytical leader reserved each participant before dispatch. The follower answered by creating a pending entry holding a query envelope, runtime, and spill directory, and freed it only on lazy `PENDING_TTL` expiry or an explicit `ReleaseSlots`. A leader that abandoned its plan, or whose release was lost, left the entry charged. The test counted no pending entries, so this was consistent with the symptom but not yet proven.
  - **Cause:** follower capacity was owned by a timer-bounded pending entry instead of by the leader. Nothing tied a grant's lifetime to its leader.
  - **Fix site:** `ReserveSlots` is now a server stream that holds a `HeldGraphGrant` (`dispatcher.rs`, `peer_service.rs`). The grant ends when the stream closes, at the query deadline, or at shutdown (`ReservationRegistry::close_all`), and it cancels the graph built on it (`GraphLease.grant_closed`). Callers checked:
    - the leader lifecycle (`AnalyticalParticipantGrants`, where dropping releases),
    - the Scribe executor (never granted),
    - shutdown,
    - follower activation (single-use envelope; rollback never restores a closed grant).
    `PENDING_TTL`, `ReleaseSlots`, and retained-release expiry are deleted. `STAGE_PROTOCOL_VERSION` 1→2 refuses mixed peers.
  - **Proof:** held grants appear in `OracleRuntimeInspection` and the ownership snapshot. `peer_network::security::peer_context_refusals` now includes the abandoned-grant journey (admit, drop, follower back to baseline including spill directories within 5 s against a 30 s deadline). The capacity journey passes.
- **Generated docs drift:** the schema inventory and `llms-full.txt` were
  regenerated for Operator connections and Gateway (`f2cb96efd`).

### Non-goals

No new provider, cloud SDK, cipher, broker, credential cache, Alert resource,
executable Workflow action, plaintext export, compatibility route, generic
secret resolver, or `SecretRef` Operator-key contract was added. Source
historical review files remain prior evidence only.

### Commands

```bash
mise run gate          # PASS (final run on 3dc98bfa7, 2093 s)
git diff --check       # clean
mise run test:wyrd     # 2292 passed after the lease fix
mise run test:sql      # 179 + 6 + 114 + 2 passed after the renumber
```

Credentialed Slack/PagerDuty live smokes remain gated release evidence and
were not run.
