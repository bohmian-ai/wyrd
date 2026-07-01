---
name: wyrd-tasks
description: Pipeline stage that turns an approved commit plan (plan.md / the 00-overview DAG) into thin, executable per-commit task contracts plus a machine-readable tasks.yaml. Use when the user says /tasks, after a plan has passed architecture review (gate 2), or when expanding a commit DAG into per-commit work. Produces decision-bearing contracts that name seams as symbols and let the implementer hydrate code from CodeGraph — never 235-line pre-rendered implementations.
---

# Wyrd Tasks

Stage 3 of the pipeline: **idea → spec → plan → tasks → implementation → test → review**.

Turn the approved commit DAG (`plan.md`) into one **thin task contract** per
commit (`tasks/NN-*.md`) and a **machine-readable graph** (`tasks.yaml`) that
drives parallel execution.

A thin task is ~40–70 lines. It pins **decisions, seams, and invariants** — not
SQL bodies, signatures, or line numbers. The implementer hydrates verbatim source
from CodeGraph at build time. This is the deliberate inversion of the old
"render the whole implementation in prose" doctrine: the deep codebase load
happens **once**, at execution, not twice (plan + build).

**This is the canonical statement of the pipeline's plan-depth doctrine: a plan
is decision-complete, not pre-rendered.** Pin decisions and contracts; name seams
as symbols with invariants; never `file.rs:line`; never rendered bodies — the
executor hydrates from CodeGraph against the current code.

## When To Use

- `/tasks`, or after `plan.md` passes `wyrd-architecture-reviewer` (gate 2).
- Input is the commit DAG: `plan.md` plus a `tasks.yaml` skeleton (one node per
  commit, with `depends_on`) emitted by `wyrd-plan`.

## Process

1. Read `spec.md` (decisions/contracts) and `plan.md` (the DAG + cross-commit
   decisions). The decisions are already made and reviewed — do not reopen them.
2. **Fan out:** one agent per DAG node, in parallel. Each agent writes its
   `tasks/NN-*.md` and fills its `tasks.yaml` node. (See
   `references/fan-out.md` for the Workflow shape.)
3. For each commit, use `codegraph_explore` to **identify the seams** — the
   existing symbols this commit reuses or must not disturb — and record them **as
   symbols**, never as `file.rs:line`. Line numbers go stale; CodeGraph does not.
4. Write the thin contract using `references/thin-task-format.md`.
5. Fill the node in `tasks.yaml` per `references/tasks-yaml-schema.md`:
   `depends_on`, `crates`, `seams`, `model`, `status: pending`.
6. Set the `model` hint: `sonnet` by default; `opus` when the commit touches
   concurrency, trait/object-safety design, the PyO3 boundary, migrations, or a
   novel algorithm — anywhere the iterate-to-green recovery pass will need real
   reasoning.

## Seams & Invariants — the load-bearing rule

This section is why thinning does not starve `wyrd-architecture-reviewer`. The arch
gate reasons about decisions and the seams a commit depends on; with CodeGraph it
verifies each named seam's invariant against the *actual* source. So the contract
must make that verification possible.

For **every** existing symbol the commit reuses or must avoid:

- Name it as a symbol (e.g. `consume_active_refresh`, `issue_for_subject`).
- State the **invariant the commit depends on**, in one line.

Examples (from the identity refresh commit):

- `consume_active_refresh` MUST be an atomic revoke-and-return (single
  `UPDATE ... RETURNING`) — single-use rotation and race-safety depend on it.
- Do **not** route through `issue_for_subject`; it inserts via the non-rotated
  `insert_refresh_token` and would break rotation.

The quality bar: delete rendered bodies, but name **every** load-bearing seam
with its invariant. Under-naming seams ("reuse the auth helpers") is the failure
mode that *does* degrade the arch gate — `wyrd-tasks` must reject it.

## References

Bundled here (process mechanics, local to this skill): `references/thin-task-format.md`,
`references/tasks-yaml-schema.md`, `references/fan-out.md`.

From the shared doctrine library (`.claude/references/`, indexed in
`.claude/references/README.md`) — load the slice matching each commit's surface so
seams and invariants are named against real doctrine:

- `review/implementation-rules.md` — Rust/PyO3/Python/API/MCP/codegen rules.
- `rust-python/rust-core.md`, `rust-python/pyo3-boundaries.md`,
  `rust-python/errors.md`, `rust-python/python-api-and-stubs.md`,
  `rust-python/testing-workflows.md`, `rust-python/agent-harness.md`.
- `architecture/patterns.md` — crate ownership and contract placement.
- `domain/iceberg-bifrost.md` and the OLAP/OTel domain refs when the surface applies.

## Anti-Patterns (reject these)

- Pre-rendered SQL, function bodies, or full signatures. That is the executor's
  job, hydrated live.
- `file.rs:123` references. Name the symbol; CodeGraph resolves it.
- A vague seam list with no invariants.
- Re-deciding something `spec.md`/`plan.md` already settled. Tasks execute
  decisions; they do not make them.
- A task that spans independent subsystems. Push back to `wyrd-plan` to split the
  node.

## Output

```
.dev/plan/<feature>/
  tasks.yaml          # the machine-readable graph (drives wyrd-implement)
  tasks/NN-<slug>.md  # one thin contract per commit
```

## Verification

Before handing off to `wyrd-implement`:

- Each `tasks/NN-*.md` is thin (~40–70 lines) and follows the format.
- Every load-bearing seam is named as a symbol with an invariant; no
  `file.rs:line`, no rendered bodies.
- `tasks.yaml` validates against the schema: every node has `depends_on`,
  `crates`, `seams`, `model`, `status`, and an acceptance/verify reference.
- The `depends_on` edges match `plan.md`'s DAG exactly.

## Hand-Off

`tasks.yaml` + `tasks/` → **`wyrd-implement`**: topological waves, worktree
isolation, CodeGraph-hydrated executors, model routed per node.
