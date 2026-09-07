---
id: SPEC-bifrost-forge-oracle-integration
revision: 2
status: draft
---

# Bifrost Forge and distributed Oracle integration

## Human intent and user value

Integrate `forge-compaction-refactor` into `oracle-distributed` so one coherent
Bifrost implementation supports the complete production data path:

```text
public write -> Gate -> Scribe -> Forge -> Iceberg
public read  -> Gate -> Oracle -> Interactive or Analytical result
```

The integrated system must preserve the refactored Forge maintenance model,
the distributed Oracle one-planner/two-path model, the production fixes made on
both branches, and the simplest useful operational telemetry. Users must be
able to write and read exact tenant-isolated data through the public Wyrd
surface in standalone and distributed deployments. Operators must be able to
diagnose the core path without maintaining hundreds of overlapping metrics.

This is an integration and deletion change, not an additive compatibility
layer. Superseded Forge and Oracle implementations, tests, commands, metrics,
and documentation must leave the current tree.

## Repository facts informing the draft

- The integration destination is `oracle-distributed`, observed at
  `2864459d54222518cda7a04c50c37f2b10ce2034` when revision 1 was drafted.
- The merge source is `forge-compaction-refactor`, observed at
  `c759227b3fe365d3f46c235b8ca544fe96d79f86` when revision 1 was drafted.
- Their observed merge base was
  `24b8349e3eae96a8901ac29aac93acf2a20705ff`.
- The source worktree contained uncommitted active-change-packet cleanup when
  revision 1 was drafted. Those filesystem changes are not part of the
  observed source commit.
- Both branches materially changed Oracle's Interactive path. The destination
  replaced heuristic planning with one retained physical root; the source
  added durable reader protection and follower-deadline safety to the older
  Oracle shape.
- `oracle-distributed` contains the existing real child-process
  `BifrostProcessCluster`. `forge-compaction-refactor` contains stronger
  role-separated Forge write, promotion, rewrite, cleanup, and reader-race
  scenarios, primarily on existing in-process test owners.
- `forge-compaction-refactor` reduced Forge's closed production metric catalog
  to seventeen families. Scribe telemetry is materially the same on both
  branch heads and therefore requires an explicit integrated cleanup decision.

These facts identify the starting implementations. They do not override the
architecture authorities listed below.

## Scope

- Incorporate `forge-compaction-refactor` into `oracle-distributed`, with
  `oracle-distributed` remaining the integration destination.
- Reconcile Oracle Interactive and Analytical planning, admission, execution,
  reader protection, deadlines, cancellation, settlement, and cleanup safety.
- Retain the refactored Forge promotion, managed rewrite, publication,
  reconciliation, snapshot expiration, expired-object cleanup, orphan cleanup,
  scheduling, fencing, resources, readiness, and recovery behavior.
- Retain the destination's Gate delegation, Scribe ingress, live-tail, Oracle
  peer, public query, terminal, resource, audit, and process-topology behavior.
- Reconcile public and internal contracts, SQL state, server boot, readiness,
  shutdown, configuration, health, audit, generated artifacts, documentation,
  and test support affected by both branches.
- Delete superseded production paths and their exclusive tests, metrics,
  commands, checks, fixtures, and documentation.
- Simplify Scribe and Oracle telemetry using the refactored Forge telemetry
  standard.
- Prove the integrated write, maintenance, and read paths across standalone,
  distributed, single-tenant, and multi-tenant production-shaped topologies.
- Preserve the approved behavior of other active change specifications present
  on the destination unless an explicit later spec revision says otherwise.

## Non-goals

- A new Bifrost test harness, topology framework, process controller, or
  renamed wrapper around an existing harness.
- A second Oracle planner, query classifier, execution attempt, fallback path,
  distributed scheduler, shuffle service, or query-job API.
- A second Forge compaction planner, legacy compaction compatibility route, or
  catalog commit performed by the managed compaction core.
- A new public API, Card kind, storage system, analytical engine, dependency,
  Cargo feature, or alternate tenant model merely to make the branches merge.
- Distributed Scribe writes or cross-pod Scribe assembly.
- Treating a clean textual merge, successful compilation, or lower-tier test as
  proof of integrated production behavior.
- Preserving empty tests, inactive implementation, unused telemetry, obsolete
  qualification machinery, or unreachable checks for historical appearance.
- Deleting immutable historical migrations or weakening compatibility rules
  for durable state that supported rolling deployments may still read.
- Hand-editing generated schemas, stubs, OpenAPI, protobuf descriptors, or
  documentation projections.
- Delivering a periodic metadata-only manifest-rewrite protocol. Normal
  Iceberg manifest creation during promotion and compaction remains required.
- Restoring the deleted inactive legacy manifest-maintenance dispatcher.

## Definitions

- **Destination:** `oracle-distributed`, the branch into which the source is
  integrated and whose current Oracle execution architecture is retained.
