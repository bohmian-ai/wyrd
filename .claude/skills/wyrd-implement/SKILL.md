---
name: wyrd-implement
description: Final execution stage of the Wyrd build pipeline. Reads a tasks.yaml commit DAG and implements it in dependency-ordered parallel waves — each commit executed by a CodeGraph-hydrated agent, model routed per task, isolated in a worktree, iterated to green against the repo's mise gates. Use when the user says /build or /implement after thin task contracts exist (wyrd-tasks). This is where plan and build fuse into one warm pass; the deep codebase load happens here, once.
---

# Wyrd Implement

Stage 4 of the pipeline: **idea → spec → plan → tasks → implementation → test → review**.

Execute the commit DAG in `tasks.yaml`. The wall-clock win lives here: independent
commits run **in parallel**, each executor is **cold but CodeGraph-hydrated** (no
re-load tax), and plan+build are **fused** — the thin task drives a single warm
pass straight to code+tests.

## When To Use

- `/build` or `/implement`, after `wyrd-tasks` produced `tasks.yaml` + `tasks/`.
- Input: `.dev/plan/<feature>/tasks.yaml` (the DAG) and `tasks/NN-*.md` (thin
  contracts).

## Execution model: dependency waves

`tasks.yaml`'s `depends_on` edges form a DAG. Compute **topological waves**: a
wave is the set of `pending` tasks whose dependencies are all `done`.

- **Within a wave:** commits are independent → run in **parallel**, each in a
  worktree the **main agent pre-creates at an explicit base** (see the warning
  below). The executor `cd`s into its assigned worktree path.
- **Between waves:** barrier. The main agent integrates the completed wave onto
  the impl branch before the next wave starts, so the next wave's worktrees
  branch from code that includes its dependencies.

The Workflow script owns **parallelism within a wave**. The main agent owns **all
git — worktree creation, integration, and cleanup** (the script has no
filesystem/git access). One Workflow invocation per wave.

> **Do NOT use Workflow's `isolation: 'worktree'` for executors.** That option
> branches the worktree from the **harness default (`main`)**, ignoring the
> feature's actual base. When a feature's prerequisites live on a base branch
> other than `main` (e.g. `wyrd-client`'s prereqs live on `wyrd-client-identity`,
> never merged to `main`), an `isolation`-created worktree would be missing them
> and the executor would "rebuild the whole crate from scratch." The orchestrator
> therefore creates worktrees itself, at a base it controls (the impl-branch
> tip), and `wave.js` dispatches executors into those paths. The impl-branch tip
> descends from `base_branch` and includes every integrated wave, so all
> prerequisites are present.

## The loop (main agent)

1. Read `tasks.yaml`. Note its `base_branch` (where prerequisites live — e.g.
   `wyrd-client-identity`, **not** `main`) and `impl_branch` (where the commits
   land). In the **primary working tree**, check out `impl_branch`, creating it
   from `base_branch` if new (`git switch -c <impl_branch> <base_branch>`). Never
   assume `main`; the base ref comes from `tasks.yaml`.
2. Compute the next wave (pending tasks with all deps `done`). If none and any
   task is still pending, stop and report a cycle or a blocked task.
3. **Reindex the parent, then create worktrees, then dispatch.** Let `base` =
   the current `impl_branch` tip (it descends from `base_branch` and includes
   every integrated prior wave, so all prerequisites are present).
   - **Reindex first — before any executor makes a change.** In the **primary
     working tree** (git's original clone dir, the first entry in
     `git worktree list`), which step 1 / step 4 leaves checked out on `base`,
     run the incremental `codegraph sync` (or `codegraph index -i` if no
     `.codegraph/` exists yet). This single shared index is what every linked
     worktree's `codegraph explore` resolves to, so syncing here makes the
     parent's *current* symbols visible to this wave's hydration. Run this for
     **every** wave, including the **first** — do not skip it assuming a prior
     session left the index current; it may still reflect `main` or a stale
     branch. Invariant: the primary tree must be on `base` when you sync.
     `.codegraph/` stays gitignored; the index is never committed.
   - **Then create a worktree per node, at the explicit base:**
     `git worktree add .claude/worktrees/impl-<id> <base> -b impl/<id>-<slug>`
     (reuse the path if it already exists and is correctly based).
   - **Then invoke** the wave Workflow (`.claude/workflows/wave.js`) with
     `args = { featureDir, base, tasks: [<nodes with file+model+worktree+crates+seams+verify>] }` —
     each node carrying the absolute `worktree` path you just created. Do **not**
     rely on `isolation: 'worktree'` (it would branch from `main`; see the warning
     above).
   - **`featureDir` MUST be the absolute primary-tree path** (e.g.
     `/…/wyrd/.dev/plan/<feature>`), not the repo-relative `.dev/plan/<feature>`.
     `.dev/` is gitignored on purpose (planning artifacts never pollute the repo),
     so the task contracts exist **only** in the primary tree's working dir — a
     freshly-created linked worktree does not contain them. With an absolute
     `featureDir`, each executor's `${featureDir}/${t.file}` resolves to the
     primary-tree copy from inside any worktree; reading the contract there is
     fine (it is read-only reference — all *writes* still happen in `t.worktree`).
4. For each returned executor result:
   - **green:** integrate its branch into `impl_branch` in `id` order (`git merge
     --no-ff` or fast-forward; independent commits touch disjoint crates so this
     is clean). Mark the task `done` in `tasks.yaml`, then remove its worktree
     (`git worktree remove .claude/worktrees/impl-<id>`) now that it is merged.
   - **not green / escalate:** see "Recovery & escalation". Leave `pending` or
     mark `blocked`; surface to the user. **Keep** the worktree — its state is the
     evidence the escalation/Opus re-dispatch and you will need to debug.
