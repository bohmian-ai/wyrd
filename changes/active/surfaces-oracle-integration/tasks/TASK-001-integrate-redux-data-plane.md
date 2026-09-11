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
- There is no such thing as a pre-existing failure anymore. All failures must be explicitly handled within the current execution context.

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

## Status Amendment — in progress

### Implemented

**Redux is the sole engine.** `vala-bifrost` is gone in full: zero
`vala_bifrost` symbols, `crates/vala/` holds only `vala-bifrost-redux`, and the
remaining string matches live in `.dev/` history, `changes/` planning docs, and
`scripts/checks/client-tier.sh` (kept — the token still matches Redux by prefix
and guards a live dependency-direction boundary). Empty Oracle and server test
stubs byte-identical to the pinned input were deleted with their `mod` lines.

**Merge defects resolved against the current tree, not tolerated.**

- `expired_uploads_batch` reaped only `pending` rows on `expires_at`, so an
  orphaned `initiating` row was never swept. Adopted the pinned input's
  `init_grace` predicate plus its `WYRD_STORAGE_SWEEPER_INIT_GRACE_SECS` knob
  (default 30s, clamped 5–600).
- `check:tenant-isolation` kept the Surfaces rule body while both Oracle
  allowlists still passed a now-inert exemption flag (19 failures). Restored the
  guard with Oracle's explicit tenant-predicate rule in the exempt branch.
- `check:unwrap-audit` flagged 10 call sites of a harness helper named `expect`
  that returns `Result`. Renamed it `require`.
- Redux tier-2 `forge::orphan_cleanup` flake: the count helper included
  `forge.task.*` worker bookkeeping. It now returns the ordered operation list
  and excludes that prefix, so a failure names the transition.

**Audit publication was non-functional end to end; three production defects.**
The capability had no journey covering it, so the path was dead and green. A new
journey (`wyrd-testing/tests/bifrost/server/audit_publication.rs`) exercises it
and found:

1. `publish_audit_projection` could never append — the `audit_log` content
   column `principal_id` is a reserved Redux correlation name, so ingest refused
   every projection. Renamed to `audit_principal_id`, matching the sibling
   `audit_card_ref` precedent.
2. `AuditPublisher::sweep` read the tenant directory on the RLS application
   pool, which holds no `SELECT` on `platform.tenants`. Routed through the
   cross-tenant operator pool.
3. `list_active_tenant_ids` validated every directory row as a UUIDv7, and the
   directory always carries the nil-UUID system tenant, so the read failed for
   every tenant. The sentinel now maps to `DataTenantId::SYSTEM_OWNER`.

Either of (2) or (3) alone disabled publication for every tenant, silently: the
sweep logs `warn!` and returns.

**Consequences of publication actually running.** With audit rows reaching
Scribe for the first time, Forge refused to promote them: `audit_log` is
registered without the universal correlation columns that ingest stamps on every
batch, so the physical object could never agree with its table. Interim fix
(under review, see Remaining) set the table to `CorrelationPolicy::Observation`
and retired the unwritable `None` variant. Forge journeys that read
`vala.audit_outbox` as if it were history now read retained history as well; the
anti-enumeration leak check is scoped to the problem members that carry request
data; the process-wide Scribe row counter is held to a floor now that the same
Scribe also accepts published audit history.

### Verification status

| Lane | Result |
|---|---|
| `test:bifrost:journey:server` | 7/7 pass |
| `test:bifrost:journey:oracle` | 28/28 pass |
| `test:bifrost:journey:otlp` | 10/10 pass |
| `test:bifrost:journey:forge` | 13/13 pass, intermittent (see Remaining) |
| `test:sql` | pass |
| `check:tenant-isolation`, `check:unwrap-audit` | pass |
| `vala-bifrost-redux` lib | 977/977 pass |

### Remaining

1. **Correlation-policy direction reversed by review.** Rather than giving
   `audit_log` the correlation envelope, ingest will honour
   `CorrelationPolicy::None`: `DecodeContext` already carries
   `definition.correlation_policy`, so the unconditional append becomes
   conditional at the one site. Dynamic tables (`definition: None`) keep today's
   envelope. This restores the `None` variant and the table's policy, and leaves
   `audit_log` with exactly one principal, one request id, and one trace id.
2. **`audit_log` physical layout** does not follow the house pattern: `seq` is
   bloomed though it is monotonic and already the secondary sort key, while the
   two real audit access paths are uncovered. Target:
   `bloom = ["audit_principal_id", "resource", "operation"]`, sort unchanged.
3. **Forge lane stability.** Two `production_closeout` scenarios intermittently
   fail under the lane's parallelism since publication began running — once on
   an empty orphan set, twice on a public query hitting the handler timeout. A
   pending edit makes the retained-history fallback fire only when the outbox
   answer is empty, keeping the fused query off the hot path.
4. `mise run codegen:check` and regeneration (the `audit_log` schema change
   moves generated contracts and its fingerprint).
5. `mise run skills:sync` / `check:skills-sync`, `check:object-store-pin`,
   final `fmt`, `lints`, `git diff --check`.
6. Acceptance-evidence table, no-legacy inventory, and requirement-to-evidence
   closure.

### Notes for the change owner

- Card MCP tools have no server-side equivalent: HEAD's dead
  `register_card_tools` was deleted along with the old `wyrd-mcp` surface.
- `CardPyResult` → `WyrdPyResult` (REQ-024) stays deferred to TASK-002.
