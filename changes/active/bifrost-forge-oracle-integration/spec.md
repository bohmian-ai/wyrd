---
id: SPEC-bifrost-forge-oracle-integration
revision: 5
status: approved
---

# Integrate Forge compaction into distributed Oracle

## Objective

Merge the locked `forge-compaction-refactor` candidate
`b35ebb5e76b7a63fc7e9549329a5c90101400002` into `oracle-distributed` so the
destination retains its authoritative Gate, Scribe, Oracle, public-query,
server, and test architecture while incorporating the source branch's Forge
implementation and its required reader-protection seams.

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

The source implementation candidate is locked at
`b35ebb5e76b7a63fc7e9549329a5c90101400002`, based on approved
`SPEC-forge-task-5-production-closeout` revision 10. The merge-source tip is
`351902b0855c69849a88e7a76c9e596a666f716d`, whose only additional change is
the candidate-bound `PASS` verdict. A different implementation commit requires
a refreshed overlap assessment and explicit approval of this specification.

The destination was assessed at
`b51eb83619defe14a11813e1e35e8f0fd666f77a`. The implementation task shall
record the exact clean destination commit used and rerun the overlap inventory
if that commit changes before integration begins.

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

Authority applies by semantic hunk, not by whole-file selection. Mixed files
such as `architecture/bifrost-design.md`, `wyrd-spec::vala::api`, server boot,
and the Bifrost harness must preserve destination-owned behavior while
incorporating only the Forge-owned or Forge-required source semantics defined
by this revision.

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

The revision 4 assessment below is the baseline reconciliation evidence. The
implementation task must refresh it only if either recorded input commit
changes.

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

The locked Forge behavior specifically includes:

- one process-global Oracle reader-authority context per fenced role epoch,
  table-scoped durable protection, and serialization against snapshot
  expiration before source IO;
- real managed-core plans before admission, strict worker-local FIFO admission
  against aggregate estimated running memory, and no hard Forge DataFusion
  allocation ceiling, spill path, or scratch requirement;
- the source branch's exact pinned `iceberg-compaction-core` dependency at
  `3709a1d9f7b8b6f2c0ed886c105fb557f1aaadab`, resolved in the destination's
  single DataFusion 55, Arrow/Parquet 59, and Iceberg dependency universe;
- independent execution and publication of ordinary compaction plans, with
  committed sibling progress preserved and remaining debt replanned from the
  current table head;
- per-plan definite-conflict retries after 1s, 2s, and 4s, bounded by the
  existing deadline and authority, while unknown acceptance reconciles the
  exact existing operation without retrying the ambiguous catalog call;
- independent promotion, compaction, snapshot-expiration, expired-cleanup, and
  orphan-cleanup protocols; and
- readiness retraction before unresolved durable authority can block release,
  followed by fail-closed recovery.

This requirement incorporates the completed Forge implementation. It does not
authorize reimplementing or redesigning it during the merge.

Any source Forge mechanic found incompatible with destination authority shall
be surfaced as `DECISION_REQUIRED` rather than replaced speculatively.

### REQ-006 - Reconcile shared seams minimally

Shared catalog, storage, SQL, resource, server-composition, readiness,
shutdown, audit, reader-protection, contract, and test-harness seams shall be
changed only as much as required to compose the retained Forge implementation
with the destination architecture.

Source Oracle code is authoritative only for the reader-protection capability
Forge requires. It must be adapted to the destination's single-planner,
root-derived Interactive/Analytical selection, one-attempt terminal behavior,
peer authority, query-owned resources, and current server topology. Source
query routing, fallback, retry, EXPLAIN, admission, resource, or terminal
semantics must not replace the destination implementation.

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

The source edits existing migration files `20260910000009`, `00010`, `00019`,
`00020`, and `00022`, and deletes `00016`. Those edits shall not replace the
destination migration history. Preserve the destination files and express any
still-required Forge end-state delta through the next forward-only migration.
The source-only `20260910000025_oracle_reader_authority.sql` may be retained
only after it is reconciled with the destination schema and migration order.

