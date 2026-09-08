---
id: SPEC-bifrost-forge-oracle-integration
revision: 3
status: approved
---

# Integrate Forge compaction into distributed Oracle

## Objective

Merge `forge-compaction-refactor` into `oracle-distributed` so the destination
retains its authoritative Gate, Scribe, Oracle, public-query, server, and test
architecture while incorporating the source branch's Forge implementation.

This is an integration and reconciliation change. It does not authorize a new
Oracle, Scribe, Gate, telemetry, testing, or public-contract design.

## User and operator value

After integration, the existing Bifrost write, maintenance, and query system
works as one coherent implementation:

```text
public write -> Gate -> Scribe -> Forge -> Iceberg
public read  -> Gate -> Oracle -> result
```

Existing Scribe, Forge, Oracle, server, SDK, and Bifrost behavior remains
covered by the repository's tier 1-3 tests and journeys.

## Authority

### REQ-001 - Integration direction

`oracle-distributed` is the integration destination.
`forge-compaction-refactor` is the merge source.

The implementation task shall record the exact clean commit used for each
input before integration begins.

### REQ-002 - Component authority

Conflict resolution shall use this authority order:

| Surface | Authority |
|---|---|
| Forge implementation and Forge-owned durable behavior | `forge-compaction-refactor` |
| Gate | `oracle-distributed` |
| Scribe | `oracle-distributed` |
| Oracle | `oracle-distributed` |
| Public query behavior and contracts | `oracle-distributed` |
| Server topology and process composition | `oracle-distributed` |
| Existing test hierarchy and lane definitions | `oracle-distributed` |
| Shared seams required to compose Forge | Reconcile with destination semantics preserved |
| Repository architecture and rules | Current authoritative repository documents |

A source-branch change outside Forge is not authoritative merely because it
exists on the source branch.

### REQ-003 - Surface overlapping changes

Before resolving semantic overlap, the integration shall identify every
source-branch change outside Forge and every shared file changed by both
branches.

Each overlap shall be classified as:

- `RETAIN`: required and already compatible with destination behavior;
- `ADAPT`: required for Forge, but must be expressed through the destination
  architecture;
- `DROP`: stale, duplicated, superseded, unrelated, or unnecessary; or
- `DECISION_REQUIRED`: cannot be resolved without changing approved behavior,
  architecture, a public contract, durable state, security, or compatibility.

The reconciliation evidence shall identify:

- the affected surface;
- the destination behavior;
- the source change and why it exists;
- whether Forge requires it;
- the recommended classification; and
- the repository or verification evidence supporting that recommendation.

`DECISION_REQUIRED` stops the affected integration work and returns to
`$wyrd-spec`. The merge shall not silently select or combine competing
semantics.

### REQ-004 - Destination behavior wins outside Forge

Gate, Scribe, Oracle, public query, server topology, SDK behavior, and existing
test-lane semantics shall remain those of `oracle-distributed` unless a source
change is demonstrably required to compose Forge without altering those
behaviors.

Source Oracle changes shall not reintroduce or replace planning, routing,
execution, admission, retry, fallback, peer, terminal, or query-resource
semantics owned by the destination.

Source Scribe changes shall be retained only when Forge requires the seam and
the change preserves destination ingestion, acknowledgement, replay, live-tail,
shutdown, and recovery behavior.

### REQ-005 - Incorporate the Forge implementation

The integrated candidate shall contain the source branch's current Forge
architecture, including its required:

- promotion and managed rewrite behavior;
- scheduling, leases, fences, and publication;
- reconciliation and uncertain-operation handling;
- reader-safe retention, expiration, and cleanup;
- Forge-owned durable SQL state;
- resource and readiness integration;
- production instrumentation; and
- Forge tests and journeys.

This requirement incorporates the completed Forge implementation. It does not
authorize reimplementing or redesigning it during the merge.

Any source Forge mechanic found incompatible with destination authority shall
be surfaced as `DECISION_REQUIRED` rather than replaced speculatively.

### REQ-006 - Reconcile shared seams minimally

Shared catalog, storage, SQL, resource, server-composition, readiness,
shutdown, audit, reader-protection, contract, and test-harness seams shall be
changed only as much as required to compose the retained Forge implementation
with the destination architecture.

The integrated result shall not contain:

- two implementations of the same behavior;
- a compatibility adapter for a superseded branch shape;
- a second planner, harness, resource owner, or lifecycle;
- source-only configuration unused by retained behavior; or
- stale code retained solely to avoid resolving a conflict.

### REQ-007 - Contracts, migrations, and generated artifacts

Destination public Oracle and Bifrost contracts remain authoritative.

Source contract or durable-state additions may be retained when they are
required by the incorporated Forge implementation and do not replace or
reinterpret destination behavior.

Historical migrations shall not be deleted or rewritten. Generated schemas,
OpenAPI, SDK projections, and protobuf descriptors shall be regenerated from
the reconciled owning sources; generated conflicts shall not be resolved by
choosing one branch's generated file.

### REQ-008 - Preserve the existing test hierarchy

