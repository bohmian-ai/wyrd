---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-stored-secret-encryption
spec_revision: 1
requirements: [REQ-002, REQ-004, REQ-005, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012]
depends_on: [TASK-002]
---

# Deliver deployment wrapping-key rotation and recovery

## Outcome and flow

A deployment administrator configures a permitted replacement wrapping key,
rolls out the approved active/retained configuration, and starts maintenance.
Existing owner rows → common facade rewrap or fresh-DEK recovery → fenced
owner updates → existing runtime refresh and per-pass reporting. Stored secrets
remain attached to their resource identities, and interrupted work resumes.
The workflow makes safe key retention and deployment-wide disable effects clear.

Use the [approved spec](../spec.md), design
[What rotation versions mean and what Wyrd loads](../design/notes.md#what-rotation-versions-mean-and-what-wyrd-loads),
[Compromise and revocation boundary](../design/notes.md#compromise-and-revocation-boundary),
and [Deployment wrapping-key operation](../design/admin-journey.md#deployment-wrapping-key-operation).
Task 2 supplies every owner row/consumer using the common facade and config.
Task 3 is not a dependency: this operates on the deployment, not a selected
tenant resource, and does not need that task's new action implementation.

## Existing owner and scope

Consolidate the complete workflow of auth `SealedSecretRewrap` and Operator
`OperatorKeys` maintenance/verification runner: discover affected rows/tenants,
perform bounded cancellable work, compare-and-swap, report failures/counts,
retry/resume, and trigger existing runtime preparation/invalidation. Reuse the
existing SQL swap/version fences and named platform cross-tenant capabilities.
Task 2 has replaced crypto/key retrieval; remove duplicate rotation orchestration
here without adding a second loop around its leaf helpers.

Extend existing platform administration for start/progress and the approved
disable workflow, under platform authority. Carry the affected platform client
projections, typed contracts/errors, permissions, exports/stubs and generated
outputs with those actions. Never make a tenant writer a cloud key administrator.
Replacement provisioning remains deployment setup; activating a configured
replacement respects controlled rollout and role configuration authority.
Do not add a job/progress table, recovery service, global completion barrier,
new cache TTL, or arbitrary key/endpoint selector.

Update operator rotation/recovery documentation and runbooks with exact usable
backend instructions, including what is a provider version versus a new logical
key, what Wyrd loads, how access is retained, and how to read pass counts.
Describe identity-access remediation and backup retention separately from
rewrapping; do not claim provider tokens are revoked by key maintenance.

## Ordered acceptance scenarios and evidence

Record actual assertions, focused commands, RED/GREEN and limitations here.

1. **Rewrap one existing owner value** — REQ-004, REQ-010–011.
   Given a Task 2-created credential under an old allowed key and a configured
   replacement, start routine maintenance through the platform-authorized owner.
   Its row changes to the replacement reference/wrapped DEK while plaintext,
   DEK, payload ciphertext and resource identity are retained; actual downstream
   use succeeds. Begin with real-server/Postgres integration for the smallest
   owner, consuming the facade from Task 1. RED is absent common maintenance,
   not an isolated direct call to the facade's already-proven rewrap.
2. **Complete a whole-store routine pass** — REQ-005, REQ-010–012.
   Create values for each existing owner across tenants and platform scope,
   roll writers to the permitted active replacement retaining old reads, and
   start maintenance. Verify actual continued use and accurate per-pass current/
   changed/remaining observations and failed-owner diagnostics. Exercise GCP/
   Transit same-name versions and AWS opaque historical material at their adapter
   boundaries without inventing portable material-version IDs; separately prove
   logical-key replacement updates persisted references. Server/SQL integration
   plus backend tests assert the complete consuming transition. RED is uncovered
   owners or duplicate/incorrect maintenance, not missing new per-tenant keys.
3. **Recover from compromised wrapping material** — REQ-002 as exercised by
   REQ-010, REQ-011. Given affected values and a permitted safe replacement,
   invoke compromise recovery rather than routine rewrap. Assert fresh DEKs and
   payload ciphertext across owners, stable resource identities/values, and
   successful actual use. Demonstrate that routine rewrap alone retains the DEK
   and therefore is not the recovery mode. Integration covers resource updates;
   facade tests alone cannot prove the deployment action selected the right mode.
   RED is absent recovery mode or use of rewrap where fresh protection is required.
4. **Resume safely across interruption and races** — REQ-008–010.
   Interrupt a bounded pass, restart it from stored state, deny one required key,
   and race a credential replacement/revocation with a prepared update. Observe
   resumable remaining work, redacted failures, preserved concurrent changes and
   authority, no partial envelope, and existing cache refresh. Real Postgres/
   server integration uses deterministic interruption/race controls. Counts are
   pass observations, not a global acknowledgement or proof that backups are safe.
   RED is unsafe/missing consolidated resumption or fencing; retain already-correct
   maintenance cases as regression evidence without rewriting their semantics.
5. **Retain recovery access and contain deliberately** — REQ-005, REQ-009,
   REQ-011–012. Exercise live old-key reads during rollout and isolated restore
   with retained key access, then loss/denial of a required key. Use fails closed
   without generating substitutes. A Wyrd-initiated shared-key disable is platform
   authorized, clearly deployment-wide, and triggers existing invalidation;
   direct provider disable refuses subsequent provider calls but does not notify
   Wyrd or erase prepared keys. Verify those runtime boundaries without promising
   cancellation of admitted work. Review the retirement/runbook dependency proof
   for live/cached/backup references; ordinary disable must not destroy recovery
   keys. Integration and runbook/source proof cover their respective effects.

## Verification and handoff

Use `wyrd-auth`/server maintenance PG owners, SQL swap integration, platform
administration real-server journeys, and the journey targets for affected
platform client projections. Start with focused tests under the actual package's
existing nextest runner and repository Postgres setup; record actual selectors
before RED.
Existing environment-owning lanes include `mise run test:platform:journey`,
`mise run test:operators:integration`, and `mise run test:sql`; broader identity/
Gateway/language journeys run on the integrated candidate as needed to prove
continued consuming use. Backend boundaries reuse Task 1's real adapters and
test service setup; identify local protocol versus live-provider evidence.

Run affected owner/projection tests, codegen and boundary checks, Rust and changed
language format/lint/type/declaration checks, and `mise run docs:check` /
`mise run check:examples` where applicable. External IO belongs in maintenance
and preparation. If hot-path code changes, list every added/moved wait and obtain
the applicable clean full L1 VALID comparison, including Gateway qualification.

Integrated change review closes the four spec journeys across these four tasks,
with full required language/owner lanes and benchmarks, rather than treating
isolated provider tests as delivered proof. No cloud credentials, benchmark
results, runtime success, or independent security audit are claimed by this plan.