### REQ-008 - Preserve the test hierarchy and verify only affected capabilities

The destination's Bifrost test organization, process harness, tier definitions,
and `mise` lane meanings remain authoritative.

Source Forge coverage shall be retained within the existing Forge unit,
integration, and journey owners. Existing destination Scribe, Oracle, server,
SDK, MCP, Python, TypeScript, OTLP, and public-query coverage shall remain.

Merge acceptance requires only the focused Bifrost Forge, Oracle, Scribe, and
Gate test surfaces. It does not require the complete Redux or server integration
suites, the aggregate Bifrost journey lane, `mise run verify:bifrost`, or
`mise run gate`.

The known pre-existing Oracle admission failure is outside this merge and is
tracked by `changes/active/oracle-local-admission`. Its existing failure or
Forge closeout workaround may remain and does not block this integration. All
four focused lanes shall still run to completion; any failure not attributable
to that exact known defect blocks acceptance. This merge shall not alter Oracle
admission to make the known defect green.

A test may be removed only when it exclusively covers behavior removed under
this specification and a stronger surviving test proves every still-required
behavior.

No test, assertion, lane, feature selection, timeout, or public path may be
weakened to make the integration pass.

## Verified pre-merge assessment

### Assessed graph

| Input | Commit |
|---|---|
| Merge base | `24b8349e3eae96a8901ac29aac93acf2a20705ff` |
| Destination | `b51eb83619defe14a11813e1e35e8f0fd666f77a` |
| Locked Forge implementation | `b35ebb5e76b7a63fc7e9549329a5c90101400002` |
| Forge merge-source tip | `351902b0855c69849a88e7a76c9e596a666f716d` |

The source changes 275 files from the merge base, the destination changes 422,
and 102 files are shared. `git merge-tree --write-tree --name-only --messages`
predicts 33 textual conflicts. A clean textual merge is therefore not credible
evidence of semantic correctness for the remaining 69 shared files.

### Reconciliation ledger

| Surface | Source reason | Forge required | Classification | Resolution |
|---|---|---:|---|---|
| `vala-bifrost-redux/src/forge/**`, Forge SQL/query owners, Forge integration tests, and Forge journeys | Locked Forge implementation | Yes | `RETAIN` | Incorporate the source behavior, adapting only its destination-facing seams. |
| Oracle reader epoch, reader pins, protection frontier, follower cut, and expiration serialization | Prevent deletion while admitted readers may perform source IO | Yes | `ADAPT` | Port the capability into destination Oracle without importing source routing, fallback, retry, EXPLAIN, resource, or terminal semantics. |
| Scribe hot-object promotion and live-tail lifetime seams | Supply Forge promotion inputs and cleanup roots safely | Yes | `ADAPT` | Preserve destination Scribe ownership, acknowledgement, replay, live-tail, and shutdown behavior. |
| Catalog, Parquet, storage, resources, audit, readiness, and server composition | Compose Forge production ownership | Yes | `ADAPT` | Use destination owners and topology; add only the locked Forge capability. |
| Workspace and crate manifests | Forge introduces the pinned managed compaction core while the destination adds Oracle, MCP, TLS, and harness dependencies | Partly | `ADAPT` | Retain the exact Forge core pin and every destination dependency still used; regenerate one lockfile without a duplicate native analytical universe. |
| Public Bifrost query/table contracts in `wyrd-spec::vala::api`, HTTP, MCP, SDK, and docs | Source branch predates destination public-query work | No | `DROP` | Keep destination contracts and regenerate projections. Retain only Forge audit types and private reader-assignment data required by the locked source. |
| Source Oracle planning, routing, retry/fallback, EXPLAIN, admission, peer, resource, and terminal changes | Older Oracle architecture on the Forge branch | No | `DROP` | Keep destination Oracle exactly. |
| Shared architecture documents | One file contains both newer Forge text and older Oracle/Gate/Scribe text | Partly | `ADAPT` | Preserve destination non-Forge sections and reconcile only Forge, reader-protection, maintenance, and Forge reliability statements to revision 10. |
| Source-only Iceberg, security, deployment, and operations authority updates | Record Forge compaction, reader safety, readiness, and recovery | Yes in Forge-specific hunks | `ADAPT` | Retain Forge-specific authority without importing unrelated source-era behavior. |
| Existing migrations `00009`, `00010`, `00016`, `00019`, `00020`, `00022` | Source reshaped unshipped Forge and reader state in place | End state only | `ADAPT` | Keep destination history; add a forward-only reconciliation migration for required final state. |
| `00025_oracle_reader_authority.sql` and its SQL owners | Durable reader/expiration mutual exclusion | Yes | `ADAPT` | Retain after schema, RLS, role, and destination Oracle integration review. |
| Deleted destination MCP module and benchmark/calibration machinery | Source modified owners the destination superseded or removed | No | `DROP` | Keep deletions; do not add compatibility owners. |
| Generated JSON schemas, schema goldens, OpenAPI projections, and `wyrd.v1.bin` | Derived output changed on both branches | Derived | `ADAPT` | Regenerate from reconciled sources; never hand-merge. |
| Workflow skills, Claude mirrors, `AGENTS.md`, `changes/README.md`, and spec-development references | Both branches independently advanced repository workflow | No Forge runtime need | `DROP` | Keep destination authority. The Forge packet remains evidence but does not roll repository workflow backward. |
| `changes/active/server-topology-readiness/spec.md` | Separate source-side change packet | No | `DROP` | Do not import an unrelated active change. |
| Source OTLP adapters, generic SQL/storage exports, check scripts, and nextest settings | Exhaustive-match, boundary, or test-support consequences of Forge work | Sometimes | `ADAPT` | Retain only compile-, security-, or Forge-test-required changes after comparison with destination owners. |