- **Source:** `forge-compaction-refactor`, the branch whose Forge replacement,
  reader-safety behavior, production fixes, and focused journeys are
  incorporated into the destination.
- **Integrated candidate:** one immutable tree containing the intended history
  and behavior of both frozen inputs plus the reconciliation and deletion work
  required by this specification.
- **Interactive:** Oracle execution of the normal DataFusion physical root
  returned by the single pinned `datafusion-distributed` planning operation.
- **Analytical:** Oracle execution of the streamed stage graph represented by a
  `datafusion_distributed::DistributedExec` root returned by that same planning
  operation.
- **Protected read cut:** one immutable, tenant-qualified query cut whose
  Iceberg snapshots, committed Scribe hot objects, and leased Scribe live-tail
  sources remain protected from deletion until every local and distributed
  reader has terminally settled.
- **Standalone topology:** one real `wyrd-server` child process using the
  existing `WYRD_TARGET=all` role composition and public serving surface.
- **Distributed topology:** multiple real `wyrd-server` child processes using
  existing role targets and authenticated internal peer boundaries behind one
  logical Wyrd serving surface.
- **Production journey:** a real client-to-server-to-client workflow using the
  public Wyrd API or first-class SDK and repository-managed Postgres rather than
  an in-process engine call or test-only ingest command.
- **Closed telemetry catalog:** the complete bounded set of production metric
  families and label values emitted by one subsystem and documented and tested
  as that subsystem's operator-facing metric surface.
- **Stale logic:** code or support material whose behavior is unreachable,
  superseded by the selected architecture, or exclusively supports a removed
  implementation. Historical durable migration evidence is not stale logic.

## Required behavior

### Integration authority

#### REQ-001 — Direction and provenance

The integrated candidate shall be derived from `oracle-distributed` as the
destination and shall incorporate the frozen `forge-compaction-refactor` source
without reversing their roles or reconstructing the source as an unrelated
replacement history.

#### REQ-002 — Immutable merge inputs

Before integration implementation begins, both branch inputs shall be clean,
explicitly frozen commits. The source worktree's uncommitted change-packet
cleanup shall be intentionally committed, retained for later reconciliation,
or discarded by its owner before its frozen source commit is recorded. No
uncommitted production or planning state may be silently omitted from or added
to the merge input.

#### REQ-003 — Behavioral conflict resolution

Conflict resolution shall use these starting authorities:

- the destination owns Oracle's one-planner/two-path execution architecture,
  distributed graph lifecycle, Gate delegation, current Scribe ingress and
  live-tail fixes, public query lifecycle, and complete Bifrost verification
  orchestration;
- the source owns the replacement Forge architecture, Forge SQL lifecycle,
  reader-safety requirements, Forge readiness and recovery, and the simplified
  Forge telemetry catalog;
- shared contracts, catalog access, server composition, Scribe/Oracle seams,
  fixtures, and documentation shall be reconciled against the requirements and
  invariants in this specification rather than selected wholesale from either
  branch.

#### REQ-004 — One current architecture

The integrated current tree shall describe and implement one coherent Bifrost
architecture. Architecture prose from the source that describes heuristic
Oracle routing, fallback, retry after Analytical selection, a second physical
build, public execution-plan explanation, or another superseded destination
behavior shall not survive the merge.

### Oracle planning, execution, and read safety

#### REQ-005 — One planner and one retained physical root

Oracle shall pin one tenant-qualified source cut, run the pinned
`datafusion-distributed` planner once, and retain and execute the exact returned
physical root. A normal root shall select Interactive; a `DistributedExec` root
shall select Analytical. The caller shall not select or hint the execution
path.

Oracle shall have no operator allowlist, candidate heuristic, second physical
build, whole-plan splitter fallback, stale replan, Interactive fallback after
Analytical selection, or successor attempt for a selected Analytical query.

#### REQ-006 — Reader protection precedes snapshot data IO

Oracle shall acquire durable protection for every Iceberg snapshot in the
complete multi-table cut before materializing snapshot-backed providers or
performing snapshot data IO. It shall revalidate that the protected identities
still describe the intended cut before execution.

A concurrent authority change before plan materialization may cause only the
bounded cut-stabilization behavior permitted by the current reader authority;
it shall never become a post-selection replan, a second selected execution
attempt, or a fallback to another path.

#### REQ-007 — Reader protection spans local and distributed execution

The protected read cut shall remain a hard Forge retention root until local
sources and result batches are released, all assigned follower work has
terminally settled and joined, and the public query has reached its terminal
outcome. Interactive and Analytical queries shall receive the same snapshot
deletion safety.

Follower assignments shall carry authenticated, tenant-bound proof of the
protected cut needed to prevent distributed reads from outliving their
authority.

#### REQ-008 — Exact source binding

Oracle shall bind every physical scan occurrence to the exact pinned Iceberg,
published-hot, and leased live-tail source evidence represented by its cut.
Active, immutable, and staged rows shall remain Scribe live-tail authority;
Oracle shall never open another node's local path or read WAL as a normal query
source.

