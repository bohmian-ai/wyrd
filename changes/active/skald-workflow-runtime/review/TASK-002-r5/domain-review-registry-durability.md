# Registry durability and concurrency domain review

## Result and immutable subject

**PASS.** The independently reviewed registry/persistent-state boundary has no
material proposed finding.

Reviewed cumulative base `0569b79702218600c4f9790f45cc03100d5c6f1c` to
candidate `2d669917c03699876b3c8926f0de5ac88c578c01`. `HEAD` resolved to the
candidate before and after source inspection. The subject includes approved
Revision 12, `TASK-002-cleanup`, and the R2, R3, and R4 remediation tasks. The
repository has no `.codegraph/` directory, so navigation used repository source
search and direct source/diff reads.

## Boundary, authority, and source coverage

This review traced composite Workflow registration from authored submissions
through graph planning, effective-body preflight, tenant reads, write-time UID
and lifecycle locking, binding, version allocation, audited persistence,
relationship writes, replay, commit, and post-commit recovery. It also traced
registered loading back through the stored exact relationships and inspected
the real Postgres and public-SDK journey assertions. Prior verdicts and recorded
green commands were treated as hypotheses and evidence claims, not source
proof.

Governing authority was AGENTS.md transaction, server-contract, test, and
completion rules; `architecture/agent-rules.md` tenant-connection, transaction
ownership, RLS, audit, and effective-value rules;
`architecture/wyrd-design.md` composite-registration, version-intent,
idempotency, Card-blob, and relationship contracts;
`architecture/wyrd-security-posture.md` tenant and transactional-audit rules;
Revision 12 REQ-014, REQ-028, REQ-056 through REQ-059, INV-003/005/007, and
AC-002/006/029/030; and the remediation requirements preserving provenance,
exact UID fences, atomicity, and ordinary validated-newtype behavior.

| Reviewed boundary | Source and consumer evidence | Assessment |
|---|---|---|
| Authored submissions and version intent | `service.rs:565-597` hashes and validates the original request, while `plan_registration_graph` and `EffectiveSpecs::validate_workflows` use graph-only holders without mutating it. `RegistrationPlan` retains the authored submissions; `persist_node:1162-1229` takes the existing version-line lock before `resolve_existing:1375-1443` handles exact, scoped, or omitted intent. | PASS |
| Effective bodies and provenance | `resolve.rs:124-225,406-514` keeps sibling and external specs in separate stores, chooses by `Ref::Sibling` versus `Ref::Ref`, and loads an external body by the tenant-scoped preflight UID. Transitive registered Agent-to-Prompt dependencies are added to the same resolved-pair set before Skald validation. The collision cases in `pg_workflow_registration.rs` prove a submitted sibling cannot satisfy an external slot of the same identity. | PASS |
| Preflight body to write-time UID continuity | `resolve_external` returns `(CardRef, CardUid)` pairs unchanged into `RegistrationPlan`. `RegistrationWriter::write:1073-1147` supplies those pairs to `recheck_active_card_refs` before idempotency reservation or Card mutation. `relationships.rs:36-68` requires exact identity, expected UID, and Active status under `FOR SHARE`; `bind_card_references` then binds that same UID into the spec used for hashing and relationships. | PASS |
| Dependency lifecycle concurrency | The exact target-row share locks remain owned by the writer's `TenantConn` through node, edge, binding, replay-seed, and audit writes until its single commit. `relationship_recheck_blocks_target_lifecycle_race` demonstrates a conflicting lifecycle update waits, and `refuses_stale_preflight_after_dependency_replacement` establishes replacement between transactions and proves stale preflight refusal. | PASS |
| Atomic persistence and audit | `RegistrationWriter::write` appends allowed decisions, rechecks dependencies, reserves the operation, persists topo-ordered Cards and relationships, stores the replay seed, and commits once. `persist_node` borrows the same transaction for the Card row, relationships, artifact manifest, Card principal, frozen bindings, and Drift queue state. Any error or cancellation before commit rolls the entire write back. A validation/replay/lost-race path that commits no registration records the already-made allowed verdict through the established standalone canonical path. | PASS |
| Relationship derivation and exact stored graph | `persist_node:1168-1207` derives outbound refs from the already-bound spec and persists both spec and exact UID-bearing relationship rows in the same transaction. The server integration checks Workflow-to-Agent and Agent-to-Prompt refs/edges, while all three SDK journeys inspect the exact graph and prove v2 registrations do not float a loaded v1 dependency. | PASS |
| Idempotency, replay, and recovery | Operation lookup remains principal/key/request-hash scoped. A losing reservation drops its uncommitted transaction before bounded winner replay. Transient validation holders do not feed request hashing, replay, or version allocation. Artifact initialization remains post-commit over the existing pending-card/replay-seed saga and reconciliation owner; this task adds no second durable queue, WAL, lock service, or registry. | PASS |
| R4 exact-version correction | `VersionBlock` now deserializes through its existing exact-semver parser, preventing a range or partial from inhabiting the UID/relationship identity type. The server test replaces its manufactured invalid `CardRef` path with an owning-type invariant assertion; server input deserialization still supplies defense in depth. This strengthens exact reference state and does not alter registration SQL, transaction ordering, lock ownership, or relationship persistence. | PASS |