This ledger exhausts the source-only non-Forge surfaces and the 102 shared
files by owner family. No assessed surface requires a new product or public API
decision after revision 4 is approved.

### Predicted textual conflicts

The 33 conflicts fall into five resolution groups:

- **Destination repository authority:** `.agents/skills/wyrd-task-review/SKILL.md`,
  `.claude/skills/wyrd-task-review/SKILL.md`, `AGENTS.md`,
  `architecture/references/languages/spec-driven-development.md`, and
  `changes/README.md` retain the destination versions.
- **Mixed architecture:** `architecture/bifrost-design.md`,
  `architecture/operations/reliability-and-recovery.md`,
  `architecture/references/domain/analytical-operations-reliability.md`, and
  `architecture/references/domain/datafusion.md` keep destination Oracle,
  Gate, Scribe, and public-query semantics while adopting locked Forge and
  reader-protection semantics.
- **Engine and contracts:** `catalog/bifrost_catalog.rs`, Oracle `follower.rs`,
  `mod.rs`, `planner.rs`, and `query_stream.rs`, plus `wyrd-spec` `vala/api.rs`
  and `vala/mod.rs`, require semantic adaptation. Destination Oracle and public
  contracts win; locked reader protection and Forge audit detail are retained.
- **Server:** the deleted `wyrd-mcp/src/bifrost/mod.rs` stays deleted. Server
  `app/mod.rs`, `boot/mod.rs`, `components/health/mod.rs`, Oracle
  `forwarding.rs`, `state.rs`, and the two Postgres smoke tests keep destination
  topology and receive only required Forge/readiness seams.
- **Harness and generated output:** deleted `bench_families.rs` and
  `calibration.rs` stay deleted. `cluster.rs`, `forge_harness.rs`, `mod.rs`,
  `telemetry.rs`, `server.rs`, and Oracle `distributed.rs` adapt Forge proof to
  the destination process harness. `wyrd.v1.bin` is regenerated.

### Merge-task precondition

The source change packet remains immutable evidence for the locked Forge
candidate; the integration change records its own evidence under this packet.
The candidate-bound `TASK-061-R4-review-04/verdict.md` is committed at the
merge-source tip and records `PASS` for `b35ebb5e7`.