When an identity is visible through more than one tier, only exact evidence of
the more advanced authority may suppress the earlier source. The resulting
query shall return every logical row exactly once.

#### REQ-009 — Admission and resource ownership

Interactive and Analytical work shall retain separate queues and counters
under one atomic Oracle capacity bound. Analytical work shall never borrow the
configured Interactive floor. Both paths shall execute with the admitted
query-owned runtime, memory ceiling, scratch allocation, absolute deadline,
and cancellation tree derived for the retained physical root.

#### REQ-010 — Terminal and deadline semantics

One ingress deadline shall bound planning, admission, local work, peer work,
streaming, cancellation, and cleanup. An accepted follower shall honor the
query execution deadline rather than treating a shorter assignment-ticket
acceptance lifetime as the execution deadline.

Every public result stream shall end in one explicit success or failure
terminal carrying the selected execution path and owning attempt identity.
Rows preceding a failure terminal shall not constitute a successful result.
Cancellation or failure shall join descendants before admission, graph,
scratch, transport, and reader ownership are reported released.

### Scribe, Forge, and cross-system authority

#### REQ-011 — Preserve current Scribe ingestion fixes

The integration shall preserve the destination's exact pre-material Arrow and
IPC admission accounting, contiguous live-tail batch slicing, staged-source
discovery, canonical namespace handling, deadline enforcement, bounded
materialization, and verified delegation through Gate.

It shall also preserve the source's valid live-tail readability, multi-reader
lease lifetime, terminal release, and Forge reader-protection obligations.

#### REQ-012 — Refactored Forge is the sole maintenance implementation

Forge shall have one active maintenance architecture: unchanged Scribe-object
promotion followed by managed compaction and the separate retention and
cleanup protocols defined by Bifrost authority. The managed compaction core
shall remain the sole owner of selection, grouping, delete application,
sorting, partition fanout, rolling, and output `DataFile` production.

Forge shall remain the owner of tenant authority, durable demand, tasks,
leases, fences, resources, handoff validation, catalog publication,
reconciliation, audit, retention, and garbage collection.

#### REQ-013 — Promotion and rewrite preserve exact authority

Promotion shall append validated committed Scribe hot objects unchanged.
Managed rewrite shall publish only the exact validated five-field handoff from
the managed core. Definite failure, conflict, uncertainty, cancellation, and
success shall remain distinct durable outcomes. Retry and reconciliation shall
never widen tenant, table, snapshot, attempt, operation, deadline, lease, fence,
or resource authority.

Rewrite output identity shall conform to the current authoritative flat,
table-bound, attempt-global ordinal path contract. Source-branch path grammar
that conflicts with that authority shall be adapted rather than preserved as a
second grammar.

#### REQ-014 — Destructive maintenance honors all protection roots

Snapshot expiration, expired-object cleanup, and never-published orphan
cleanup shall preserve active refs, unresolved or uncertain operations,
committed Scribe objects lacking exact promotion evidence, live-tail leases,
and every protected Oracle reader cut. No object may be deleted from age, path
shape, local cache, or storage listing alone.

Loss or unavailability of reader authority shall stop destructive maintenance
and remove readiness from affected roles; it shall never be treated as an
empty protection set.

#### REQ-015 — Server composition and readiness include both architectures

The existing `wyrd-server` composition shall support the current Oracle peer
plane and analytical graph lifecycle together with Forge coordinator, Forge
worker, reader authority, recovery, and cleanup state. Role readiness shall
follow the existing `WYRD_TARGET` mapping and fail closed when any enabled
role's required catalog, storage, SQL, audit, peer, resource, reader-authority,
recovery, or reconciliation dependency is unsafe.

Shutdown shall stop admission, cancel and join bounded work, durably settle or
retain recovery evidence, retire reader authority by the server deadline when
safe, and never delete accepted Scribe or Forge evidence merely to terminate.

### Contracts, durable state, and generated artifacts

#### REQ-016 — Reconcile contract families at their owner

The integrated typed source contracts shall contain the current Oracle
execution-path, analytical-graph, terminal, delegation, permission, and audit
behavior together with the current Forge promotion, rewrite, cleanup,
reader-authority, and fenced-audit behavior. Resolving one family shall not
erase the other.

Removed runtime concepts shall not remain exposed as compatibility aliases or
parallel public variants. A version-aware reader required for already-supported
persisted data may remain only as a durable decode boundary; it shall not
restore a removed public authoring or execution path.

#### REQ-017 — Preserve durable migration history

Existing immutable SQL migrations shall remain ordered and checksummed.
Required schema evolution shall follow the repository migration and rollout
contract. Integration cleanup shall not delete historical migrations, rewrite
already-applied migration meaning, or make a newer worker claim durable work it
cannot decode.

#### REQ-018 — Regenerate derived artifacts

