---
name: wyrd-plan-v3
description: Decompose a Wyrd feature, refactor, migration, contract, SDK, server, CLI, MCP, storage, Vala, or cross-language request into a concise implementation plan and well-scoped, decision-complete task packets. Use when implementation agents need aligned ownership, paths, behavior, dependencies, acceptance criteria, and focused verification without executing the work.
---

# Wyrd Plan v3

Turn one user request into a plan implementation agents can follow without
rediscovering the design. Optimize expected wall-clock time to accepted,
integrated code. Concurrency is a means to remove real critical-path work, not
an objective by itself. Full decision-ready decomposition is mandatory, but it
does not require maximal execution fragmentation. Planning is not execution
orchestration: do not produce scheduling metrics, proof-attempt ledgers,
digest-bound manifests, or preflight evidence.

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
evidence scouts using `.agents/model-routing.md`. Give each scout one bounded
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
  intent.md
  plan.md
  plan-review.yaml
  tasks/
    01-<slug>.md
    02-<slug>.md
```

Keep `plan.md` short and decision-oriented:

1. `source_revision: <immutable SHA>`, objective, and user value.
2. Current state: the relevant source evidence and constraints.
3. Decisions: only choices an implementor must not reopen.
4. Task inventory: owner, outcome, and direct dependencies.
5. Cross-task contracts and initial runnable tasks. Do not define waves;
   successors become runnable as soon as their specific predecessors integrate.
6. Overall acceptance and progressive-verification strategy.
7. `Execution handoff: wyrd-implement-plan-v3`, initial runnable tasks,
   reconciliation points, and `wyrd-implement-v3` for every task.

Fully decompose the requested outcomes, decisions, owners, producer/consumer
contracts, actual dependencies, acceptance criteria, and proof obligations.
Then package that understood work into cohesive, decision-ready execution tasks
for the shortest expected time to accepted integration. Full decomposition is
about decision and dependency completeness, not packet count. Do not create a
worker dispatch or serial dependency merely because touched paths have different
repository owners.

Each task has one primary outcome and integration owner, but it may include
inseparable consumer, test, journey, generated, or wiring closure in other owned
surfaces. Create a separate packet when it gives a substantial independent
implementation outcome or is required to preserve authority and review
integrity; separation does not itself imply ordering. Add a dependency only
when the successor genuinely requires the predecessor's integrated artifact or
behavior. When planning fixes the shared contract, release independent producers
and consumers together.

Split a coherent task further for parallel execution only when the new packets
can perform substantial work concurrently and the expected time saved exceeds
context loading, dispatch, proof, review, reconciliation, invalidation, and
integration overhead. Every concurrency-motivated split must name the serial
critical-path work it removes.

Do not create a packet merely because work is theoretically separable. Keep
work together when it shares one owner or lifecycle invariant, uses the same
setup and proof lane, would collide in composition files, or one side is too
small to amortize orchestration. Do not make each journey, test selector,
wiring edit, or matrix row its own packet. Group related journeys by runtime
and shared fixture unless they can perform substantial implementation work in
parallel. Prefer a few concurrent end-to-end owner tracks over dozens of tiny
packets and serial joins.

Use a dependency only when a task requires a predecessor's committed contract
or behavior. Declare only direct dependencies. Never add ordering for
convenience, shared closeout verification, integration simplicity, or a named
phase. Do not use umbrella tasks or broad serial join tasks when independently
owned server, SDK, language projection, generated-artifact, telemetry,
fixture, or focused-test work can consume an already fixed contract. When
several consumers need a new shared contract, isolate the smallest coherent
foundation and release every independent consumer directly from it.

Expose substantial independent work when doing so is expected to shorten the
critical path, but never target worker utilization or DAG width independently
of total acceptance time. Do not add packets, dependencies, or integration
stages solely to occupy slots. Do not manufacture parallelism across a
genuinely shared mutable contract, fixture, migration, or test seam; keep that
inseparable seam cohesive and allow unrelated owner tracks to continue around
it. Prefer lower critical-path depth and fewer reconciliation points when two
decompositions expose similar useful concurrency.

Write `intent.md` as a concise normalization of the user's requested outcomes,
constraints, and non-goals. Preserve meaning; do not expand the request.
Planning should finish after one bounded source pass, optional evidence scouts,
and one independent review/revision cycle. A second review is required only
when remediation changes plan or packet bytes. Do not exhaustively enumerate
every possible touched file or incidental repair before implementation.

## Write task packets

Each `tasks/<NN>-<slug>.md` packet must contain these sections:

1. **Task contract** — a YAML block containing `id`, `depends_on`, `write_set`,
   `prohibited_writes`, `execution_tier`, `acceptance_criteria`, and
   `verification`. This is the machine-readable coordination contract; the prose below explains
   it. Use repository-relative paths, stable `AC<N>` IDs, and the narrowest
   exact `mise` command.
2. **Outcome** — the observable result and user/operator value.
3. **Context** — current source behavior and the repository evidence that
   determines the change.
4. **Scope** — expected paths/symbols, explicit non-goals, and prohibited
   changes. `write_set` is the best coordination forecast, not an allowlist.
5. **Design** — owned types, contracts, state/IO boundaries, error behavior,
   and the nearest repository-native precedent. Name exact paths and symbols;
   include signatures only where a new or changed cross-owner API needs one.
6. **Implementation** — ordered steps an implementor can perform without
   choosing a material contract. Describe negative, recovery, lifecycle,
   concurrency, cancellation, tenancy, audit, and durability behavior only
   when the surface makes each relevant. Explain inapplicability only when a
   maintainer could reasonably expect that dimension to matter.
7. **Consumers and integration** — callers, downstream surfaces, generated
   artifacts, and task dependencies affected by the change.
8. **Acceptance criteria** — reference the canonical YAML `AC1`, `AC2`, … IDs
   and explain evidence mapping without restating their text.
9. **Tests and verification** — name the smallest test that directly proves
   each AC, then add a broader module/package or journey check only when it
   covers a distinct cross-boundary risk. Define `verification.worker` as one
   optional, fast diagnostic command and `verification.candidate` as the
   narrowest authoritative exact `mise` command. Do not prescribe broad
   workspace checks per task or multiple overlapping tests merely because they
   are available. A focused database, Node, language-runtime, or integration
   check is valid when it is the narrowest authoritative proof of that task's
   owned boundary. Put only genuinely cross-task or full-plan checks in
   `plan.md` after their direct dependencies. Planning names commands; it does not run a
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

Use this task-contract verification shape:

```yaml
verification:
  worker: <optional fastest safe diagnostic mise command, or null>
  candidate: <one narrow authoritative mise command>
