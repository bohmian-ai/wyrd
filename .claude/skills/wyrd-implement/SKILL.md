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

- **Within a wave:** commits are independent → run in **parallel**, each in its
  own worktree (`isolation: 'worktree'`).
- **Between waves:** barrier. The main agent integrates the completed wave onto
  the feature branch before the next wave starts, so the next wave's worktrees
  branch from code that includes its dependencies.

The Workflow script owns **parallelism within a wave**. The main agent owns **git
integration between waves** (the script has no filesystem/git access). One
Workflow invocation per wave.

## The loop (main agent)

1. Read `tasks.yaml`. Ensure the feature branch is checked out
   (`git switch -c <feature>` from `base_branch` if new).
2. Compute the next wave (pending tasks with all deps `done`). If none and any
   task is still pending, stop and report a cycle or a blocked task.
3. Invoke the wave Workflow (`.claude/workflows/wave.js`) with
   `args = { featureDir, base, tasks: [<this wave's nodes with file+model+seams+verify>] }`.
4. For each returned executor result:
   - **green:** integrate its branch into the feature branch in `id` order
     (`git merge --no-ff` or fast-forward; independent commits touch disjoint
     crates so this is clean). Mark the task `done` in `tasks.yaml`.
   - **not green / escalate:** see "Recovery & escalation". Leave `pending` or
     mark `blocked`; surface to the user.
5. Repeat from step 2 until all tasks are `done`.
6. Hand off to the **test** stage (the feature-level `final_gate` in
   `tasks.yaml`) and then **review** (`review-and-plan`).

`tasks.yaml` status makes this **resumable**: a re-run skips `done` nodes and
resumes at the first incomplete wave.

## Per-executor contract

Each commit is one agent. Its job, in order:

1. **Hydrate, don't re-read.** Pull the named seams' verbatim source + call paths
   with `codegraph_explore` (the seam symbols are in the task and in
   `tasks.yaml`). This replaces the multi-file read the old 235-line plan tried to
   pre-serialize.
2. **Honor the contract.** Implement exactly the decisions, seams, and invariants
   in `tasks/NN-*.md`. Do not reopen decisions; if the contract is wrong or
   under-specified, stop and report — do not improvise architecture.
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