Generated schemas, OpenAPI, language projections, checked documentation, and
protobuf descriptors shall be regenerated from reconciled owning sources. A
generated binary or golden conflict shall never be resolved by manually
choosing or editing one branch's output.

### Required deletion and retention

#### REQ-019 — Delete superseded Forge production logic

The current tree shall contain no active legacy Forge candidate-selection,
bin-packing, right-sizing, inactive maintenance-dispatch, staging-fold,
reselection, duplicate rewrite, or parallel publication route superseded by
the managed Forge architecture.

This includes deleting the stale `forge/binpack.rs`, `forge/discovery.rs`,
`forge/maintenance.rs`, and `forge/right_size.rs` modules and removing stale
bodies and callers from the retained `compact`, `rewrite`, `live_replace`, and
`planner` owners. The retained owners shall expose only the refactored behavior
required by REQ-012 through REQ-014.

#### REQ-020 — Delete superseded Oracle production logic

The current tree shall delete the old whole-plan splitter and every exclusive
caller, heuristic query classifier, second-build path, pre-selection fallback,
stale-replan metric and behavior, and Interactive retry path superseded by
REQ-005. Reader protection adapted from the source shall not reintroduce those
deleted execution semantics.

#### REQ-021 — Delete obsolete support material

The integration shall delete:

- the legacy staged-compaction journey and empty Forge journey placeholders;
- test-only reporting types and assertions that exist solely for the removed
  Forge telemetry or maintenance path;
- the retired Bifrost benchmark and qualification stack, fixtures, scripts,
  configuration, documentation claims, and `mise` tasks removed by the
  destination;
- `check:bifrost-bench-registration` when its sole checked property is
  unreachable after benchmark removal; and
- stale documentation that describes a deleted implementation or unsupported
  performance qualification.

A required behavioral scenario shall be migrated to the surviving owner before
its obsolete test representation is deleted. Empty or assertion-free tests do
not constitute behavior to preserve.

#### REQ-022 — Retain live supporting owners

The integration shall retain the existing `BifrostProcessCluster` as the sole
production multi-process journey harness. It shall retain `WyrdTestCluster` and
focused standalone Forge fixtures only for their existing lower-tier roles
where they provide deterministic inspection, fault injection, or subsystem
integration not supplied by the process harness.

No new harness or wrapper abstraction shall be introduced. A supporting owner
may be removed only after every live consumer and uniquely covered behavior has
moved to an existing surviving owner.

#### REQ-023 — Do not add permanent stale-name checks

Removal shall be proven by caller closure, compilation, review of the
integrated diff, and behavioral verification. The change shall not add a
permanent name-ban or a check that merely verifies another check. Existing
checks that protect live boundaries such as tenancy, dependency direction,
resource ownership, generated drift, or a single authoritative implementation
shall remain.

### Tracing, metrics, logging, and audit

#### REQ-024 — Use one subsystem-wide telemetry selection rule

A production metric shall exist only when its bounded aggregate directly
supports an alert, service-level indicator, capacity decision, backlog or
pressure decision, or active-owner balance invariant. Core execution flow,
identities, detailed transitions, and diagnostic context shall use structured
traces or logs. Durable authority and recovery correctness shall use audit and
the owning durable state rather than inferred telemetry.

Metric labels shall remain closed and bounded. Tenant, table, SQL, object path,
query, batch, task, attempt, snapshot, principal, and request identities shall
never be metric labels; when permitted, they belong in scrubbed traces or
protected durable evidence.

#### REQ-025 — Retain Forge's simplified closed catalog

Forge shall retain the source's closed seventeen-family production metric
catalog covering demand, pending and active work, attempts, failures, duration,
committed physical input and output, proven deletion and expiration, and
compaction debt. The removed legacy Forge stage, lease, conflict, retry,
resource-envelope, scheduler, fairness, quarantine, and duplicate operation
families shall not return unless a later approved specification establishes a
new operator requirement.

Forge shall derive physical volume observations from committed or recovered
production evidence. It shall not perform object-store reads solely to
calculate telemetry.

#### REQ-026 — Simplify Oracle telemetry around the query lifecycle

Oracle shall retain bounded signals for Interactive and Analytical admission,
queue wait, active ownership, selected path, planning, execution, memory and
scratch refusal, exchange and spill pressure, terminal outcome, cancellation,
peer failure, time to first frame, terminal latency, incomplete streams, graph
cleanup, and read-audit relay health.

Oracle shall remove inert families and no-op emission hooks, use one canonical
terminal-duration signal rather than duplicate query-duration families, and
avoid counting one Analytical authority decision through overlapping stage and
security families. Detailed per-query scan and stage facts shall be represented
by structured spans unless their aggregate is required for an approved
capacity or service-level decision.

#### REQ-027 — Simplify Scribe telemetry around durable authority

Scribe shall retain bounded signals for ingress and acknowledgement, refusal,
queue pressure and age, WAL append and fsync, durable volume ownership, active
and immutable ownership, staging and publication age/outcome, replay,
reconciliation, live-tail availability, shutdown, and retirement.