5. **Integrate in the primary working tree, leaving it on the new `base`.** Wave
   integration (the step-4 merges) happens **in the primary working tree** (git's
   original clone dir, the first entry in `git worktree list` — *not* a
   `.claude/worktrees/` linked worktree, and nothing to do with `main`). After
   merging, the primary tree is checked out at exactly the `base` the next wave
   will branch from. Do **not** reindex here — the reindex is always the
   **pre-dispatch** step (step 3), which the next iteration runs against this new
   `base`. Keeping the sync in one place (just before dispatch) is what
   guarantees the parent is reindexed *before any change is made*, on every wave
   including the first.
6. Repeat from step 2 until all tasks are `done`.
7. Hand off to the **test** stage (the feature-level `final_gate` in
   `tasks.yaml`) and then **review** (`review-and-plan`).

`tasks.yaml` status makes this **resumable**: a re-run skips `done` nodes and
resumes at the first incomplete wave.

## CodeGraph in worktrees (why hydration works without an index in the worktree)

Executors run in **linked worktrees** under `.claude/worktrees/`, which have **no
`.codegraph/` of their own** (it is gitignored — a SQLite index is generated
state, never tracked). That is fine: the `codegraph explore` CLI run from inside
a linked worktree **auto-resolves the index in the primary working tree** (git's
original clone dir — the first entry in `git worktree list`) and returns the seam
source. It prints a "results come from a different git worktree" warning — that
warning is expected and benign here, because seams are pre-existing symbols whose
committed source on `base` is exactly what the executor wants. That single
primary-tree index reflects whatever branch the **primary working tree** has
checked out (in this pipeline, the impl branch — *not* `main`, and not
necessarily the same `base_branch` the impl branch forked from). The pre-dispatch
`codegraph sync` (loop step 3, run before each wave including the first) keeps that
shared index current on `base` as the impl branch advances, so executors always
hydrate against the parent's symbols as of the moment before they start changing. Executors hydrate via the **CLI**, not the
MCP tool: the `codegraph_explore` MCP tool is deferred for Workflow subagents
(needs a ToolSearch to load), whereas `codegraph explore` is always in Bash.

## Per-executor contract

Each commit is one agent. Its job, in order:

1. **Hydrate, don't spelunk.** Pull the named seams' verbatim source + call paths
   with **one** `codegraph explore "<seam symbols>"` Bash call (the seam symbols
   are in the task and in `tasks.yaml`); re-run with more symbols if a referenced
   seam is still unseen. Ignore the worktree-mismatch warning. This replaces the
   multi-file read the old 235-line plan tried to pre-serialize. **Do not** use
   `cat`/`ls`/`grep`/`find`/`cargo metadata` to read or discover source — that is
   the exact failure mode this pipeline removes. (Editing files, writing tests,
   and running build/test/lint gates is of course fine.)
2. **Honor the contract.** Implement exactly the decisions, seams, and invariants
   in `tasks/NN-*.md`, and follow its `## Approach` steps literally and in order —
   they are prescriptive. Do not reopen decisions or read crates outside the
   commit's scope; if the contract is wrong, under-specified, or names a seam
   CodeGraph can't find, stop and report `blocked` — do not improvise architecture
   or spelunk to fill the gap.
3. **Write code + tests** following `wyrd-rust-python` doctrine (owning crate,
   error catalog, PyO3 boundary, tenant/audit rules).
4. **Iterate to green** against the task's `verify` commands (targeted first:
   `cargo test -p <crate> <name> --all-features -- --test-threads=1`).
5. **Commit** on its worktree branch and return structured status.

## Model routing

Route each executor on the task's `model` field (`wyrd-tasks` set it):

- **sonnet (default):** a well-specified, low-surprise commit — writing is
  mechanical translation of a settled contract; Sonnet is faster.
- **opus:** concurrency, trait/object-safety design, PyO3 boundary, migrations,
  novel algorithms — the iterate-to-green recovery pass needs real reasoning.

Pass `model` through to the executor agent (`agent(..., { model })`).

## Recovery & escalation

The recovery pass (making real Rust compile and pass) is the one part the plan
cannot pre-empt. Handle failure explicitly, do not let a weak model thrash:

- A `sonnet` executor that cannot reach green in **N iterations** (default 3)
  returns `status: needs_escalation` with its failure context (compiler/test
  output, what it tried).
- The main agent re-dispatches that single task on **opus** with the failure
  context attached. This is the **empirical** escalation: Opus is paid only where
  reasoning was actually required, discovered rather than guessed.
- If opus also fails, mark `blocked` and surface to the user with the evidence.
  Never mark a task `done` with failing tests.

## References

The executor's implementation doctrine is **`wyrd-rust-python`** — invoke it rather
than re-listing its references here. It routes into the shared library
(`.claude/references/rust-python/*`, `architecture/patterns.md`,
`domain/iceberg-bifrost.md`) by surface and carries the CodeGraph-hydration rule.
Load `domain/observability-otel.md` or `domain/olap-datafusion-iceberg-arrow.md`
directly when a commit's surface warrants. The test stage uses
`rust-python/testing-workflows.md` plus the repo `mise` gates.

## Verification

- Every integrated task's `verify` commands are green before it is marked `done`.
- After all waves: the feature-level `final_gate` (`mise run test:unit`, `lints`,
  `check`, the `test:bifrost` Postgres matrix, `codegen:check` on contract
  changes) is clean on the assembled branch.
- `mise run pre-pr` clean before review.

## Hand-Off

Assembled feature branch (all tasks `done`, gates green) → **`review-and-plan`**
for the final fan-out review; optionally `wyrd-architecture-review` again if the
implementation diverged from `plan.md`.
