---
name: wyrd-plan
description: Stage 2 of the Wyrd build pipeline. Turns an approved spec.md into plan.md — the dependency-ordered commit DAG in the 00-overview format — plus a tasks.yaml skeleton. Use when the user says /plan, after a spec passes architecture review (gate 1), or when sequencing a feature into commits. Stop at the DAG: name commits, decisions, crates, and dependencies; do NOT pre-render per-commit implementations (that is wyrd-tasks). Routes to wyrd-architecture-reviewer skill (gate 2).
---

# Wyrd Plan

Stage 2 of the pipeline: **idea → spec → plan → tasks → implementation → test → review**.

Turn the approved `spec.md` into `plan.md`: the **dependency-ordered commit DAG**.
Each node is one self-contained commit. The plan is where the load-bearing
architecture-review gate (gate 2) fires — before any code, before per-commit
contracts exist.

This stage is **decision-complete, not pre-rendered**. `plan.md` names the
commits, the cross-cutting decisions, the crates each commit touches, and the
dependency edges. It does **not** expand a commit into rendered SQL, signatures,
or line numbers. That over-rendering is the exact failure mode the pipeline
removes; per-commit detail is `wyrd-tasks`' job, and implementation is hydrated
from CodeGraph at build time. (Canonical doctrine: `wyrd-tasks`.)

## When To Use

- `/plan`, or after `spec.md` passes `wyrd-architecture-reviewer` (gate 1).
- Sequencing a feature into commits, defining the dependency order, deciding what
  can run in parallel.

## Source Of Truth

- `spec.md` — the decisions and contracts. Do not reopen them; sequence them.
- `AGENTS.md` and `architecture/wyrd-design.md` — boundaries and authority.
- `wyrd-rust-python` "Ownership Boundaries" — which crate owns what, so each
  commit lands in the right crate and respects the durability split.
- Use `codegraph_explore` to confirm the seams a commit will touch exist and to
  size each commit; record findings as symbols, not line numbers.

## Process

1. Read `spec.md` and ground in the repo. Identify the units of change.
2. **Slice vertically.** Each commit is self-contained and leaves the system
   closer to working behavior. Prefer small commits touching a focused set of
   crates. Split anything that spans independent subsystems.
3. **Order by dependency.** Build the DAG: what must be sequential, what can run
   in parallel. The parallel structure here is what `wyrd-implement` later turns
   into concurrent execution waves — make it explicit.
4. **Capture cross-cutting decisions** that span more than one commit (carried
   from `spec.md`, refined to commit-level consequences).
5. Write `plan.md` in the format below to `.dev/plan/<feature>/plan.md`.
6. Emit the `tasks.yaml` **skeleton**: one node per commit with `id`, `title`,
   `depends_on`, and a `cratesHint`. `wyrd-tasks` fills the rest.
7. **Gate 2.** Hand `plan.md` to `wyrd-architecture-reviewer` **at full-sweep
   depth** (it runs with CodeGraph and verifies the commit seams against real
   source). Tell it "full sweep" explicitly and do not pre-narrow it to a short
   focus list. The DAG is where sequencing, commit-ownership, and missing-edge
   errors hide — they only become visible once work is sliced into commits, and
   they are cheap to fix as plan text but expensive once discovered mid-build.
   Then run the **dynamic review loop** (`review/dynamic-review-loop.md`): revise
   `plan.md`, hand it back for a fresh full-sweep re-review, and repeat until
   `wyrd-architecture-reviewer` returns `Decision: approve`. One correction is not
   a pass — a fresh re-review must approve the revised `plan.md` before moving to
   `wyrd-tasks`. `approve with changes`, `needs redesign`, and `reopen locked
   decision` are all non-terminal; if a **spec** decision reopens, return to
   `wyrd-spec`, otherwise revise `plan.md`, then re-submit. Preserve the loop
   ledger (`.dev/review/architecture/{REVIEW_ID}/loop-ledger.md`) as iteration
   evidence.

## References

Load from the shared doctrine library (`.claude/references/`, indexed in
`.claude/references/README.md`) when relevant:

- `doctrine/architecture-constraints.md` — boundary checklist for crate placement.
- `architecture/patterns.md` — implementation patterns (server/client/storage/
  provider/observability) that shape where each commit lands.
- `review/implementation-rules.md`, `review/rust-service-architecture.md` —
  sequence commits to pass these review rules by construction, not at the gate.
- `review/dynamic-review-loop.md` — the terminal gate-2 loop: review → revise →
  fresh re-review until `Decision: approve`.
- `review/production-architecture-rubric.md` — reliability, scale, security, HA,
  and distributed-systems constraints, consulted **at plan time** so the DAG is
  sequenced with them in mind.
- `map/source-map.md`, `map/codebase-map.md`, `map/surface-mapping.md` — where
  things live; predecessor and migration evidence.
- `domain/olap-datafusion-iceberg-arrow.md`, `domain/observability-otel.md` when
  the surface warrants.

## plan.md Shape (the 00-overview format)

This is the format already proven in the repo — e.g.
`.dev/plan/foundations/05-security-auth/identity/pr-plan/00-overview.md`. Keep it.

```markdown
# <Feature> Plan — Commit Train Overview

## What this delivers
One or two paragraphs: the behavior delivered and what stays unchanged.

## Commit train (dependency-ordered)
| # | Commit | Crates touched | Depends on |
|---|---|---|---|
| 01 | <slug> | <crates> | — |
| 02 | <slug> | <crates> | 01 |
...

```
<ASCII dependency graph showing parallel branches and join points>
```

## Cross-commit decision: <name>
The accepted decision, its rationale, and the consequences threaded through the
commits it touches. One block per cross-cutting decision.

## Global verification gates
The repo gates each commit class must pass (mise/cargo commands).

## Boundary rules honored throughout
The crate/ownership/doctrine boundaries this train must not violate.
```

## Anti-Patterns (reject these)

- Expanding a commit inline into rendered implementation (SQL bodies, full
  signatures, algorithm pseudocode). Stop at the DAG; that detail is `wyrd-tasks`.
- `file.rs:line` references. Name the crate and, where a seam matters, the symbol.
- A commit that spans independent subsystems — split it.
- Re-deciding what `spec.md` settled. The plan sequences decisions; it does not
  remake them.
- An XL commit with no dependencies stated.

## Output

```
.dev/plan/<feature>/
  plan.md             # the commit DAG (this stage)
  tasks.yaml          # skeleton: id, title, depends_on, cratesHint (this stage)
```

## Verification

Before handing off:

- Every commit has a goal (one line), crates touched, and `depends_on`.
- The DAG is acyclic; parallel branches and joins are explicit.
- Cross-cutting decisions from `spec.md` are present with commit-level
  consequences.
- No commit is expanded into rendered code; no `file.rs:line`.
- `tasks.yaml` skeleton matches the DAG exactly.
- `wyrd-architecture-reviewer` (gate 2) has run the dynamic review loop
  (`review/dynamic-review-loop.md`) on `plan.md` and returned `Decision: approve`
  on the final revised artifact. The loop ledger is present with one block per
  iteration. `approve with changes` is not a pass — the gate advances only on
  `approve`.

## Hand-Off

`plan.md` + `tasks.yaml` skeleton (gate 2 returned `Decision: approve` on the
final revised artifact) → **`wyrd-tasks`**: expand
each DAG node into a thin per-commit contract and fill `tasks.yaml`.