```

Set `execution_tier: fast` only for mechanical, localized, low-risk work whose
material decisions are fixed. Use `general` for work requiring normal
engineering judgment. The controller maps the tier through
`.agents/model-routing.md` and escalates unexpected ambiguity separately.

`worker` is optional and must not duplicate a costly candidate command. It is
for one narrow post-implementation signal, not a test-after-every-edit loop.
It may be a filtered Cargo check/test when that is the fastest useful type or
compile signal, but the controller must run it in the shared heavy-resource
lane. Focused database, Node, or language-runtime checks belong in `candidate`
when they directly prove the owned boundary. Broad cross-task suites belong at
integration closeout.
`candidate` is queued by the controller after the immutable candidate exists.

## Check before handoff

Before returning the plan, verify that:

- every requested outcome maps to one or more tasks;
- every task has bounded responsibility, a useful write forecast, an owner,
  non-goals, and concrete ACs;
- every cross-task contract has one producer and named consumers;
- dependencies are necessary and task order is executable;
- every user-facing behavior has the journey coverage required by `AGENTS.md`;
- the dependency DAG represents every material outcome, contains only direct
  dependencies, and exposes each source-evidenced independent execution track
  whose separation is expected to reduce wall time, with no fixed waves,
  umbrella tasks, or broad joins delaying that work;
- every outcome, owner, producer/consumer contract, actual dependency,
  acceptance criterion, and proof obligation is explicit without treating
  ownership boundaries as automatic task or ordering boundaries;
- every concurrency-motivated split identifies meaningful critical-path work
  removed and is large enough to justify its orchestration and integration
  cost;
- verification maps each AC to the smallest direct proof, queues expensive
  candidate checks, and reserves broad checks for integration/closeout; and
- no task leaves a material behavior or contract decision to its implementor.

Before handoff, calculate SHA-256 digests for `intent.md`, `plan.md`, and every
packet and request one independent `$wyrd-plan-review-v3` review of those exact
working-tree bytes against the immutable source revision. Correct bounded
findings and rerun review after any plan or packet change. Write the final reviewer YAML unchanged
to `plan-review.yaml`; it is the controller's approval input. Return unresolved
material choices to the user rather than guessing. Do not create an execution
manifest or mark tasks `Ready`; the execution controller owns candidates,
verification, review, and integration.

## Repair plan and task contracts autonomously

Accept either bounded repair bundle:

- `source_drift`: old and new source SHAs plus evidence naming only changed
  assumptions, affected tasks, contracts, paths, and verification seams; or
- `task_contract`: the review artifact/digest, finding IDs, affected task IDs,
  and bounded non-material corrections. Terminal-review contract repairs use
  this same bundle and identify the already integrated task digests.

For source drift, update `source_revision` and only the evidenced affected
contracts, packets, dependency text, and verification. For task repair,
revise only the named non-material packet fields and directly affected text.
Preserve `intent.md` and all approved material decisions. Rerun
`$wyrd-plan-review-v3`, replace `plan-review.yaml`, and return the old/new
digests plus exact invalidation set. Invalidate unintegrated changed tasks and
their dependents. Never relabel an integrated commit with new packet bytes;
retain its historical digest and express any required correction as an
additive remediation generation from the current integration SHA. Ask the user
only if the correction requires a new material decision.
