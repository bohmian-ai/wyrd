# Task Decomposition

Compile the implementation specification into bounded, risk-routed Luna,
Terra, or Sol tasks. Task packetization and separate task files are mandatory
for every plan.

## Contents

- [Choose task count](#choose-task-count)
- [Choose boundaries](#choose-boundaries)
- [Plan and task ownership](#plan-and-task-ownership)
- [Dependencies and parallelism](#dependencies-and-parallelism)
- [Cold implementation rehearsal](#cold-implementation-rehearsal)
- [Example decomposition](#example-decomposition)
- [Decomposition audit](#decomposition-audit)

## Choose task count

- **Small:** one complete task packet under the plan's `tasks/` directory.
- **Medium:** default to two through five dependency-ordered tasks plus one
  plan-level closeout.
- **Large or risky:** use explicit milestones, multiple tasks, task review, and
  dependency-aware execution.

Split when a task has its own cohesive outcome and focused proof, a prerequisite
contract must stabilize first, the context exceeds the assigned model's useful
working set, or separate ownership improves execution safety.

Do not split one cohesive change merely because it touches several files. Do
not keep unrelated outcomes together merely because one agent could edit them
in one session.

## Choose boundaries

Prefer vertical outcomes:

- contract + owner + integration + tests;
- migration + data access + transaction behavior + tests;
- one public projection + its generated surface + journey;
- one background capability + lifecycle + recovery;
- one focused refactor + all affected callers + regression tests.

Avoid layer-only packets such as “types,” “handlers,” and “tests” when none
produces a usable or verifiable boundary. Avoid one task per file.

Each task must leave the repository coherent for the next dependent task. A
task may introduce a stable internal contract consumed by a later task, but it
must test that contract itself.

## Plan and task ownership

The canonical plan owns:

- product intent, requirements, architecture, and decisions;
- public, durable, and cross-task contracts;
- global constraints, non-goals, risks, migration, and rollout;
- task inventory, dependency order, and requirement traceability;
- feature-wide acceptance and closeout verification;
- review and approval status.

Each task owns:

- one observable implementation outcome;
- the exact allowed and prohibited scope;
- relevant current-state context;
- target paths, symbols, callers, and tests;
- required code structure, interfaces, responsibilities, and control flow;
- task-specific errors, edge cases, and acceptance criteria;
- focused tests, commands, features, exclusions, and completion evidence;
- escalation conditions.

Reference parent requirement and decision IDs instead of duplicating feature
reasoning. Repeat the concrete contract details required to execute the task.

## Dependencies and parallelism

Sequential tasks may overlap files. Record why the later task depends on the
earlier contract or behavior.

Mark tasks parallel only when:

- neither depends on the other's uncommitted contract;
- their write sets do not overlap;
- they do not mutate the same generated artifact, migration chain, manifest,
  lockfile, or shared configuration;
- their verification can be serialized when they share a Cargo target.

All Cargo-backed commands remain sequential across agents sharing a checkout
or target directory, regardless of source-task parallelism.

If a task discovers a missing material decision, it stops and returns to Sol.
It does not expand scope or invent a contract.

Assign Luna to mechanical, low-risk work with one established repository
pattern. Assign Terra to ordinary implementation and review work. Assign Sol
to security, public or persisted contracts, migrations, concurrency,
cross-owner or cross-language changes, and other materially high-risk work.
Reasoning effort never exceeds `high`.

## Cold implementation rehearsal

Before marking a task `Ready`, run a read-only cold rehearsal. Use a fresh
agent when the environment supports it and give that agent only the task
packet, repository, and normal repository authorities. Do not provide the
intended solution, suspected omissions, or prior planning conclusions.

When fresh-agent delegation is unavailable, perform the same cold pass
yourself from the task packet and document why an independent agent was
unavailable. Unavailable delegation does not by itself block `Ready`.

Require the rehearsal to:

- locate every owner, target symbol, caller, and consumer;
- trace trait and runtime dispatch where applicable;
- confirm named fixtures, support exports, features, and explicit test targets;
- inspect proposed commands and repository-managed setup;
- walk the first implementation and test steps;
- report any decision it would have to make while coding.

Revise and repeat when the rehearsal finds a nonexistent seam, invalid command,
missing impacted surface, or material choice. A document-only architecture
review does not satisfy this gate.

## Example decomposition

This example demonstrates structure only:

```markdown
| Task | Outcome | Requirements | Model | Depends on | Packet |
|---|---|---|---|---|---|
| T1 | Server-owned typed Source check is reachable over HTTP | R1–R4 | Terra | None | `tasks/01-source-check-server.md` |
| T2 | Rust and Python clients project the shared contract | R5 | Terra | T1 | `tasks/02-source-check-sdks.md` |
| T3 | MCP exposes the checked operation and journey | R6 | Luna | T1 | `tasks/03-source-check-mcp.md` |
| T4 | Integrated closeout and traceability | R1–R6 | Sol | T1–T3 | Parent plan |
```

Why this split works:

- T1 locks the wire and server semantics before projections.
- T2 owns two client projections that share one SDK owner and generated stub
  workflow.
- T3 owns the independent agent-facing projection and its journey.
- T2 and T3 may edit independently after T1, but their Cargo-backed checks run
  sequentially.
- Closeout remains a plan responsibility, not a fourth implementation packet.

For a localized parser refactor, use one separate task:

```markdown
`tasks/01-consolidate-parser.md` owns the parser, its direct callers, focused
regression tests, and targeted verification. A second “tests” task would not
have an independent outcome.
```

## Decomposition audit

Before finalizing:

- every requirement maps to one or more tasks;
- every task has one cohesive outcome;
- task order follows contract and migration dependencies;
- every task fits its assigned Luna, Terra, or Sol context and reasoning level;
- every task passed a documented cold read-only implementation rehearsal,
  fresh-agent when available;
- no task must reconstruct the full plan;
- sequential overlap is explicit;
- parallel tasks have non-overlapping write sets;
- integration and closeout remain plan-level responsibilities.

Do not create `tasks.yaml`, duplicate specs, ledgers, run directories, or
completion sentinels.