Scribe shall remove or consolidate contention transition, vector, current,
cumulative, gauge, and event observations that report the same operator fact.
Telemetry cleanup shall preserve production-path evidence needed to diagnose
admission fairness, acknowledged-data safety, stalled staging/publication, and
recovery without retaining every internal state transition as a metric.

#### REQ-028 — Emit observations from the effect owner

Metrics, spans, and structured logs shall be emitted by the concrete production
owner that performs the observed effect. Test descriptors, surrogate emitters,
and test-only diagnostic facts shall not substitute for production
instrumentation. Every active gauge shall release on success, refusal, retry,
uncertainty, cancellation, failure, and shutdown.

Telemetry collection and failure rendering shall remain compatible with the
approved telemetry-first tiered-test behavior already present on the
destination.

### Production journeys and verification

#### REQ-029 — Use the existing process harness

Production topology journeys shall use and extend the destination's existing
`BifrostProcessCluster`. The change shall not introduce a new harness,
controller, topology DSL, or parallel process lifecycle. Existing Forge
production scenarios shall be exercised through this owner where real
cross-process behavior is required.

#### REQ-030 — Cover the production topology and tenancy matrix

The process-harness journeys shall cover:

| Topology | Tenant scope | Required complete behavior |
|---|---|---|
| Standalone | single tenant | Public registration and write through Gate and Scribe, Forge promotion and rewrite, then exact public Interactive read |
| Standalone | multiple tenants | Same-named tables, writes, maintenance, and exact reads remain isolated across tenants |
| Distributed | single tenant | Public write and Forge maintenance produce data read exactly through both Interactive and distributed Analytical Oracle paths |
| Distributed | multiple tenants | Concurrent tenant writes, maintenance, Interactive and Analytical reads, reader-safe cleanup, and explicit cross-tenant tripwires preserve isolation |

The exact number of retained journey functions is an implementation decision;
the four observable topology/tenancy obligations are not optional.

#### REQ-031 — Journeys use production public paths

The complete production journeys shall register tables, authenticate, write,
flush or drain, trigger or await real Forge maintenance, and query through the
public `wyrd-server` surface and real SDK or protocol client. Test-only child
commands may coordinate faults or inspect evidence but shall not replace the
public write or read operation being qualified.

Exact row identity and payload shall be asserted before and after promotion and
rewrite. Analytical scenarios shall assert that the returned terminal selected
Analytical; Interactive scenarios shall assert Interactive. No journey may
claim success from topology boot, internal seeding, a nonterminal frame, or
subsystem-local state alone.

#### REQ-032 — Preserve required edge and failure behavior

Across the strongest appropriate journey or supporting integration tier, the
integrated change shall prove:

- replayed append does not double-write and a conflicting replay fails;
- an under-privileged token cannot write or read protected data;
- an unregistered table write and a non-SELECT or oversized query fail with
  the stable public outcome;
- Scribe backpressure and drain preserve acknowledged authority;
- peer loss, deadline, cancellation, memory, exchange, and spill failure never
  produce partial success and release or retain ownership correctly;
- Forge lease loss, definite conflict, ambiguous publication, restart,
  cleanup-cursor takeover, and orphan protection preserve exact authority;
- a reader surviving snapshot expiration and physical cleanup sees its pinned
  rows exactly once; and
- tenant-tripwire failure terminates the whole query without returning a
  filtered partial result.

#### REQ-033 — Retain the complete tiered verification hierarchy

The destination's `verify:bifrost` and complete `test:bifrost` meanings shall
remain authoritative. The aggregate shall cover the Bifrost Rust, Python, and
TypeScript unit surfaces; Redux, SQL, and server integration; Rust capability
journeys; Python and TypeScript journeys; and the retained Forge scale lane
under the repository-managed environment.

The real Forge journeys shall replace obsolete Forge placeholders in that
catalog. The source's narrower Redux-only meaning of `test:bifrost` shall not
replace the destination aggregate.

#### REQ-034 — Every surviving journey passes

Every retained Scribe, Oracle, Forge, server, SDK, OTLP, MCP, Python, and
TypeScript Bifrost journey shall pass on the same immutable integrated
candidate. A journey may be deleted only when REQ-019 through REQ-023 establish
that it exclusively proves removed behavior or when its complete obligation is
covered by a stronger surviving journey.

Historical branch-local pass reports are evidence about the inputs, not
acceptance evidence for the integrated candidate.

## Invariants and prohibited outcomes

- **INV-001:** The integrated current tree has exactly one active Oracle
  planning and execution architecture.
- **INV-002:** The integrated current tree has exactly one active Forge
  selection, rewrite, publication, and maintenance architecture.
- **INV-003:** Reader protection never reintroduces heuristic routing, a second
  build, fallback, or a successor selected-query attempt.
- **INV-004:** No snapshot or object protected by a local or distributed Oracle
  reader, Scribe live-tail lease, unresolved operation, or retained catalog ref
  is expired or deleted.
