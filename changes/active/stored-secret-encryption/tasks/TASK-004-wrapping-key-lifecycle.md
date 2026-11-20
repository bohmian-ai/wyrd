---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 2
requirements: [REQ-002, REQ-004, REQ-005, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012]
depends_on: [TASK-002]
---

# Deliver explicit deployment wrapping-key rotation and recovery

## Outcome and flow

A deployment administrator configures and rolls out a permitted replacement,
then explicitly runs planned rotation or compromise recovery through HTTP or
CLI. Activation alone leaves existing envelopes untouched. Existing owner rows
→ common facade rewrap or fresh-DEK renewal → fenced owner commits → existing
runtime refresh → per-pass counts and redacted failures. Repeating the operation
resumes unfinished work without changing resource identities or client references.

Use the [approved revision-2 spec](../spec.md) and finalized
[lifecycle design](../design/task-004-lifecycle.md), especially Recommended
contract and Connected recovery flow and limits. The spec defines behavior;
this task owns implementation scenarios and evidence.

Task 002 supplies common envelopes and trusted binding metadata in all five
current stores: human connections, workload issuers, platform OIDC, Gateway
managed credentials, and Operator connections. It also supplies consuming readers,
runtime preparation, configuration, and fenced update owners. Task 001 supplies
the facade and backend clients through that dependency. Task 003's resource
actions are independent; maintenance does not require them. The integrated
incident runbook may reference them for credential replacement.

## Existing owner and implementation scope

Extend the existing auth `SealedSecretRewrap` and Operator maintenance workflows
into one explicit platform-owned pass across their current stores. Reuse scans,
budgets, cancellation, CAS/version fences, reports, and runtime refresh. Do not
add parallel rotation orchestration around leaf calls. The facade's `rewrap`
and `renew` already provide the two crypto operations; `is_current` compares
logical references, not native provider material generations. No provider-admin
adapter operation, new authentication method, dependency, or key registry is
needed. Keep provision/activate/disable outside this platform maintenance action.

Remove automatic auth/Gateway rewrap from server boot and automatic Operator
rewrap scheduling beside delivery. Preserve boot's independent missing-KMS
refusal, delivery scheduling, runtime invalidation, and owner safety checks.
Carry affected production/test-server boot callers, maintenance callers, tests,
fixtures, configuration references, and docs together. Task 002's revision-1
evidence remains historical; this task owns expectations changed by revision 2.

Prepare remote crypto outside held database locks/transactions and commit using
observed-envelope and authority fences at each SQL owner. Existing maintenance
scanning/reporting is reusable, but unchanged automatic scheduling or a batch
transaction held across remote operations cannot deliver the approved flow.
No new migration, table, lease framework, global completion barrier, cache TTL,
or durable incident/mode lock is authorized.

Expose one typed platform-authorized maintenance action on HTTP and CLI, with
an explicit routine/recovery choice and destination taken from active trusted
configuration. Carry server contracts/errors, permission checks, asynchronous
permission auditing through the existing outbox, CLI transport and output, and
affected generated projections. Use existing platform authority; tenant write
permission is insufficient. Do not expose SDK or MCP methods, a disable endpoint,
an activation endpoint, a background job, or a status-polling API. The action
returns the bounded pass report without waiting for asynchronous audit custody.

Update setup/rotation/recovery docs and runbooks with the two complete admin
procedures, administrative mode-isolation preconditions, key retention/restore,
credential issuer remediation, and truthful pass-count interpretation. Reconcile
the owning security posture and operations guidance with REQ-010/011 where
needed; do not change unrelated authority. Provider-specific instructions explain
deployment setup/access denial, never different Wyrd semantics.

## Ordered acceptance scenarios and evidence

Add actual assertion locations, focused commands, observed RED failures, GREEN
results, and limitations to these entries. New test names/selectors remain
implementation-owned and must be recorded before RED.

1. **Activation does not move stored secrets** — REQ-004–005, REQ-010.
   Given real Task-002-created envelopes under an old permitted reference,
   configure a replacement as active, retain old reads, restart through the
   production/test-server boot path, and allow Operator delivery maintenance
   opportunities. Existing envelopes remain byte-identical, new writes use the
   replacement, and actual auth/Gateway/Operator consumption still succeeds.
   Removing automatic transitions must preserve boot refusal when stored
   secrets exist without required KMS configuration. Server/Postgres integration
   covers both startup callers and the Operator runner with deterministic
   synchronization. Expected RED is automatic envelope mutation, not failure of
   an isolated facade test.
2. **Explicit routine pass moves one existing value** — REQ-008–010.
   With an old envelope and configured active replacement, a platform-authorized
   administrator invokes routine maintenance through HTTP and the CLI. Assert
   the stored reference/wrapped DEK changes while DEK, payload, plaintext and
   resource identity remain unchanged; actual downstream use succeeds. The
   response returns pass counts and contains no secret material. Tenant-only or
   unauthenticated callers are refused without row mutation. Real-server/Postgres
   journeys exercise the smallest owner first and both shipped surfaces.
   Expected RED is missing explicit action/CLI consumption or platform authority.