## Constraints

- Reconcile `architecture/bifrost-design.md` at the semantic-hunk level: keep
  destination Gate, Scribe, Oracle, and public-query authority and adopt the
  locked Forge and reader-protection authority described by revision 4.
- Preserve the destination Oracle planning, routing, execution, admission,
  peer, resource, retry, terminal, and public behavior; add only the locked
  reader-protection seam required to make Forge cleanup safe.
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

### AC-005 - Focused capability verification

The dedicated Forge, Scribe, and Oracle journey lanes and the Gate module tests
run to completion. They pass except for the bounded known Oracle admission
failure allowed by AC-006.

No complete Redux, server, aggregate journey, SDK, MCP, OTLP, Python, or
TypeScript lane is required for this merge.

The selected journeys continue to exercise their existing production-shaped
public paths.

### AC-006 - Known Oracle admission failure is bounded

If the defect surfaces as a selected-test failure, the implementation evidence
names the exact test and failure signature, demonstrates that it matches the
pre-merge Oracle admission defect, and links it to
`changes/active/oracle-local-admission`. Retaining its existing Forge closeout
workaround is also permitted. Any different or additional failure blocks
acceptance.

### AC-007 - Existing coverage is not weakened

No existing test, assertion, lane, feature selection, timeout, or public path
is weakened, deleted, or bypassed to accommodate the merge or the known Oracle
admission failure.

### AC-008 - Scoped merge verification

The focused Forge, Oracle, Scribe, and Gate commands in the merge task are the
required test proof on the immutable integrated candidate. `mise run
verify:bifrost` and `mise run gate` are explicitly not required.

### AC-009 - Independent acceptance review

Independent review confirms that the final diff:

- incorporated Forge;
- preserved destination authority outside Forge;
- resolved every recorded overlap consistently;
- introduced no unrelated behavior;
- weakened no required test or lane; and
- contains no avoidable compatibility or duplicate implementation.

### AC-010 - Locked Forge semantics survive integration

Focused and journey evidence proves real-plan FIFO admission, no Forge
DataFusion ceiling or spill path, independent per-plan publication, bounded
1s/2s/4s definite-conflict retries, exact-operation uncertain reconciliation,
reader-safe expiration and cleanup, and readiness retraction before unresolved
release. Dependency evidence proves the locked managed core resolves in the
destination's one native analytical universe. No destination Oracle execution
behavior is replaced to obtain that proof.

### AC-011 - Durable and generated state is reconciled from owners

Migration review proves the destination history is unchanged and any required
Forge schema transition is forward-only. Code generation proves schemas,
OpenAPI, SDK projections, and protobuf descriptors derive from the reconciled
source contracts rather than a selected branch artifact.

## Open material decisions

None. The human owner approved revision 5's narrowed verification contract and
known Oracle admission exception on 2026-09-09.

Any overlap that requires changing approved destination behavior becomes a new
material decision and requires a draft spec revision before implementation
continues.

The source review-evidence precondition is satisfied by merge-source tip
`351902b08`.

## Revision history

- **Revision 5 - 2026-09-09 - approved.** Explicitly approved by the human
  owner. Requires only focused Forge, Oracle, Scribe, and Gate tests; removes
  the complete Redux, server, aggregate journey, `verify:bifrost`, and
  repository `gate` requirements; and bounds the known pre-existing Oracle
  admission failure for later work under `oracle-local-admission`.
- **Revision 4 - 2026-09-09 - approved.** Explicitly approved by the human
  owner. Locks the Forge implementation candidate at `b35ebb5e7` and its
  evidence-only merge-source tip at `351902b08`, records the refreshed
  275/422-file branch deltas, 102 shared files, and 33
  predicted conflicts, resolves mixed authority by semantic hunk, makes locked
  Forge revision 10 behavior explicit, requires forward-only migration
  reconciliation, and records the committed source `PASS` verdict as satisfied
  pre-task evidence.
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
