---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-skald-workflow-runtime
spec_revision: 5
requirements: [REQ-014, REQ-015, REQ-017, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-029, REQ-030, REQ-032, REQ-033, REQ-034, REQ-034A, REQ-034B, REQ-034C, REQ-035, REQ-038, REQ-039, REQ-041, REQ-042, REQ-043, REQ-044, INV-001, INV-005, INV-006, INV-008, INV-009, INV-010, INV-010A, INV-011, INV-012, INV-013, AC-004, AC-009, AC-010, AC-014, AC-015, AC-016, AC-017, AC-018]
depends_on: [TASK-001, TASK-002, TASK-003]
parent_task:
remediates: []
---

## Outcome and Value

`wyrd-server` hosts registered Workflow runs as secure, bounded, asynchronous,
process-owned resources. Submission returns a run ID promptly; authorized owners
can poll or cancel long code-review workflows after disconnect; no durable queue,
run table, replay, or second executor is introduced.

## Owners, Scope, Consumers, and Prohibited Changes

The server owns authentication, authorization, policy, audit, tenant-qualified
Card resolution, route dependencies, admission, process lifecycle state, HTTP,
and shutdown. Skald remains the sole workflow executor. The canonical audit path
owns all permission decisions; the existing gateway pipeline owns WyrdGateway
governance; approved server egress controls own ExtGateway networking.

Do not persist runs or idempotency records, add SQL/migrations/queues/leases,
allow cross-replica lookup without affinity, execute Native or Agent tools,
reimplement DAG/Prompt/retry semantics, add credential administration, or add
Python/TypeScript/MCP/UI surfaces.

## Approach

1. Add a cohesive server owner for process-local run state, tracked work,
   cancellation, limits, retention, idempotency, and shutdown composition.
2. Authenticate, authorize/audit, resolve and pin the exact Card graph, validate
   suitability, and admit work before returning the first `202`.
3. Execute Skald asynchronously with route-specific existing provider registries
   and atomically project monotonic snapshots.
4. Implement owner-qualified get/cancel, terminal races, deadlines, eviction,
   and indistinguishable not-found behavior.
5. Project the typed endpoints through the shared error/OpenAPI boundary and
   verify tenant, audit, SSRF, secret, and no-side-effect negative paths.

## Ordered Implementation Scenarios

### Scenario 1 — Acceptance is authorized, audited, resolved, and bounded

**Behavior.** Create derives tenant/actor from verified credentials, requires
`workflows:run`, transactionally audits allow and deny, resolves and pins an
active exact graph, rejects Native/tools/invalid routes/oversized or over-cap
requests, and performs no provider side effect on any pre-acceptance failure.
This proves REQ-014, REQ-017, REQ-029, REQ-032 through REQ-034, INV-005,
INV-006, INV-008, AC-009, and AC-010.

**RED.** Add a `pg_workflow_runs` server integration target with
`create_authorizes_audits_resolves_and_admits_before_provider_io` and run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-server --test pg_workflow_runs \
  -E 'test(=create_authorizes_audits_resolves_and_admits_before_provider_io)'"
```

It must fail because the server has no Workflow-run resource or Workflow
authorization path.

**GREEN.** Implement the minimum pre-acceptance server workflow on one cohesive
owner and return a queued snapshot only after every required gate succeeds.

**REFACTOR.** Reuse existing authenticated caller, Card registry, policy, audit,
and admission patterns; remove duplicate handler plumbing.

### Scenario 2 — Accepted work survives disconnect and exposes monotonic state

**Behavior.** After `202`, tracked work continues without the request, progresses
queued→running→terminal without regression, uses its pinned graph, retains
normalized intermediate results, and remains queryable by its owner until TTL or
capacity eviction. This proves REQ-015, REQ-020 through REQ-023, REQ-029,
REQ-030, REQ-034A, INV-001, INV-013, AC-004, and AC-018.

**RED.** Add `accepted_run_survives_disconnect_and_polls_monotonically` to
`pg_workflow_runs` and run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-server --test pg_workflow_runs \
  -E 'test(=accepted_run_survives_disconnect_and_polls_monotonically)'"
```

It must fail until work and lifecycle state are detached from the response.

**GREEN.** Host the existing Skald executor as tracked process work and expose
internally consistent snapshots through the typed GET route.

**REFACTOR.** Keep lifecycle dependencies on one owning struct and keep pure
snapshot transformations synchronous.

### Scenario 3 — Idempotency, cancellation, deadline, and terminal races are safe

**Behavior.** Stable same-request replay returns the existing run without
duplicate acceptance/provider work; changed-request reuse conflicts; cancel is
idempotent; completion/cancel races commit one terminal state; total deadline
times out; shutdown cancels and drains; active runs are never retention-evicted.
This proves REQ-019, REQ-030, REQ-034A, REQ-034C, INV-013, and AC-018.

