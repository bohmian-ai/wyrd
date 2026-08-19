---
name: wyrd-plan-v3
description: Decompose a Wyrd feature, refactor, migration, contract, SDK, server, CLI, MCP, storage, Vala, or cross-language request into a concise implementation plan and well-scoped, decision-complete task packets. Use when implementation agents need aligned ownership, paths, behavior, dependencies, acceptance criteria, and focused verification without executing the work.
---

# Wyrd Plan v3

Turn one user request into a plan implementation agents can follow without
rediscovering the design. Prefer the smallest useful decomposition. Planning is
not execution orchestration: do not produce scheduling metrics, proof-attempt
ledgers, digest-bound manifests, or preflight evidence.

## Investigate only what fixes the plan

Read `AGENTS.md`, `architecture/agent-rules.md`, and the repository paths that
own the requested behavior. Read `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, and relevant routed references when the
request changes a behavior or contract they govern. Use CodeGraph first when
the repository is indexed. Inspect the nearest implementation, callers,
consumers, tests, manifests, and the relevant `mise` tasks.

Resolve material questions from repository evidence. Ask the user only when a
choice changes the product behavior, public or durable contract, ownership,
security, tenancy, migration, or acceptance outcome. Do not turn ordinary
implementation details, test setup, or adjacent mechanical repairs into
planning blockers.

## Use bounded evidence scouts when breadth earns them

For an unfamiliar or multi-owner evidence gap that direct tracing cannot
efficiently close, the planner may dispatch one through three read-only
`gpt-5.6-luna` scouts at medium reasoning effort. Give each scout one bounded
question and explicit repository or source scope. Require an evidence-only
report containing verified facts with paths, symbols, or primary links,
relevant tests or commands, counterevidence, and unresolved uncertainty.

Use scouts to inventory owners and projections, find repository-native
precedents, or map consumers, configuration, and verification conventions. Do
not dispatch them for a single named symbol, a local change with an obvious
precedent, or a judgment-heavy design choice. Scouts do not edit, recommend,
decide, assign tasks, or create plan artifacts. The parent planner verifies
facts that affect contracts, ownership, security, tenancy, dependencies, or
acceptance criteria, and remains solely responsible for the plan and packets.

## Write the plan

Create one plan directory with:

```text
<slug>/
  plan.md
  tasks/
    01-<slug>.md
    02-<slug>.md
```

Keep `plan.md` short and decision-oriented:

1. Objective and user value.
2. Current state: the relevant source evidence and constraints.
3. Decisions: only choices an implementor must not reopen.
4. Task inventory: owner, outcome, dependencies, and whether tasks can run in
   parallel.
5. Cross-task contracts and integration order.
6. Overall acceptance and verification strategy.
7. Handoff: the task order and the implementation skill each task requires.

Decompose by cohesive ownership and independently reviewable outcomes. A task
should leave its declared boundary coherent, have one clear owner, and include
the smallest consumer/test updates required by its behavior. Use a dependency
only when a task needs a predecessor's committed contract or behavior. Tasks
with disjoint ownership may run in parallel; do not manufacture parallelism or
calculate concurrency scores.

## Write task packets

Each `tasks/<NN>-<slug>.md` packet must contain these sections:

1. **Task contract** — a YAML block containing `id`, `depends_on`, `write_set`,
   `prohibited_writes`, `acceptance_criteria`, and one `verification.command`.
   This is the machine-readable execution boundary; the prose below explains
   it. Use repository-relative paths, stable `AC<N>` IDs, and the narrowest
   exact `mise` command.
2. **Outcome** — the observable result and user/operator value.
3. **Context** — current source behavior and the repository evidence that
   determines the change.
4. **Scope** — allowed paths/symbols, explicit non-goals, and prohibited
   changes.
5. **Design** — owned types, contracts, state/IO boundaries, error behavior,
   and the nearest repository-native precedent. Name exact paths and symbols;
   include signatures only where a new or changed cross-owner API needs one.
6. **Implementation** — ordered steps an implementor can perform without
   choosing a material contract. Describe negative, recovery, lifecycle,
   concurrency, cancellation, tenancy, audit, and durability behavior only
   when the surface makes each relevant; state why an omitted dimension is
   inapplicable.
7. **Consumers and integration** — callers, downstream surfaces, generated
   artifacts, and task dependencies affected by the change.
8. **Acceptance criteria** — stable `AC1`, `AC2`, … statements that describe
   externally observable behavior, including required negative paths.
9. **Tests and verification** — named Rust/Python/TypeScript/journey tests as
   appropriate, plus the narrowest exact `mise` command expected after the
   task is implemented. Planning names the command; it does not run a
   preflight or record selected-test counts.
10. **Stop and escalate if** — only material decisions or authority gaps that
   cannot be resolved from the task and repository authority.

Use exact paths, symbols, owners, behavior, and test names where they exist.
For a new symbol or test, state its proposed path/name and what it proves. Do
not use placeholders such as “wire it up,” “as needed,” “follow existing
patterns,” “add tests,” TODO, or TBD.

Keep task packets detailed enough for an implementor to remain aligned, but do
not prescribe private helper names, local refactors, or incidental repair
mechanics when multiple repository-native implementations satisfy the task.

## Check before handoff

Before returning the plan, verify that:

- every requested outcome maps to one or more tasks;
- every task has a bounded write scope, owner, non-goals, and concrete ACs;
- every cross-task contract has one producer and named consumers;
- dependencies are necessary and task order is executable;
- every user-facing behavior has the journey coverage required by `AGENTS.md`;
- verification commands use the narrowest relevant `mise` task; and
- no task leaves a material behavior or contract decision to its implementor.

Return the plan and packets with a concise note of unresolved material choices,
if any. Do not create an execution manifest or mark tasks `Ready`; the
execution controller owns candidates, verification, review, and integration.
