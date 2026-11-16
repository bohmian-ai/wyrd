---
id: TASK-001A
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 3
requirements: [REQ-005, REQ-014, AC-008]
depends_on: [TASK-001]
---

# Merge the settled verifier pipeline implementation

## Outcome and inputs

Merge the completed verifier pipeline branch into
`task/wyrd-ui-main-integration/TASK-005`, so TASK-002 builds on the real server,
client and compact-result/details contracts rather than the older UI baseline.
Use the existing-read approach in [design notes](../design/notes.md#reopened-ui-only-contract-alignment-2026-11-12)
and [read map](../design/contract-alignment-research.md).

Assume that, when execution starts, all upstream implementation, commits and
verification are settled and its worktree is ready for merge. The source is the
branch checked out in the sibling `wyrd-worktrees/github-verifier-pipelines-task-004`
worktree; that directory name is not the Git branch name. Record its actual
branch and full committed SHA at execution time, plus the destination's starting
SHA. Merge that fixed revision, including its already-integrated resource-scaling
contracts; do not select individual feature commits or chase a moving HEAD.

## Work and boundary

Perform the Git merge and resolve integration conflicts at their existing
owners. Preserve upstream server/SDK behavior and authority amendments, this
packet's approved UI-only scope, and TASK-001's completed definition viewer.
Rebuild generated bindings from their owners rather than editing generated files.
Make only compatibility repairs needed to keep existing consumers working.

This task adds no endpoint, server behavior, migration, authorization rule or
new execution UI. It does not reopen upstream optimization work or repeat its
feature review. TASK-002 still owns runtime graph adaptation, execution panels,
new UI adapters and their real input journeys. If conflict resolution requires
a material behavior or authority decision, report it under the repository's
existing failure-routing rules rather than silently redesigning either branch.

## Verification and handoff

This is integration of existing behavior, so use regression proof rather than
manufacturing a new test. Confirm the merged HEAD contains both recorded inputs,
with no unresolved conflicts. Rebuild the server and Node addons, then prove
the existing UI compiles, builds and retains exact Verifier definition reads
through its authorized hosts and real browser page, including existing refusals.

Run sequentially from the repository root:

```bash
mise run build:server
mise run ts:build:testing
mise run ui:check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t '^an exact Verifier definition and where it runs load through every authorized host$'
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '^an exact Verifier Card page draws its declared graph and where it runs$'
git diff --check
```

`ui:check` builds the public TypeScript addon. Revalidate these runners against
the merged `mise.toml`; record equivalent commands if upstream moved an owner.
Run owner-specific format/lint checks for compatibility repairs. Preserve test
assertions and required real-server dependencies.

Record the merged revision, conflict resolutions, command results and the
upstream evidence for TASK-002's status/details/invocation/publication/file/cancel
and Card/Bifrost inputs in this task's execution evidence. Refresh
[source alignment](../design/source-alignment.md) with the actual integrated
revision. Passing this task establishes the shared integration baseline; it
does not certify the execution inspector or later UI journeys. TASK-002 consumes
that baseline and proves its own reads through the production UI path.

## Obligations

| ID | Promise | Contract source | Planned proof |
|---|---|---|---|
| `M-1` | Merged HEAD contains the recorded upstream revision and the destination start revision, with no unresolved conflicts | Verification and handoff | `git merge-base --is-ancestor` for both; `git diff --check`; conflict-marker scan |
| `M-2` | Conflicts resolve at existing owners, preserving upstream server/SDK behavior and authority amendments, this packet's UI-only scope and TASK-001's definition viewer; generated bindings rebuilt from owners | Work and boundary | conflict resolution log; `codegen:check` |
| `M-3` | No new endpoint, server behavior, migration, authorization rule or execution UI beyond the two inputs; only compatibility repairs | Work and boundary | diff audit of merge result against both parents |
| `REQ-005/AC-008` | Existing UI compiles, builds and keeps exact Verifier definition reads, including refusals, through authorized hosts and the real browser page | Verification and handoff | `build:server`, `ts:build:testing`, `ui:check`, UI build, the two named integration tests |
| `REQ-014` | TASK-002's upstream inputs (status/details/invocation/publication/file/cancel, Card/Bifrost) are identified in the integrated baseline and source alignment is refreshed to the actual revision | Verification and handoff | execution evidence; `design/source-alignment.md` |