- **INV-005:** Tenant authority comes only from verified credentials and is
  preserved through Gate, Scribe, Forge, Oracle, Postgres, object storage,
  peer assignments, telemetry, and audit.
- **INV-006:** No successful result contains a missing, duplicated, partial, or
  cross-tenant row.
- **INV-007:** No accepted Scribe row is discarded to satisfy shutdown,
  recovery, test, or merge convenience.
- **INV-008:** No stale Forge worker may publish, settle, audit, or delete after
  losing its lease or fence.
- **INV-009:** An uncertain external effect retains its original identity and
  protection evidence until reconciliation; it is never retried as fresh work.
- **INV-010:** Analytical work never consumes the protected Interactive floor.
- **INV-011:** A selected Analytical failure never falls back to Interactive or
  reports preceding frames as a successful result.
- **INV-012:** Generated artifacts are never hand-merged or treated as contract
  authority.
- **INV-013:** Historical migrations are never deleted or rewritten as code
  cleanup.
- **INV-014:** No new test harness, compatibility layer, dependency, or Cargo
  feature is introduced merely to integrate the branches.
- **INV-015:** No metric label contains an unbounded tenant, table, query, SQL,
  object, task, attempt, snapshot, principal, batch, or request value.
- **INV-016:** No object-store, catalog, or payload IO occurs solely to produce
  a metric.
- **INV-017:** Tests never weaken assertions, add sleeps, bypass public paths,
  ignore failures, or emit surrogate telemetry to make the candidate pass.
- **INV-018:** A clean merge, compile, unit test, or isolated branch result
  never substitutes for integrated production-journey evidence.

## Externally observable behavior and failure modes

- A successful public append returns acknowledged durability only after Scribe
  WAL fsync and batch fencing; later Forge states are not reported as append
  durability.
- A successful Interactive or Analytical query returns exact authorized rows
  followed by one terminal naming the selected path. The same logical rows
  remain exact across Scribe authority, promotion, rewrite, expiration, and
  cleanup.
- A tenant using the same logical table name as another tenant can write,
  maintain, and query only its own physical table and rows.
- Query planning, codec, peer, resource, tenant, deadline, cancellation, or
  cleanup failure returns a stable typed failure and never partial success or
  silent fallback.
- Admission pressure rejects before unsafe ownership is acquired and preserves
  already acknowledged data.
- Forge conflict or uncertainty remains distinguishable to recovery and
  operators and never becomes success from the existence of output objects.
- A reader whose cut was accepted before Forge maintenance remains able to
  finish that cut while newer readers observe the valid newer authority.
- A role whose required reader, recovery, audit, peer, catalog, storage, SQL,
  or resource dependency is unsafe reports not ready and does not accept work
  for that role.
- Operators receive a bounded core metric catalog plus correlated structured
  tracing, logging, audit, and durable state for detailed diagnosis.

## Material constraints

- Preserve Wyrd's language-agnostic client/server model and `wyrd-server` as
  the only external serving surface.
- Keep Bifrost durable behavior in Rust-owned Vala and server owners; clients
  remain contract projections.
- Keep `wyrd-spec` IO-free, async-free, PyO3-free, and foundational.
- Preserve Postgres RLS through `TenantConn` for tenant work and use only named,
  audited `OperatorPool` capabilities for cross-tenant Forge work.
- Preserve Oracle's fsynced local read-audit acceptance before rows and Forge's
  fenced, tenant-bound transactional audit behavior.
- Preserve the existing typed public permission, error, terminal, audit, and
  peer-ticket boundaries across Rust, HTTP, gRPC, Python, TypeScript, and MCP.
- Preserve role-specific readiness and the canonical `WYRD_TARGET` mapping.
- Reuse existing owners and installed dependencies. No speculative abstraction
  or configuration is authorized.
- Resolve source contracts first, regenerate derived artifacts second, and
  verify drift last.
- Use repository-managed Postgres and existing local storage dependencies for
  journey evidence; unit tests remain credential-free.
- Because this change crosses Vala, SQL, server, contracts, test
  infrastructure, generated artifacts, CI commands, and documentation, final
  acceptance requires the broad repository gate in addition to the complete
  Bifrost capability gate.

## Required system boundaries and cross-boundary flow

### Write and maintenance

1. A public client authenticates to `wyrd-server`; Gate derives tenant and
   permission authority and validates the logical batch identity and envelope.
2. Scribe admits the exact material, appends and fsyncs WAL, records the batch
   fence, installs authoritative active rows, and acknowledges the write.
3. Scribe rotates, stages, validates, registers, publishes, and durably records
   committed hot-object evidence without losing query visibility.
4. Forge validates and promotes the exact committed Scribe object unchanged
   into Iceberg under tenant, table, lease, fence, attempt, and operation
   authority.
5. Forge invokes the managed core for an eligible rewrite, validates its exact
   handoff, commits or reconciles one fenced snapshot operation, and settles
   SQL and audit exactly once.