The destination's Bifrost test organization, process harness, tier definitions,
and `mise` lane meanings remain authoritative.

Source Forge coverage shall be retained within the existing Forge unit,
integration, and journey owners. Existing destination Scribe, Oracle, server,
SDK, MCP, Python, TypeScript, OTLP, and public-query coverage shall remain.

A test may be removed only when it exclusively covers behavior removed under
this specification and a stronger surviving test proves every still-required
behavior.

No test, assertion, lane, feature selection, timeout, or public path may be
weakened to make the integration pass.

## Constraints

- Preserve `architecture/bifrost-design.md`.
- Preserve the destination Oracle architecture exactly unless a later approved
  revision explicitly changes it.
- Preserve the source Forge architecture rather than recreating it.
- Reuse the existing Bifrost process and test harnesses.
- Do not add a dependency, Cargo feature, compatibility layer, public API, or
  alternate execution path merely to resolve the merge.
- Do not redesign Gate, Scribe, Oracle, or their telemetry.
- Do not hand-edit generated artifacts.
- Keep tenant isolation, audit, durability, readiness, bounded-resource, and
  failure semantics intact.
- Keep source changes outside Forge only when supported by explicit
  reconciliation evidence.

## Non-goals

- New Bifrost behavior unrelated to integrating Forge.
- Oracle feature development or reader-authority redesign.
- Scribe ingestion, live-tail, or recovery redesign.
- Gate redesign.
- Oracle or Scribe telemetry simplification.
- A new Bifrost test harness or topology framework.
- Restoring benchmark or qualification machinery removed by the destination.
- Preserving source-branch mechanics that the destination superseded.
- Expanding Forge beyond the implementation already completed on the source
  branch.

## Acceptance criteria

### AC-001 - Complete overlap assessment

Every source change outside Forge and every shared surface changed by both
branches has a `RETAIN`, `ADAPT`, `DROP`, or `DECISION_REQUIRED`
recommendation with evidence.

No unresolved semantic overlap is hidden by a textual conflict resolution or
successful compilation.

### AC-002 - Authority preserved

Diff review demonstrates:

- destination Gate behavior remains;
- destination Scribe behavior remains except for justified Forge seams;
- destination Oracle architecture remains;
- destination public query and server behavior remains; and
- source Forge behavior is incorporated.

### AC-003 - One integrated implementation

The candidate contains one Gate, one Scribe architecture, one Oracle
architecture, and one Forge architecture. No stale branch implementation,
compatibility route, unused configuration, duplicate lifecycle, or duplicate
test harness remains.

### AC-004 - Forge integration

The source Forge unit, integration, recovery, publication, expiration,
cleanup, reader-safety, and production-journey obligations pass through the
integrated destination.

### AC-005 - Tier 1 journeys

The complete existing Bifrost journey lane passes, including the dedicated:

- Scribe journey lane;
- Forge journey lane;
- Oracle journey lane; and
- server, SDK, MCP, OTLP, Python, and TypeScript lanes.

Journeys exercise their existing production-shaped public paths.

### AC-006 - Tier 2 integration

The complete Bifrost Redux, SQL, and server integration lanes pass on the same
integrated candidate.

### AC-007 - Tier 3 units

The complete Bifrost Rust, Python, and TypeScript unit surfaces pass on the
same integrated candidate.

### AC-008 - Complete Bifrost verification

`mise run verify:bifrost` passes on the immutable integrated candidate.

Any broader repository gate required by the final changed surface under
`AGENTS.md` also passes.

### AC-009 - Independent acceptance review

Independent review confirms that the final diff:

- incorporated Forge;
- preserved destination authority outside Forge;
- resolved every recorded overlap consistently;
- introduced no unrelated behavior;
- weakened no required test or lane; and
- contains no avoidable compatibility or duplicate implementation.

## Open material decisions

None before overlap analysis.

Any overlap that requires changing approved destination behavior becomes a new
material decision and requires a draft spec revision before implementation
continues.

## Revision history

- **Revision 3 - 2026-09-08 - approved.** Reframes the change as branch
  integration, makes `oracle-distributed` authoritative outside Forge,
  requires explicit overlap classification, and makes the complete Bifrost
  tier 1-3 hierarchy the acceptance gate.
- **Revision 2 - 2026-09-06 - draft.** Excluded periodic metadata-only
  manifest rewriting.
- **Revision 1 - 2026-09-06 - draft.** Initial integration specification.

## Material authorities

- [`AGENTS.md`](../../../AGENTS.md)
- [`architecture/agent-rules.md`](../../../architecture/agent-rules.md)
- [`architecture/wyrd-design.md`](../../../architecture/wyrd-design.md)
- [`architecture/bifrost-design.md`](../../../architecture/bifrost-design.md)
- [`architecture/wyrd-security-posture.md`](../../../architecture/wyrd-security-posture.md)
- [`architecture/operations/reliability-and-recovery.md`](../../../architecture/operations/reliability-and-recovery.md)
- [`architecture/references/languages/testing-workflows.md`](../../../architecture/references/languages/testing-workflows.md)
