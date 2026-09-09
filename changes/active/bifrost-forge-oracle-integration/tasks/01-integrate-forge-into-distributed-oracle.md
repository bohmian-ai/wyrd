---
id: BIFROST-FORGE-ORACLE-T01
title: Integrate the locked Forge candidate into distributed Oracle
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-bifrost-forge-oracle-integration
spec_revision: 5
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011]
---

# Integrate Forge into distributed Oracle

## Objective

Merge the locked Forge implementation candidate
`b35ebb5e76b7a63fc7e9549329a5c90101400002`, using its evidence-complete source
tip `351902b0855c69849a88e7a76c9e596a666f716d`, into the distributed Oracle
destination assessed at `b51eb83619defe14a11813e1e35e8f0fd666f77a`.
Produce one coherent Bifrost implementation that retains destination-owned
Gate, Scribe, Oracle, public-query, server, SDK, and test behavior while adding
the approved Forge implementation and its required reader-protection seams.

Required execution skill: `$wyrd-implement`.

## Constraints

- Begin from clean, exact source and destination commits. If either input has
  changed, refresh the overlap inventory before integration. A changed Forge
  implementation candidate requires a new approved specification revision.
- Resolve conflicts by the component and semantic-hunk authority in approved
  specification revision 5. Do not use whole-file branch preference for mixed
  owners.
- Preserve the destination implementation outside Forge except for the minimum
  seams required to compose the locked Forge behavior. Do not introduce a
  second planner, lifecycle, resource owner, harness, or compatibility path.
- Preserve every historical destination migration. Express any required Forge
  schema delta through the next forward-only migration, and reconcile the
  source reader-authority migration with the destination schema and ordering.
- Regenerate schemas, OpenAPI, SDK projections, and protobuf descriptors from
  their reconciled owning sources. Do not resolve generated files by selecting
  one branch artifact.
- Keep destination-deleted benchmark, calibration, obsolete MCP, workflow, and
  unrelated planning machinery deleted. Do not weaken tests, lane definitions,
  feature selections, assertions, or timeouts to obtain a pass.
- Do not fix or redesign Oracle admission in this merge. Its known pre-existing
  failure or Forge closeout workaround may remain when evidence ties it to
  `changes/active/oracle-local-admission`; any unrelated failure blocks
  acceptance.
- Stop and return to `$wyrd-spec` if reconciliation would change approved
  destination behavior, the locked Forge semantics, a public contract,
  durable-state meaning, security, compatibility, or concurrency semantics.

## Relevant Surface

Paths are guidance, not an implementation allowlist.

- Approved authority and overlap evidence in
  `changes/active/bifrost-forge-oracle-integration/spec.md`, plus
  `architecture/agent-rules.md`, `architecture/wyrd-design.md`,
  `architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`,
  `architecture/wyrd-security-posture.md`, and
  `architecture/operations/reliability-and-recovery.md`.
- Workspace manifests and lockfile entries governing the single DataFusion,
  Arrow, Parquet, Iceberg, and pinned managed-compaction dependency universe.
- `vala-bifrost-redux` Forge, Oracle reader protection, Scribe integration,
  catalog, Parquet, resource, maintenance, readiness, and shutdown owners.
- `vala-sql` migrations, queries, and row projections required by retained
  Forge durable behavior.
- `wyrd-spec` private coordination, audit, and public Bifrost contract owners.
- `wyrd-server` boot, process composition, health, MCP, OTLP, and shutdown
  consumers.
- `wyrd-testing` harnesses and the existing Bifrost unit, integration, and
  user-journey owners across Rust, Python, TypeScript, SDK, MCP, and OTLP.
- Generated schema, OpenAPI, SDK, and protobuf outputs owned by reconciled
  source contracts.

## Approach

1. Verify and record the clean integration inputs. Refresh the specification's
   overlap counts and classifications only if an assessed input changed.
2. Integrate the source tip and resolve every recorded shared or source-only
   surface against the specification's `RETAIN`, `ADAPT`, and `DROP` evidence,
   escalating any newly discovered material conflict.
3. Reconcile Forge-owned durable behavior, reader protection, dependencies,
   contracts, and SQL state with the destination owners, preserving historical
   migrations and adding only required forward state transitions.
4. Adapt the minimum shared Scribe, Oracle, server, readiness, resource,
   shutdown, audit, and test seams so the retained Forge implementation runs
   within the destination topology and authority model.
5. Regenerate owned artifacts, remove stale branch residue and duplicate
   implementations, then exercise only the focused Forge, Oracle, Scribe, and
   Gate test surfaces plus applicable non-test formatting and contract checks.
6. Record the exact commands and results on one clean candidate, bound any
   surfaced Oracle admission failure against the known pre-merge defect, and
   audit the final diff against every overlap classification and acceptance
   criterion.

## Acceptance Criteria

- Every source-only and shared change in the approved overlap assessment is
  resolved consistently, with no hidden semantic conflict or unexplained
  branch residue.
- The integrated tree contains one destination-owned Gate, Scribe, Oracle,
  public-query, server, SDK, and test architecture and one source-owned Forge
  implementation.
- Locked Forge behavior survives integration: real-plan FIFO admission without
  a Forge DataFusion ceiling or spill path; independent per-plan publication;
  bounded 1s/2s/4s definite-conflict retries; exact-operation uncertain
  reconciliation; independent maintenance protocols; reader-safe expiration
  and cleanup; and fail-closed readiness retraction.
- The exact managed compaction pin resolves inside the destination's single
  native analytical dependency universe.
- Destination Oracle planning, routing, execution, terminal, peer, admission,
  resource, and public-query semantics remain intact. Destination Scribe
  acknowledgement, replay, live-tail, recovery, and shutdown behavior remain
  intact except for justified Forge composition seams.
- Destination migration history is unchanged. Required Forge durable state is
  represented by reconciled forward-only migrations, with no duplicate or
  contradictory schema authority.
- Generated schemas, OpenAPI, SDK projections, and protobuf descriptors match
  the reconciled owning contracts without manual branch-artifact selection.
- Existing Forge, Oracle, Scribe, server, SQL, SDK, MCP, OTLP, Python, and
  TypeScript coverage remains intact without weakening. Required execution is
  limited to the focused Forge, Oracle, Scribe, and Gate tests.
- The focused lanes have no failure other than an exact manifestation of the
  known pre-existing Oracle admission defect recorded against
  `changes/active/oracle-local-admission`.
- Independent task review can map the final diff and verification evidence to
  all `AC-001` through `AC-011` and finds no unrelated behavior, compatibility
  layer, duplicate lifecycle, or avoidable implementation.

## Verification

Finish on one immutable integrated candidate with only these focused Bifrost
tests:

```bash
mise run test:bifrost:journey:forge
mise run test:bifrost:journey:scribe
mise run test:bifrost:journey:oracle
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^gate::/)'
```

Also run only the applicable non-test checks:

```bash
mise run fmt
mise run lints
mise run codegen:check
mise run check:object-store-pin
git diff --check
```

Do not run `mise run check:tenant-isolation`, `mise run
test:bifrost:integration:redux`, `mise run test:bifrost:integration:server`,
`mise run test:bifrost:journey`, `mise run verify:bifrost`, or `mise run gate`
for this merge. If Oracle admission surfaces as a focused-test failure, record
its test name and failure signature as the sole permitted known failure.