6. Expiration and cleanup proceed independently only after current catalog,
   operation, Scribe, and Oracle reader protection proves deletion safe.

### Read

1. A public client authenticates to `wyrd-server`; Gate preserves verified
   delegation and tenant-qualified query authority.
2. Oracle validates read-only SQL and permissions and crosses the fsynced local
   read-audit acceptance boundary before permitting rows.
3. Oracle pins the complete Iceberg, published-hot, and live-tail cut, acquires
   durable reader protection, revalidates it, and materializes providers.
4. Oracle builds one physical root, selects Interactive or Analytical from that
   root, and obtains the corresponding bounded admission.
5. Interactive executes the retained normal root locally. Analytical executes
   the retained distributed graph through authenticated, tenant-bound peers
   using the same protected cut, deadline, resources, and cancellation tree.
6. Oracle streams bounded frames and one explicit terminal, joins local and
   distributed ownership, then releases admission and reader protection.
7. The audit relay appends the accepted read decision at least once to the
   canonical tenant hash-chained outbox.

### Observation and diagnosis

1. Each concrete Scribe, Oracle, or Forge owner emits its bounded operational
   metrics and structured spans at the effect boundary.
2. Metrics expose alertable aggregate health, pressure, capacity, backlog, and
   terminal state with closed labels.
3. Structured traces and logs carry permitted detailed causal context and
   correlations without exposing sensitive payloads.
4. Audit and durable state remain the authority for security, publication,
   recovery, and destructive-maintenance decisions.
5. Existing test consumers capture these production observations; tests do not
   invent another diagnostic channel.

## Acceptance obligations and credible evidence classes

### AC-001 — Integrated architecture and deletion closure

An immutable integrated diff and caller analysis show one Oracle architecture,
one Forge architecture, no prohibited legacy route, no orphaned exclusive
caller, no empty Forge journey placeholder, and no obsolete benchmark or
qualification path. Architecture and operator documentation describe the same
current behavior. Covers REQ-001 through REQ-004 and REQ-019 through REQ-023.

### AC-002 — Oracle planning and reader-safety seam

Focused unit and integration evidence proves one retained physical root selects
the path, both paths acquire reader protection before snapshot IO, follower
assignments preserve that protection and the ingress deadline, and protection
releases only after terminal descendant settlement. Covers REQ-005 through
REQ-010.

### AC-003 — Scribe and Forge authority transitions

Focused integration evidence with real Postgres, Iceberg, and object storage
proves exact Scribe visibility, unchanged promotion, managed rewrite handoff,
fenced publication, conflict and uncertainty handling, restart reconciliation,
expiration, cleanup, and hard reader roots. Covers REQ-011 through REQ-015.

### AC-004 — Contract, migration, and generated closure

Source contract review, migration verification, code generation, protobuf
drift checks, SDK typing checks, and public surface tests prove both contract
families coexist without a legacy public alias or hand-edited generated output.
Covers REQ-016 through REQ-018.

### AC-005 — Closed minimal telemetry catalogs

Production-emission tests, closed-catalog tests, gauge-balance tests, operator
documentation parity, and telemetry-first failure capture prove the Forge,
Oracle, and Scribe catalogs satisfy REQ-024 through REQ-028. Evidence shall
show that each retained metric has a named alert, service-level, capacity,
backlog, pressure, or balance purpose and that removed detail remains
diagnosable through production traces, logs, audit, or durable state.

### AC-006 — Standalone single-tenant journey

The existing process harness runs one real all-role server process. A real
public client registers, writes, receives acknowledged durability, waits for
Scribe publication and Forge promotion/rewrite, and reads exact rows through
Interactive Oracle before and after maintenance. Covers the first row of
REQ-030 and REQ-031.

### AC-007 — Standalone multi-tenant journey

The same existing process harness proves two or more tenants with the same
logical table name can write, maintain, and read concurrently without shared
physical authority, cross-tenant rows, metric labels, or audit attribution.
Covers the second row of REQ-030 and tenant aspects of REQ-032.

### AC-008 — Distributed single-tenant journey

The existing process harness runs real role-targeted child processes and uses
the public write and query surfaces. Refactored Forge output is read exactly by
both Interactive and selected Analytical Oracle execution, including peer
settlement and cleanup. Covers the third row of REQ-030 and the previously
unproven Forge-to-distributed-Oracle seam.

### AC-009 — Distributed multi-tenant journey

The existing process harness proves concurrent same-name tenant writes,
maintenance, Interactive and Analytical reads, retained-reader safety, and
explicit tenant-tripwire failure across real process boundaries. Covers the
fourth row of REQ-030 and tenant aspects of REQ-032.

### AC-010 — Failure and recovery closure

The strongest appropriate process journey or real-dependency integration tests
prove the replay, authorization, registration, admission, peer, deadline,
resource, lease, conflict, uncertainty, restart, cleanup, reader-race, and
tripwire outcomes in REQ-032 without sleeps, weakened assertions, or test-only
production shortcuts.

