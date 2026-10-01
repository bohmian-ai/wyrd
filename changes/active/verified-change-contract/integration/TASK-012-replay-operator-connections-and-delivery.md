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
