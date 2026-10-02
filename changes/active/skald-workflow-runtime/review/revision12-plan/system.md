# Revision 12 independent system perspective

Scope: static system-rev pass on the approved Revision 12 specification and
TASK-002-cleanup, TASK-003, TASK-004, and TASK-005. No builds, tests, source edits,
or plan edits were performed. The repository has no `.codegraph/` directory;
source discovery therefore used repository text search and direct reads.

## Findings

No evidence-backed Critical or Major system finding in the revised packet.
This is a specialist pass, not an overall readiness verdict or implementation
verification result.

## Traced topology and workflows

- Author's Python/Node/Rust process loads local files through wyrd-loader. Only
  an external Agent/Prompt ref initializes ambient Cards/client authentication.
  TASK-002-cleanup's ordering preserves sibling/external provenance, transitive
  exact reads, and completed-Workflow publication. Cancellation publishes no
  partial instance and loading has no durable write or execution-secret read.
- `wyrd apply` enters existing server composite registration, tenant effective
  bodies and declarative Skald validation, then existing audited write binding.
  TASK-002-cleanup explicitly retains the exact preflight UID write fence and
  collision tests; an invalid graph dispatches no provider and writes no partial
  graph. Workflow Card identity creates no principal or transferred privilege.
- Registered local execution retains its Cards connection context; authored
  local execution lazily obtains ambient client configuration for selected
  WyrdGateway calls. TASK-003 owns common selected-route dependency/secret
  preparation for all SDKs/CLI. Per-call fallback and timeout are immutable;
  concurrent calls do not mutate shared transport headers or provider state.
- Accepted remote runs belong to one wyrd-server process. Deployment affinity
  is explicitly required; non-owning replica and restart loss return the same
  404. There is no implied durable recovery, provider replay, Workflow credential,
  or cross-replica state service. TASK-005 owns the operator/architecture closure.

## Interruption and shared-service containment

1. Missing/unauthorized/inactive dependencies fail the loading or admission
   operation before dispatch. They do not crash the shared server or make Cards,
   gateway, and Bifrost globally unavailable. Failed preparation releases its
   tenant/global reservation and wakes waiters; retries may prepare anew.
2. Creator disconnection and replay-waiter cancellation do not destroy tracked
   preparation or accepted execution. Scoped idempotency prevents duplicate
   accepted calls within the process lifetime. Fresh requests authenticate and
   audit independently; replay cannot widen accepted authority.
3. Explicit cancellation/deadline stops scheduling, drains Skald-owned work,
   preserves completed results, and commits one terminal snapshot. Gateway
   settlement remains gateway-owned and cannot mutate the terminal Workflow or
   retain its admission slot.
4. An admitted query response survives cancellation of its Skald waiter under
   a tracked owner. TASK-004 fixes transfer-before-waiter exposure, signals exact
   authenticated lifecycle cancellation, and joins collection/settlement before
   Workflow capacity release. Original query deadline/shared shutdown deadline
   bounds confirmation. Terminal/transport loss produces honest unconfirmed
   cleanup diagnostics rather than partial rows or a false clean drain.
5. Shutdown closes Workflow admission and drains preparations/executors before
   required gateway/query owners, under the same process deadline. Existing
   BoundServer already shares one absolute deadline across supervised work,
   MCP, gateway accounting/capture and Bifrost shutdown. TASK-004 calls out the
   required insertion ordering rather than allocating fresh grace periods.
6. Oversized or deep graph preparation is bounded outside the run lock and
   request executor. TASK-004 Scenario 7 requires ordinary Cards/gateway/Bifrost
   requests to remain serviceable with normal concurrency; terminal metadata is
   reserved so failure/cancellation cannot be prevented by exhausted snapshots.

## Source evidence and counter-hypotheses

- `crates/shared/wyrd-client/src/cards/hydrate/graph.rs:173` reads through the
  existing client, verifies exact returned Card identity, requires relationship
  UIDs, and currently fetches inventories. The task explicitly extends this
  owner for authored provenance and in-memory loading without irrelevant
  inventory/artifact work. Existing disk hydration is retained as a consumer.
