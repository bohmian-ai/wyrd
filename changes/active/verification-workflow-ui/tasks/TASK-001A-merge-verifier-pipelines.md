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

## Execution evidence

### Inputs and merged revision

| Input | Revision |
|---|---|
| Upstream branch (`wyrd-worktrees/github-verifier-pipelines-task-004`) | `task/github-verifier-pipelines/TASK-012-integrate-code-mode` at `f6fac677cd71208871fa5c21e4035085026a99c4` |
| Destination start (`task/wyrd-ui-main-integration/TASK-005`) | `a6a482306e82c8ad6269c30cbfb7cc7e25b24d43` |
| Merge commit | `d118ee49f` (769 upstream commits; merge base `78ca329b4`) |
| Compatibility repair | `0d337a006` |

Deviation from the task's settled-upstream assumption: at execution the upstream
worktree still had uncommitted TASK-012-R1 gateway edits, and `f6fac677c` landed
while its revision was being recorded. The maintainer chose to merge the committed
`f6fac677c` as the fixed revision; the uncommitted R1 work is excluded and the
upstream remediation remains open in its own packet.

Two untracked files predating this task (`wyrd-ui/.g.mjs`, a screenshot harness,
and `wyrd-ui/src/lib/zz-shots.integration.test.ts`) were moved out of the tree,
unchanged, to the session scratchpad so the merge commit did not include them.

### Conflict resolutions

| File | Resolution |
|---|---|
| `changes/spec-driven-development.md` | Upstream text (`f4ca61126`, `ec87d12e1`) supersedes this branch's earlier design-skill wording (`fa4538ae6`) and already carries the design folder lifecycle; taken whole. |
| `crates/wyrd-spec/src/envelope.rs` | `Status` keeps both read-side projections: this branch's `prompt: Option<PromptStatus>` and upstream's `repository: Option<RepositoryStatus>` with its type. |
| `crates/wyrd/wyrd-server/src/components/cards/service.rs` | The served-Card overlay sets both `prompt` and `repository`; rustdoc and the blob test fixture name both. |
| `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts` | Not hand-merged: regenerated with `mise run codegen:regen` from `wyrd-spec`. |

Compatibility repair (`0d337a006`): merged wire types add `ActionApproval`, an
optional `extract.from` with `extractor: pull_request`, `DependsOn` as a list or
`{any_succeeded}`, and the `Repository` Card kind (upstream amended `AGENTS.md` to
13 kinds). TASK-001's `definitionGraph` treats approvals and extractor-driven
extracts as reading no inputs, links an any-succeeded join to every predecessor
with an `any succeeded` chip, and `CARD_KINDS` lists `Repository`, which the
existing every-registrable-kind type guard requires; `catalog.json` regenerated
by `scripts/catalog.ts`. This is the minimum for the existing viewer to compile
and stay truthful; TASK-002 still owns runtime presentation of new step forms.
RED: the added `verifier graph projects and lays out the declared topology` case
failed against the pre-repair build (`reads is not a function or its return value
is not iterable`); GREEN after the repair.

### Commands (sequential, repository root, against `0d337a006`)

| Command | Result |
|---|---|
| `mise exec -- cargo check --locked --workspace --all-targets` | pass |
| `mise run build:server` | pass (`wyrd-server 0.4.1`) |
| `mise run ts:build:testing` | pass |
| `mise run ui:check` | pass: 0 svelte-check errors in both projects; 9 files, 60 tests |
| `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` | pass |
| `mise run codegen:check` | pass |
| `… ui-library.integration.test.ts -t '^an exact Verifier definition and where it runs load through every authorized host$'` | 1 passed, 5 skipped |
| `… ui-pages.integration.test.ts -t '^an exact Verifier Card page draws its declared graph and where it runs$'` | 1 passed, 7 skipped |
| `mise exec -- cargo fmt --check -p wyrd-spec -p wyrd-server` | pass |
| `mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-server --all-targets -- -D warnings` | pass |
| `git diff --check a6a482306 HEAD`; conflict-marker scan | clean |

The two integration commands ran as written in the task under
`scripts/postgres/with-test-postgres.sh` with `WYRD_UI_INTEGRATION=1`. Runner
owners are unchanged in the merged `mise.toml`. The UI tree has no formatter
task; `svelte-check` inside `ui:check` is its lint lane.

### Upstream inputs for TASK-002 in the integrated baseline

Recorded from upstream's own routes and journey tests; not re-reviewed here.

| Input | Server owner | Client | Upstream proof |
|---|---|---|---|
| Status, compact result | `verification/routes.rs:298` | `Execution.status()` (`wyrd-sdk-ts/wyrd/src/index.ts:2674`) | `verifier-building-blocks.test.ts:1256` |
| Explicit details | `verification/routes.rs:754` | `Execution.details()` (`index.ts:2807`) | `verifier-building-blocks.test.ts:1256` |
| Invocation pages | `verification/routes.rs:345` | `Execution.invocations()` (`index.ts:2728`) | `durable-verifier-work.test.ts:43` |
| Publication pages and exact publication | `verification/routes.rs:396`, `:447` | `publications()`, `publication()` (`index.ts:2738`, `:2750`) | `verifier-orchestration.test.ts:678` |
| Publication content and file download | `verification/routes.rs:567`, `:506` | `download()` (`index.ts:2762`) | `verifier-orchestration.test.ts:678`, `verifier-building-blocks.test.ts:757` |
| Cancel | `verification/routes.rs:638` | `Execution.cancel()` (`index.ts:2773`) | `durable-verifier-work.test.ts:43` (foreign tenant refusal) |
| Approval decision | `verification/routes.rs:691` | (TASK-006 consumes) | upstream packet |
| Card by UID | `cards/routes.rs:48`, `:603` | existing HTTP/MCP read | TASK-001 hosts |
| Bifrost execution inputs/outputs/events | `architecture/bifrost-design.md:419–458` | existing `bifrost.query` UI operation | upstream packet |

### Obligation results

| Obligation and required cases | Implementation evidence | Proof location | Verification evidence | Result |
|---|---|---|---|---|
| `M-1`: both inputs in HEAD; no unresolved conflicts | merge `d118ee49f` | `git merge-base --is-ancestor` for `f6fac677c` and `a6a482306` | both ancestors; marker scan and `git diff --check` clean | PASS |
| `M-2`: conflicts at owners; upstream behavior, this packet's scope and TASK-001 viewer preserved; generated bindings from owners | conflict table above | `envelope.rs:475–505`, `cards/service.rs:424–425,490–491` | workspace `cargo check`; `codegen:regen`/`codegen:check`; TASK-001 integration tests | PASS |
| `M-3`: no new endpoint, server behavior, migration, authorization rule or execution UI; only compatibility repairs | `0d337a006` touches only `verifier.ts`, `schemas.ts`, `catalog.json`, one unit test | `git diff d118ee49f 0d337a006 --stat` | diff audit | PASS |
| `REQ-005/AC-008`: existing UI compiles, builds, keeps exact Verifier definition reads and refusals through authorized hosts and the browser page | `0d337a006` | `src/lib/ui-library.test.ts` (any-succeeded case) | `ui:check`, UI build, both named integration tests | PASS |
| `REQ-014`: TASK-002's upstream inputs identified; source alignment refreshed to the integrated revision | tables above | `design/source-alignment.md` | content review | PASS |