**RED.** Add `run_lifecycle_is_idempotent_cancellable_bounded_and_race_safe` to
`pg_workflow_runs` and run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-server --test pg_workflow_runs \
  -E 'test(=run_lifecycle_is_idempotent_cancellable_bounded_and_race_safe)'"
```

It must fail because no server Workflow lifecycle or deduplication state exists.

**GREEN.** Reuse the existing typed idempotency key, cancellation token, tracked
task, clock/test-time, and shutdown patterns to implement the approved bounded
semantics.

**REFACTOR.** Remove duplicated state-transition checks and keep one monotonic
terminal transition boundary.

### Scenario 4 — Lookup is tenant/owner qualified and replica-local

**Behavior.** Current permission plus tenant/owner is required for get/cancel;
foreign tenant, foreign principal, evicted, expired, process-lost, and unknown
IDs are indistinguishable 404s; non-owning replicas rely on deployment affinity
and do not claim transfer/recovery. Every evaluated permission is canonically
audited. This proves REQ-032, REQ-033, REQ-034B, INV-006, INV-013, AC-009,
AC-010, and AC-018.

**RED.** Add `run_lookup_and_cancel_hide_foreign_or_lost_identity` to
`pg_workflow_runs` and run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-server --test pg_workflow_runs \
  -E 'test(=run_lookup_and_cancel_hide_foreign_or_lost_identity)'"
```

It must fail until run state is qualified and authorized at lookup.

**GREEN.** Apply existing principal, permission, audit, and not-found projection
patterns before returning any state.

**REFACTOR.** Share the authorization/audit seam across get and cancel without
creating unaudited shortcuts.

### Scenario 5 — Server routes use governed gateway or direct bounded egress

**Behavior.** WyrdGateway enters the in-process governed pipeline with verified
caller context; ExtGateway uses direct screened/pinned egress and its exact
tenant binding; Native and tools fail before execution; all supported protocol
combinations and negative SSRF/redirect/secret cases preserve stable redacted
errors. This proves REQ-034, REQ-035, REQ-038 through REQ-044, INV-009 through
INV-012, AC-014 through AC-017.

**RED.** Add `server_routes_gateway_and_external_steps_without_recursive_egress`
to `pg_workflow_runs` and run:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked \
  -p wyrd-server --test pg_workflow_runs \
  -E 'test(=server_routes_gateway_and_external_steps_without_recursive_egress)'"
```

It must fail until the server supplies route-specific providers to Skald.

**GREEN.** Compose the implemented gateway seam and approved outbound-network
owner around the same Skald executor with no recursive Wyrd gateway call for
ExtGateway.

**REFACTOR.** Keep security checks in their existing owners and route selection
out of the DAG engine.

## Acceptance Criteria

- POST create, GET status, and POST cancel project the approved typed contract
  and structured errors.
- Runs continue after disconnect but are lost on restart; no persistence or
  recovery artifact exists.
- Global/per-tenant active and retained limits, size limits, deadlines, TTL,
  oldest-terminal eviction, and shutdown drain are enforced deterministically.
- Authorization and audit ordering is fail-closed before protected work.
- Server route suitability rejects Native and tools; ExtGateway never enters
  WyrdGateway.
- Deployment documentation states affinity as required for V1 multi-replica
  operation.

## Expected Write Set and Consumer Closure

Likely surfaces include `crates/wyrd/wyrd-server` application state,
configuration, workflow component/routes, OpenAPI aggregation, shutdown,
permission/audit consumers, existing gateway/outbound-network integration, and
a real-server Postgres integration target. The server manifest may add the
existing Skald Workflow owner as an approved workspace dependency. No SQL
migration or run table belongs in the write set.

## Verification and Evidence

Run all five focused scenarios sequentially, then:

```bash
mise run test:wyrd
mise run test:gateway:journey
mise run codegen:check
mise run check:tenant-isolation
mise run check:single-into-response-impl
mise run check:unwrap-audit
mise run fmt
mise run lints
git diff --check
```

## Material Stop Conditions

- Correctness requires a database table, durable queue, lease, replay, recovery,
  or cross-replica ownership protocol.
- Server execution must accept Native or Agent tools, or change the approved
  permission/audit/ownership semantics.
- The existing gateway or network owner cannot satisfy the approved route and
  SSRF contract without a new third-party dependency, Cargo feature, or material
  redesign.
- A public endpoint, status, retention, idempotency, or terminal-race decision
  must change.

## Authority Links

- `changes/active/skald-workflow-runtime/spec.md` Revision 5
- `changes/active/wyrd-gateway-v1/spec.md` Revision 21
- `AGENTS.md` §§5–6, 9–12
- `architecture/agent-rules.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/architecture/patterns.md` §§Server Pattern, External
  Network Pattern
- `architecture/references/languages/errors.md`
- `architecture/references/languages/testing-workflows.md`
