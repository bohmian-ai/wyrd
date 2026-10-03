# TASK-002 r5 system-resilience review

## Subject and result

Reviewed the immutable cumulative range
`0569b79702218600c4f9790f45cc03100d5c6f1c` to
`2d669917c03699876b3c8926f0de5ac88c578c01` against approved Revision 12,
`TASK-002-cleanup`, and the R2, R3, and R4 remediation tasks. `HEAD` resolved
to the candidate before and after source inspection. The review covered the
actual client loading, graph hydration, local execution delegation, composite
registration, exact-UID write fence, foreign-runtime projections, and the R4
validated-selector correction. No implementation source was changed.

## Review findings

No material system-resilience finding. Failures remain scoped to the affected
load, local run, or registration request; no partial Workflow is published;
durable registration remains transactional and replayable; and the R4 changes
reject malformed values before IO without adding a worker, cache, retry loop,
health policy, queue, configuration option, compatibility path, or other
bespoke recovery mechanism.

## Deployed paths and affected capabilities

| Path | Process and dependency topology | State and availability boundary |
|---|---|---|
| Authored Workflow load | In the caller process, `wyrd_client::Workflow::from_path` moves synchronous loader and canonicalization work to Tokio's blocking pool, then composes loader bodies into the existing Skald Workflow (`crates/shared/wyrd-client/src/workflow.rs:27-73,99-120`). | A wholly local bundle needs no server or credential. Filesystem and composition state is call-local, loading writes nothing durable, and no provider or tool is invoked. |
| Authored external references | The same facade constructs `Cards` only after discovering an external ref, then `CardGraphHydrator` performs exact reads through the existing shared client transport (`workflow.rs:65-71`; `cards/hydrate/graph.rs:339-369`). | Registry availability affects only the requesting load. Fully local and already-hydrated Workflows remain independent of the registry. The candidate adds no independent retry or cache owner. |
| Registered Workflow load | `Cards::workflow().load` rejects a wrong-kind or versionless selector locally, then Runtime-scope graph hydration reads the exact Workflow/Agent/Prompt closure without artifact inventories (`workflow.rs:148-190`; `cards/hydrate/graph.rs:38-55,207-249,311-336`). | Traversal state belongs to the loading future and returned Workflow. An error returns no partial Workflow, starts no execution, and mutates no durable state. |
| Rust/Python/TypeScript execution | The shared facade delegates to the existing Skald executor (`workflow.rs:76-95`); Python uses the shared runtime bridge and TypeScript holds the same native facade. | Provider failure, deadlines, cancellation, retry, and task draining retain their existing Skald owner. TASK-002 creates no detached executor or durable local-run registry. Process loss loses the caller-local run only. |
| Composite registration | Authenticated server request -> pure graph plan -> tenant-scoped `EffectiveSpecs::resolve` preflight -> `RegistrationWriter::write` in one tenant transaction (`wyrd-server/src/components/cards/resolve.rs:133-190,406-465`; `service.rs:565-582,1046-1147`). | Registration validates declarative bodies only. It performs no provider/tool dispatch or secret resolution and creates no Workflow principal. Database, RLS, idempotency, and canonical audit owners are unchanged. |
| Registration concurrency and recovery | The writer rechecks every preflight `(CardRef, CardUid)` as the same Active row under `FOR SHARE`, then retains those locks through node, relationship, replay-seed, audit, and transaction commit (`wyrd-sql/src/queries/cards/relationships.rs:17-68`; `service.rs:1073-1147`). | Pre-commit error or cancellation rolls back uncommitted Cards, relationships, operation state, and transaction-local audit. After commit, replay and another replica use shared PostgreSQL state; no graph state is replica-local. |
| R4 selector correction | `VersionBlock` now deserializes through its existing exact-version parser (`wyrd-semver/src/block.rs:153-166`). The Node boundary parses raw selector strings with the existing domain constructors and reports the exact malformed field (`wyrd-sdk-ts/native/src/workflow.rs:53-106`). | Invalid ranges, partial versions, and malformed UID/space/name/version values fail before transport IO. This only narrows accepted in-memory state and changes no process, dependency, retry, persistence, or recovery topology. |

The cumulative candidate therefore adds no deployment role and no health
dependency. A failed client load or caller-local run does not affect
`wyrd-server`. A failed registration rolls back that tenant transaction rather
than terminating the shared server or disabling unrelated Cards, gateway,
Bifrost, or Vala capabilities.

## Failure, interruption, and recovery assessment