## Failure, concurrency, and recovery assessment

Malformed composition, missing dependencies, provenance collisions, and
resolved Workflow validation failures occur before the writer opens its
mutation transaction and therefore create no registration operation, Card, or
relationship. A target deleted or replaced after preflight fails the
identity-plus-expected-UID predicate; a lifecycle change after the recheck
waits behind the share lock. Audit failure, dependency recheck failure,
idempotency conflict, version/spec conflict, node/edge/binding failure, or
cancellation before commit leaves no partial write. Once SQL commits,
post-commit upload work remains an explicit recoverable saga rather than
pretending the object store shares the database transaction.

No candidate mechanism in this boundary is project-local novelty. The
transaction, expected-row predicate, PostgreSQL row lock, unique idempotency
reservation, validated newtype, and post-commit reconciliation patterns are
both established Wyrd owners and ordinary production database practices.
`EffectiveSpecs` and `RegistrationWriter` consolidate required state under the
repository's struct-centered rule; they do not introduce a parallel registry,
cache, parser, validator framework, setting, option, check, or file format.
Under the human standing direction, there is therefore no DRIFT finding.

## Prior-finding closure

- **FIND-TASK-002-1** remains closed at the producer: sibling and registered
  bodies cannot cross-satisfy Workflow, binding, baseline, or related
  consumers.
- **FIND-TASK-002-4** remains closed: the exact preflight UID is carried to the
  Active-row recheck and locked through the durable commit.
- **FIND-TASK-002-9** remains closed for this slice: `EffectiveSpecs` owns
  preflight and `RegistrationWriter` owns the dependency-backed write workflow
  without duplicating the registry.
- **FIND-TASK-002-10** remains closed: graph-ready holders do not change
  request hashing, replay identity, or server version allocation.
- **FIND-TASK-002-7** remains closed for durable graph proof: server and three
  language journeys compare exact stored refs and relationship targets and
  retain after-v2 pinning.
- R4's closure of **FIND-TASK-002-11** prevents an invalid exact-version value
  from reaching registry IO; it neither weakens nor replaces the exact-UID
  write fence.

## Verification limits

This was a static review. I did not run builds, tests, or a Postgres lane. I
inspected the complete cumulative diff, current symbol bodies, callers, SQL,
and relevant test assertions. The task and R4 implementation evidence report
PASS for the focused Workflow registration validation test, stale-preflight
replacement test, SQL lifecycle-lock test, all three SDK journeys, shared and
SDK lanes, `check:registry-tx-coupling`, codegen/boundary checks, formatting,
lints, and cumulative diff hygiene. Those are reported executions rather than
independently rerun evidence; the reviewed source and assertions are consistent
with the claims.

Proposed finding ledger: **empty**. Overall domain result: **PASS**.