- `crates/wyrd/wyrd-server/src/components/cards/resolve.rs:175` owns tenant
  effective bodies and distinct sibling/external caches. Its current
  `validate_workflows` still uses WorkflowGraph; the cleanup explicitly removes
  that adapter/lookup loop while preserving effective resolution and Skald
  declarative validation. It does not replace the registration transaction.
- `components/auth/caller_extractor.rs:13,44` stores verified tenant, principal,
  request correlation and verified delegation without retaining a bearer token.
  TASK-004 captures this accepted scoped context; it explicitly preserves live
  gateway deployment/credential admission and forbids authority refresh.
- `components/gateway/invocation.rs:296,427` consumes Caller and separate
  cancellation, and performs normal gateway permission/admission when
  `authorized` is false. TASK-004 explicitly requires false; Workflow acceptance
  is not a blanket authorization to use a model.
- `components/cards/routes.rs:93` is the canonical permission/audit/kind-qualified
  UID read boundary. TASK-004 requires Cards tools to use it or an equivalent
  shared exact-ref boundary, not unaudited service resolution alone.
- `oracle/lifecycle_controls.rs:47` retains the response and validates terminal
  evidence under the original deadline; dropping that future loses confirmation.
  TASK-004's tracked read-tool owner closes that cancellation-lifetime hazard
  and requires a genuinely forwarded admitted-query journey, not a fake stream.
- `app/server.rs:744-805` establishes the current single shutdown deadline and
  tracker ordering. The plan requires Workflow insertion ahead of required
  gateway/query drain and Scenario 6 uses the actual BoundServer path.

Counter-hypotheses examined: ambient ref resolution transferring team Agent
privilege; Cards-loaded execution silently switching server connection;
registration validation dispatching providers; accepted context bypassing live
gateway permission/admission; query waiter abort discarding settlement; Workflow
failure monopolizing sibling traffic; shutdown extending deadlines; restart
implicitly replaying work. Each has an explicit boundary and material assertion
in the current packet. None requires an additional owner or speculative durable
infrastructure.

## Proof coverage and static rehearsal

- Cleanup: rehearse existing loader → provenance-aware existing graph owner →
  Skald lowering, then exact Cards view. Scenarios 1–5 prove registry-free local
  loading, real-server refs, version/UID pinning, refusals and three SDK projections;
  Scenario 2 preserves registration UID/provenance safety and no principal.
- TASK-003: rehearse shared config → selected secrets/dependencies → existing
  authenticated transport/provider codecs. Required immutable call metadata,
  remaining timeout, cancellation and SDK configuration closure are present.
- TASK-004: rehearse verified/audited request → scoped reservation → tracked
  preparation → bounded atomic acceptance → executor/terminal boundary. Scenarios
  2–3 cover disconnect/idempotency/scoped authority, Scenario 4 forwarded query
  cancellation/settlement, Scenario 6 shutdown/restart/terminal races, and Scenario
  7 shared-service responsiveness and budget refusal.
- TASK-005: rehearse local-file run → compiled apply → exact registered load/run,
  remote detach/status/cancel, then route/security matrix and team reuse in all
  SDKs. Explicit ignored CLI and Rust SDK journeys close aggregate-lane gaps.

Remaining implementation choices in this system pass are private placement of
guards/channels, extension method names, source movement and deterministic test
gates. Security, interruption, identity, pinning, capacity ownership and shutdown
decisions are specified. Runtime proof remains the implementation tasks' duty;
this static pass does not certify tests as passing.

Authorities: current user-approved contracts; AGENTS.md and agent-rules.md;
wyrd-design.md/doctrine for owner/ref/principal boundaries; security posture for
verified scope, delegation and canonical audit; bifrost-design.md for query
ownership and honest terminal evidence; Revision 12 normative flows and task
acceptance criteria. Production-risk and plan-review rubrics governed finding
severity and interruption analysis.