### AC-011 — Complete Bifrost verification

`mise run verify:bifrost` passes on the immutable integrated candidate and its
owned aggregate demonstrably includes every retained Bifrost tier and
first-class language surface required by REQ-033 and REQ-034. Every specifically
named test in later tasks also has and passes its exact focused command through
the repository-pinned `mise` toolchain.

### AC-012 — Broad integration verification

Because the change is intentionally broad and changes shared CI/build/test
infrastructure and multiple ownership boundaries, `mise run gate` passes on
the same immutable integrated candidate. `git diff --check`, final generated
drift inspection, and an immutable base-to-candidate review also pass.

## Open material decisions

None.

## Planning-decision inventory

After approval, `$wyrd-plan` must decide the minimum implementation mechanics
for:

- freezing the exact clean source and destination commits and preserving both
  inputs' intended history;
- sequencing textual conflict resolution and semantic reconciliation while
  keeping reviewable, buildable integration boundaries;
- adapting durable Oracle reader identity, protection, and release into the
  destination's one-root planner and distributed graph lifecycle;
- combining governed catalog IO with permit-gated reader IO;
- reconciling Scribe live-tail changes and proving one authority decision for
  active, immutable, staged, hot, promoted, and rewritten sources;
- composing Forge and Oracle boot, readiness, shutdown, and recovery owners;
- reconciling typed contracts, peer versions, durable SQL state, migration
  ordering, and version-aware historical decoding before regeneration;
- conforming source Forge output identity to the authoritative flat
  attempt-global ordinal contract;
- extending the existing `BifrostProcessCluster` with existing role targets and
  Forge scenarios without introducing a new harness or wrapper;
- allocating the four required topology/tenancy obligations among the minimum
  number of maintainable journey scenarios;
- selecting the exact retained Oracle and Scribe metric family names that meet
  REQ-024, REQ-026, and REQ-027 and mapping each to operator documentation and
  production emission evidence;
- migrating any still-required assertion before deleting an obsolete test or
  telemetry report representation;
- reconciling the destination's other approved active change packets and the
  source change-packet cleanup without silently discarding authority;
- choosing exact focused test commands, broader lanes, and dependency ordering
  for scenario-by-scenario Red-Green-Refactor execution; and
- preserving normal Iceberg manifest creation while leaving periodic
  metadata-only manifest rewriting outside this change.

These decisions may choose private mechanics but may not change the required
behavior, public outcomes, tenant and audit semantics, deletion obligations,
or acceptance evidence in this specification.

## Revision history

- **Revision 2 — 2026-09-06 — draft.** Records the human decision not to add
  periodic metadata-only manifest rewriting back during this integration.
  Normal snapshot manifest creation remains required; the deleted inactive
  rewrite route remains prohibited. No open material decisions remain.
- **Revision 1 — 2026-09-06 — draft.** Initial integration specification from
  repository comparison and human direction. Fixes merge direction, subsystem
  authority, mandatory deletion, telemetry simplification, use of the existing
  process harness, and the production topology/tenancy journey matrix. Leaves
  manifest-rewrite delivery as the sole open material decision.

## Material authority and evidence links

- [`AGENTS.md`](../../../AGENTS.md)
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md)
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md)
- [`architecture/wyrd-doctrine.mdx`](../../../architecture/wyrd-doctrine.mdx)
- [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md)
- [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md)
- [`architecture/operations/deployment-and-release.md`](../../../architecture/operations/deployment-and-release.md)
- [`architecture/operations/reliability-and-recovery.md`](../../../architecture/operations/reliability-and-recovery.md)
- [`architecture/references/languages/spec-driven-development.md`](../../../architecture/references/languages/spec-driven-development.md)
- [`architecture/references/languages/implementation-execution.md`](../../../architecture/references/languages/implementation-execution.md)
- [`architecture/references/languages/testing-workflows.md`](../../../architecture/references/languages/testing-workflows.md)
- [`architecture/references/domain/vala-architecture.md`](../../../architecture/references/domain/vala-architecture.md)
- [`architecture/references/domain/telemetry-observations.md`](../../../architecture/references/domain/telemetry-observations.md)
- [`architecture/references/domain/olap-serving.md`](../../../architecture/references/domain/olap-serving.md)
- [`architecture/references/domain/iceberg.md`](../../../architecture/references/domain/iceberg.md)
- [`architecture/references/domain/datafusion.md`](../../../architecture/references/domain/datafusion.md)
- [`architecture/references/domain/analytical-operations-reliability.md`](../../../architecture/references/domain/analytical-operations-reliability.md)
- [`BifrostProcessCluster`](../../../crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs)
- [`Telemetry-first tiered-test debugging`](../telemetry-first-tiered-test-debugging/spec.md)
- [`Scribe journey and staged-member retry recovery`](../scribe-journey-retry-recovery/spec.md)