| Credible failure or interruption | What stops and what survives | Recovery and proof assessment |
|---|---|---|
| Authored file is missing, unreadable, malformed, or resolves to an invalid graph | That load returns a structured error before publishing a Workflow or dispatching a step. Private loader/body values are dropped and no durable state exists (`wyrd-client/src/workflow.rs:57-73,109-120`). | Correct the bundle and retry from the beginning. The focused shared-client test exercises the checked-in bundle and pre-dispatch refusals. |
| Loading future is dropped during filesystem work or the runtime shuts down | The standard `spawn_blocking` job may finish after the future is abandoned, but it has no durable side effect and no receiver to publish a partial result. Join failure maps to the Workflow error channel (`workflow.rs:39-64`). | A later load starts fresh. Tokio's normal blocking pool is the established/native mechanism; no custom cancellation pool or worker is required. |
| Registry read times out, is denied, or becomes unavailable during graph hydration | Only the affected load fails. The active async read and call-local traversal are dropped; prior successful reads may retain their normal server audit effects, but no graph escapes and no independent retry loop amplifies the outage (`cards/hydrate/graph.rs:138-149,207-249,311-369`). | Restore dependency/service access and retry the complete exact load. Already-loaded and wholly local Workflows remain usable. |
| A dependency is missing, inactive, cross-tenant, or does not match the exact UID | The load or registration preflight refuses before returning a runnable Workflow or writing Cards. Sibling bodies cannot satisfy external refs with the same identity. | Restore the exact dependency or submit a fresh request. The three SDK journeys cover denied, missing/retired, exact/UID, and pinning paths; server tests cover tenant and graph refusal boundaries. |
| Malformed Workflow selector, including a semver range | Rust deserialization or the TypeScript selector boundary refuses it locally with the Workflow error before any registry request (`wyrd-semver/src/block.rs:153-166`; `native/src/workflow.rs:79-106`). Other clients and server defenses remain available. | Correct the field and retry. The semver unit scenario covers exact/prerelease/build round trips and invalid/range refusal; the TypeScript journey uses a client without read authority to prove malformed fields do not cross the transport boundary (`workflow-loading.test.ts:186-214`). |
| Database or audit append fails, or the request is cancelled before registration commit | The request fails and dropping `TenantConn` rolls back its transaction and releases locks. The audit append precedes mutation in the same transaction, so a successful unaudited write is not produced (`service.rs:1073-1147`). | Retry with the stable idempotency key after dependency recovery. No process abort or global unhealthy state is introduced. |
| External dependency is replaced or retired after preflight | The write-time query requires the exact preflight UID, exact identity, and Active status while holding a share lock. A replacement at the same name/version is refused rather than silently rebound (`relationships.rs:17-68`). | A fresh request re-runs preflight against current state. The recorded stale-preflight and lifecycle-race tests directly exercise rollback and lock preservation. |
| Server process or replica exits during registration | PostgreSQL rolls back before commit. After commit, Card rows, relationships, audit staging, operation response seed, and existing post-commit recovery state are durable and visible to another replica. | Replay with the existing idempotency key. The candidate introduces no replica affinity or new process-local registration state. This is preservation of an existing recovery owner, not a claim of fresh crash qualification. |
| Provider error, timeout, explicit cancellation, or caller process loss after local execution starts | Existing Skald execution owns the run outcome and draining. TASK-002 loading does not spawn a second execution lifecycle. The shared server remains unaffected by caller-local loss. | Re-run from the immutable hydrated graph where appropriate. Durable server-run restart recovery belongs to later tasks and is not inferred from TASK-002. |

## Recovery and regression assessment

- R4 closes the invalid-state producer rather than adding a downstream guard:
  ordinary validated-newtype Serde ensures a range cannot inhabit
  `VersionBlock`, and the TypeScript foreign-runtime boundary uses existing
  domain constructors. This is standard Rust/Node boundary behavior and does
  not warrant a DRIFT finding under the standing human direction.
- The R4 addition of the already-workspace-managed `wyrd-semver` dependency to
  the TypeScript native crate does not create a service dependency or runtime
  failure domain; it is an in-process parser used before IO.
- The existing exact-preflight-UID fence, transaction/audit order, idempotent
  replay state, and post-commit recovery path remain unchanged by R4.
- The earlier `spawn_blocking` remediation uses Tokio's standard facility and
  adds no task-specific runtime, pool, setting, or health mechanism.
- No cumulative change converts a request error into a process exit or makes
  an integrity refusal take unrelated capabilities offline.

## Verification notes and limits

The R4 implementation record reports all five focused commands and the
smallest affected lanes passing at candidate `2d669917c03699876b3c8926f0de5ac88c578c01`:
the owning `VersionBlock` Serde unit scenario; Rust, Python, and TypeScript
Workflow journeys; the server registered-Workflow integration test;
`test:shared`; `test:wyrd-sdk`; TypeScript unit/integration/type/N-API checks;
codegen; client-tier boundaries; formatting; lints; and cumulative
`git diff --check`. This reviewer inspected the changed test assertions and
confirmed the cumulative diff has no whitespace error, but did not rerun the
environment-owning lanes.

No new crash, rolling-replacement, registry-outage, or host-load exercise was
run. Those are residual release-qualification limits, not completion gaps for
this read-only, request-scoped loader and the preserved canonical registration
transaction. Healthy-path SDK execution is not treated as proof of later
server-hosted Workflow lifecycle behavior.

## Overall result

**PASS.** The cumulative candidate keeps filesystem abandonment, malformed
selector refusal, registry outage, dependency replacement, cancellation,
database failure, transaction rollback, and replica restart effects within
their established owners and approved operation boundaries. No material
system-resilience remediation is required.
