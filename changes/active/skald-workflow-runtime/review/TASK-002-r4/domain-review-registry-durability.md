# Registry durability and concurrency domain review

## Result and immutable subject

**PASS.** The independently reviewed registry boundary has no material proposed
finding.

Reviewed cumulative base `0569b79702218600c4f9790f45cc03100d5c6f1c` to
candidate `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`. Candidate identity matched
live `HEAD` when this review began. The subject is Revision 12,
`TASK-002-cleanup`, R2 and R3 remediation, and the cumulative diff, including
`375d97e67f3affe0d5c59727ef3135b22a459140`. CodeGraph was unavailable because
the repository has no `.codegraph/` directory.

## Boundary, authority, and source coverage

This review traced authored submissions through graph planning, effective-body
resolution, server version allocation, the audited write transaction, UID and
lifecycle locking, relationship persistence, replay, post-commit publication,
and recovery. It inspected the public registration/load callers and the actual
server, SQL, Rust, Python, and TypeScript journey assertions. It did not treat
prior PASS reports or recorded green lanes as source proof.

Governing authority was AGENTS.md transaction, server-contract, testing, and
completion rules; `architecture/agent-rules.md` tenant-connection, transaction
ownership, audit, and effective-value rules; `architecture/wyrd-design.md`
composite registration, Card blob, idempotency, and version-intent contracts;
Revision 12 REQ-014, REQ-028, REQ-056 through REQ-059, INV-002/003/005, and
AC-002/006/029/030; and the task/remediation preservation of provenance, exact
UID fences, registration audit, and atomicity. The reference router,
spec-driven-development, maintainer-style, and architecture patterns were also
applied. No Bifrost or persistent Workflow-run boundary is changed here.

| Reviewed boundary | Source and consumer evidence | Assessment |
|---|---|---|
| Pure graph plan and authored version intent | `service.rs:1012-1037` applies the existing graph-ready projection only to graph construction. `resolve.rs:430-465` likewise validates transient holders, while `RegistrationPlan` retains original submissions. `persist_node` takes the existing version-line lock before `resolve_existing`, so omitted, scoped, and pinned intentions still reach the server allocator. The PG journey registers omitted and scoped Workflow versions, reloads the allocated pins, and proves invalid resolved bindings write nothing. | PASS |
| Effective bodies and provenance | `EffectiveSpecs::{resolve,new,load,body}` keeps submitted siblings and registered external bodies in distinct maps and dispatches by `Ref::Sibling` versus `Ref::Ref`. Transitive registered Agent-to-Prompt dependencies are loaded by their resolved tenant UID before Skald validation. The identity-collision journey proves a sibling body cannot satisfy an external slot. | PASS |
| Preflight body to write-time UID continuity | `resolve_external` returns `(CardRef, CardUid)` pairs; `RegistrationWriter::write` carries those pairs directly to `recheck_active_card_refs` before reservation or mutation. The SQL predicate requires exact identity, exact expected UID, and Active status. `bind_card_references` then writes that UID into the stored spec. The stale-preflight test replaces the validated UID between transactions and proves refusal with no operation, Workflow, or relationship write. | PASS |
| Dependency lifecycle concurrency | `relationships.rs:36-68` takes `FOR SHARE` locks on exact Active target rows. Those locks live on the writer's `TenantConn` through all node, relationship, binding, replay-seed, and audit writes until commit/rollback. The SQL race test demonstrates a conflicting lifecycle update receives lock timeout, then proceeds after registration commits; normal delete retains its inbound-reference guard. | PASS |
| Atomic registration, audit, and relationships | `RegistrationWriter::write:1073-1147` appends allowed audit decisions, rechecks dependencies, reserves idempotency, persists topo-ordered nodes and exact relationships, records the replay seed, and commits once. `persist_node:1162-1229` borrows that same transaction for Card, relationships, manifest, Card principal, bindings, and Drift queue state. Any error or cancellation before commit drops the transaction. | PASS |
| Idempotency, replay, and races | Existing operation lookup uses principal/key plus request hash; a losing insertion drops its uncommitted transaction before bounded winner replay. Authored submissions feed request hashing and replay, so transient graph pins do not redefine idempotency. No candidate change relocates this ownership or introduces parallel durable state. | PASS |
| Post-commit publication and recovery | Upload initialization remains strictly after the SQL commit. The committed replay seed and pending Card state remain the inputs to existing bounded initialization, cleanup, blob finalization, and reconciliation. Candidate changes do not claim object-store atomicity, add another WAL/registry, or move reconcile mechanics into Workflow validation. | PASS |
| Exact graph proof at consumers | `pg_workflow_registration` checks every stored Workflow-to-Agent and Agent-to-Prompt spec ref and relationship target, including version and UID. The R3 remediation additions repeat the same exact graph through public Rust, Python, and TypeScript journeys, then register v2 and prove registered loading remains pinned. | PASS |

## Failure and recovery paths

Malformed composition, missing dependencies, provenance collisions, and
resolved Workflow validation failures occur before the writer transaction and
persist no operation or Card. Audit append, dependency recheck, version/spec
conflict, node/relationship/binding failure, or cancellation before commit
rolls back the single tenant transaction. A stale or deactivated dependency is
refused at the exact-UID lock fence. Once SQL commits, storage initialization
is deliberately a recoverable saga over pending rows and the replay seed; the
existing reconciliation owner handles interruption without pretending the
object store participates in the SQL transaction.

No task-local durability mechanism is accepted merely because it exists. The
candidate's transaction, PostgreSQL row-lock, unique-idempotency, and
post-commit reconciliation patterns are established repository owners and
ordinary production database patterns. `EffectiveSpecs` and
`RegistrationWriter` consolidate already-required state into the repository's
mandated struct-centered owners; they create no second registry, lock service,
check, setting, file format, or configurable option. I found no mechanism in
this domain that lacks both repository and widely used project precedent, so
the human standing direction produces no DRIFT finding.

## Prior remediation closure

- **FIND-TASK-002-1:** remains closed at the producer: sibling and external
  bodies cannot cross-satisfy, including binding, baseline, and Workflow
  consumers.
- **FIND-TASK-002-4:** remains closed: the preflight UID is preserved to the
  write-time exact-UID/Active row lock and held through commit.
- **FIND-TASK-002-9 (registry slice):** remains closed: `EffectiveSpecs` owns
  preflight orchestration and `RegistrationWriter` owns the dependency-backed
  write workflow without duplicating the registry.
- **FIND-TASK-002-10:** remains closed: transient graph-ready holders do not
  alter original version intent, request hashing, replay, or allocation.
- **FIND-TASK-002-7 (durable graph slice):** the R3 additions now assert exact
  stored references and server-derived relationship targets for all three
  Agent/Prompt pairs through each public language journey. This does not
  substitute for the other reviewers' assessment of complete language proof.
- The human-removed **FIND-TASK-002-11** implementation commit is included in
  the cumulative review. It changes Workflow selector error projection, not
  registration durability, transactionality, locking, or relationship state.

## Evidence limits

This was static review only, as assigned: no build, test, database lane, or
implementation edit was run. Full cumulative diff, relevant symbol bodies,
callers, manifests, and test assertions were inspected. R3 records passing
focused registration, UID-race, lifecycle-lock, transaction-coupling, and SDK
journey commands, but those remain reported executions rather than
independently rerun evidence. Candidate commits after the prior review only
extend exact graph assertions and selector/error proof in this boundary; the
registration implementation itself is unchanged from the previously inspected
candidate.

Proposed finding ledger: **empty**. Overall domain result: **PASS**.