3. **Whole-store pass covers every current owner** — REQ-005, REQ-010–012.
   Create common envelopes across multiple tenants and platform scope in all
   five stores, complete writer rollout, and repeat explicit bounded routine
   passes. Assert every eligible owner can transition, actual consuming use
   succeeds, and current/changed/remaining observations plus redacted failed-owner
   diagnostics match observed rows, including values left by the work bound.
   Use platform HTTP/CLI real-server journeys and SQL integration. Reuse backend
   adapter proof for native rotation and retained decryption; do not invent
   portable native-version inventory or retirement counts. Expected RED is
   omitted owners, incorrect bounded traversal/counts, or missing runtime refresh.
4. **Recovery renews exposed data keys** — REQ-002, REQ-004, REQ-010–011.
   Given envelopes across the same owners/scopes and independently provisioned
   safe material under a fresh logical destination, complete rollout and select
   recovery. Assert each older envelope receives a fresh DEK/nonce/payload
   ciphertext, keeps its value/resource identity, opens through its actual
   consuming owner, and refreshes prepared runtime material. Compare with
   scenario 2's retained DEK to prove mode selection reaches the facade correctly.
   Include an incident discovered after routine rewrap to an earlier destination:
   activate another fresh destination and prove those envelopes are renewed too.
   HTTP/CLI plus server/Postgres integration owns this proof. Expected RED is
   missing recovery selection, skipped affected owners, or using rewrap for
   recovery. Runbook/source review verifies fresh-destination and mode-isolation
   preconditions without inventing an incident lock.
5. **Repeat after interruption, denial, or concurrent change** — REQ-008–010.
   Interrupt recovery after some real commits; restart and repeat the same mode
   and destination. Already transitioned envelopes remain byte-identical; old
   ones are renewed. Deny an old key and observe redacted remaining/failure
   reports with committed state intact. Race prepared routine/recovery updates
   against replacement and revocation: stale writes lose their fences, preserve
   the winning envelope and authority, never re-enable a resource, and remain
   safely retryable where work is still required. Server/Postgres integration
   uses deterministic barriers and proves downstream use/refusal and existing
   cache behavior. Expected RED is unsafe resumption, false completion, lost
   changes, or bypassed authority; no global acknowledgement is expected.
6. **Contain and restore through deployment administration** — REQ-005,
   REQ-009, REQ-011–012.
   Given old-key dependencies, remove a reference from trusted configuration and
   replace affected processes; subsequent attempts to open it refuse without
   fallback. With retained key access, an isolated restore can use the recovered
   credentials. With access denied, renewal fails without mutation; documented
   recovery restores controlled read access or replaces the actual credential
   through its owner. Integration proves refusal/restore boundaries; source and
   runbook review prove the procedure accounts for live/prepared/backup
   dependencies and stops processes when immediate containment is required.
   External key-service denial alone must not be described as cache erasure.
   There is no Wyrd disable action or promise to cancel admitted work. Expected
   RED is fallback, substitute-key generation, broken restore, or documentation
   still directing administrators to the removed action.

## Verification and handoff

Use server maintenance/boot and SQL PG integration owners, existing platform
administration real-server journeys for HTTP, and CLI tests exercising the actual
command/transport/report path against `WyrdTestServer` and repository Postgres.
Add maintenance journey coverage to existing runner selection where needed so
these scenarios are selected, rather than creating unreferenced fixtures or
disconnected client/parser tests.

Existing environment-owning runners are `mise run test:platform:journey`,
`mise run test:operators:integration`, and `mise run test:sql`. The platform
runner currently selects server `platform_admin_e2e` in the `integration` target;
the Operator runner selects connection/delivery PG modules with `test-support`.
Record exact focused nextest commands for each new test before its RED run;
this plan names no existing individual tests. Use `mise run test:cli:journey`
for the CLI surface and relevant boot/module regression lanes. Backend boundaries
reuse Task 001/002's adapter and live-provider evidence, clearly distinguishing
local protocol tests from live runs.

Run affected owner/projection checks, `mise run fmt`, `mise run lints`,
`mise run codegen:check`, `mise run check:deps`, `mise run check:tenant-isolation`,
and `mise run docs:check`; use `mise run check:examples` if examples change.
No Python/TypeScript API expansion is assigned. External crypto IO belongs in
control/maintenance/preparation. If hot-path code changes, enumerate every
added/moved external wait and its sanctioned authority. Benchmarks require an
explicit maintainer request for the current change and are not assigned here.

Integrated change review closes the four spec journeys with affected
surface/owner lanes. This task's planning does not claim runtime success,
cloud experiments, benchmark results, or an independent security audit.
