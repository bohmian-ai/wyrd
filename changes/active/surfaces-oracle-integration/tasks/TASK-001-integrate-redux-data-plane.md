---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-surfaces-oracle-integration
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-026, REQ-026A, REQ-026B, REQ-027, REQ-027A, REQ-028, REQ-029, REQ-030, REQ-030A, REQ-048, REQ-049, REQ-050, REQ-051, REQ-052, REQ-053, REQ-053A, REQ-054, REQ-062, REQ-063, INV-002, INV-003, INV-007, INV-008, INV-008A, INV-008B, INV-009, INV-017, INV-018, INV-019, INV-020, INV-021, INV-023, AC-005, AC-006, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-020]
depends_on: []
parent_task:
remediates: []
---

## Objective

Integrate the immutable Oracle checkpoint as the authoritative Bifrost data
plane while preserving the destination's non-Bifrost behavior. The completed
outcome has one Redux engine owning Gate, Scribe, Oracle, incorporated Forge,
canonical telemetry, query execution, maintenance, recovery, and retained
audit publication; legacy `vala-bifrost` is gone in full.

## Constraints

- Use the completed revision-4 conflict ledger and only the pinned Oracle
  input. Forge is already incorporated and must not be merged again.
- Preserve Surfaces authority outside Bifrost. A newly discovered material
  conflict returns `SPEC_REVISION_REQUIRED`; a clean Git merge is not proof.
- Keep all listeners, authentication, authorization, readiness, and durable
  orchestration server-owned. Preserve tenant isolation and the exact audit,
  reader-protection, WAL, resource, cancellation, and fail-closed boundaries.
- Complete audit publication directly against Redux. Do not port legacy
  sealing, derivation, typed-read, or direct-Iceberg relay machinery.
- Do not add compatibility crates, routes, aliases, a second scheduler,
  cluster-wide Oracle quotas, or alternate durable formats.
- Live UI integration, SDK package convergence, repository-wide CI closeout,
  and the single data-root follow-up are owned by later tasks.

## Relevant Surface

- `crates/vala/vala-bifrost-redux` and deletion of
  `crates/vala/vala-bifrost`
- Bifrost-owned contracts in `crates/wyrd-spec`
- `crates/vala/vala-sql`, `crates/wyrd/wyrd-sql`, and their greenfield
  migration sources where data-plane authority requires them
- Bifrost integration in `crates/wyrd/wyrd-server`
- Bifrost capability journeys in `crates/wyrd/wyrd-testing`
- Bifrost, security, reliability, and operations architecture authorities

Paths are ownership guidance, not a private implementation allowlist.

## Approach

1. Reconcile the pinned Oracle Redux tree and its server/SQL contracts against
   the ledger, retaining Surfaces behavior outside Bifrost.
2. Remove the legacy engine and redirect every live server, audit, test,
   manifest, feature, generated, and documentation consumer to Redux or its
   approved server owner.
3. Preserve Scribe WAL v6, admission, staging, publication, replay, shutdown,
   and retirement behavior as one bounded durability lifecycle.
4. Preserve Oracle's one-build planning, pod-local admission, scoped
   authorization, reader epochs/protection, one-attempt execution, terminal
   streaming, cancellation, and readiness behavior.
5. Preserve incorporated Forge scheduling, independent maintenance,
   publication, conflict/ambiguous recovery, protection serialization, and
   fail-closed readiness behavior.
6. Converge OTLP and canonical Arrow writes on the three canonical signal
   tables with trusted attribution and SQL-only reads.
7. Complete bounded, idempotent outbox publication into
   `vala.system.audit_log`, guarded retirement, and crash recovery while
   retaining transactional audit and Oracle's WAL-first read exception.

## Acceptance Criteria

- The tree and dependency graph contain exactly one Bifrost engine, Redux;
  no legacy package, symbol, feature, route, schema, migration owner, test,
  benchmark, documentation alias, or compatibility facade remains.
- Acknowledged Scribe writes survive replay and progress through staging and
  publication without weakening fences, idempotency, bounded ownership, or
  tenant-qualified physical identity.
- Interactive and distributed queries use one physical build, local fair
  admission, complete object authorization, durable reader protection before
  source IO, one deadline, and one selected execution attempt. Failure never
  becomes partial success or a successor attempt.
- Reader protection and Forge expiration serialize per tenant-qualified table;
  lease loss, uncertain authority, and unresolved maintenance remove readiness
  and fail closed.
- Forge retains independently committed sibling progress, bounded conflict
  retry, ambiguous-outcome reconciliation, worker-local FIFO estimated-memory
  admission, and separate cleanup protocols.
- Stock OTLP and canonical Arrow writes produce equivalent rows in only
  `vala.traces.spans`, `vala.logs.records`, and `vala.metrics.points`, with
  trusted principal/correlation attribution and exact partial-success rules.
- Same-transaction mutations and WAL-accepted reads reach the canonical tenant
  outbox. Publication into `vala.system.audit_log` is bounded and idempotent;
  rows retire only after durable publication and recover safely from ambiguous
  publication or retirement.
- Scoped-role, cross-tenant, audit-unavailable, replay, backpressure,
  cancellation, peer-failure, restart, and cleanup journeys fail or recover
  exactly as revision 5 requires.

## Verification

- `mise run fmt`
- `mise run lints`
- `mise run test:sql`
- `mise run check:tenant-isolation`
- `mise run check:object-store-pin`
- `mise run check:unwrap-audit`
- `git diff --check`

Run and record exact focused `mise exec -- cargo nextest run` commands for the
Redux, Scribe, Oracle, Forge, OTLP, server, and audit scenarios changed during
implementation. Record the no-legacy inventory and requirement-to-evidence
closure. Do not run a Bifrost aggregate in this task.
