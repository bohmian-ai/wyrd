# Plan Review Rubric

Use impact, blast radius, and reversibility to decide whether an issue belongs
in a pre-implementation verdict.

## Contents

- [Finding test](#finding-test)
- [Decision completeness](#decision-completeness)
- [Task executability](#task-executability)
- [Plan traceability](#plan-traceability)
- [Severity](#severity)
- [Exclusions](#exclusions)
- [Wyrd authority](#wyrd-authority)
- [Reversibility](#reversibility)
- [Gate expectations](#gate-expectations)

## Finding Test

Report a finding only when all are true:

1. The artifact owns or changes the decision.
2. Current Wyrd authority, CodeGraph, and the named local precedent do not make
   the task instruction unambiguous for its assigned model.
3. Leaving it unresolved can produce incompatible implementations, material
   failure, or force the assigned implementation model to redesign
   architecture, contracts, persistence, security, ownership, acceptance
   outcomes, or proof.
4. The impact clears the Critical or Major threshold.

Silence is not automatically a gap. Plans do not need to restate repository
rules, render complete source bodies, or prescribe inconsequential syntax.
Executable task packets do need verified target seams, material interfaces,
consequential behavior, tests, and executable proof recipes.

## Decision Completeness

For every changed public, durable, security-sensitive, migration, concurrency,
or cross-owner behavior, confirm the artifact settles:

- the observable requirement and non-goal;
- the owning crate, service, package, or store;
- interface and invariant semantics;
- ordering, atomicity, error, retry, idempotency, and recovery behavior where
  material;
- compatibility and rollout expectations;
- the objective proof required before completion.

Do not require detail already fixed by current authority, CodeGraph, or the
nearest named local pattern. The task must name that precedent when it relies
on it; do not require the assigned model to discover which of several patterns
applies.

## Task Executability

For every implementation task, confirm:

- the exact standardized headings exist in the required order;
- every inapplicable section is retained as
  `Not applicable: <one-sentence reason>`;
- one cohesive outcome, assigned model, requirement IDs, and dependencies;
- relevant current-state context;
- allowed and prohibited paths, crates, packages, and public surfaces;
- expected owners, current seams, callers, consumers, tests, and generated
  effects;
- required new or changed types, methods, responsibilities, visibility,
  invariants, and error behavior;
- typed stubs when interface or data shape matters;
- pseudocode when ordering, state, IO, transactions, concurrency, side effects,
  or error mapping matters;
- task-specific failure and edge cases;
- numbered acceptance criteria and required tests with meaningful assertions;
- executable focused recipes, required feature semantics, repository-managed
  setup, and excluded broad commands;
- controlled living-task updates for repository facts and equivalent
  non-weaker proof;
- escalation conditions and structured completion evidence.

Judge the packet from the assigned Luna, Terra, or Sol risk tier's perspective. If
the model must choose architecture, persistence, a public contract, task scope,
consequential control flow, failure semantics, dependencies/features, or proof
design, the packet is not ready. Private helper extraction,
repository-aligned names and paths, adjacent private fixtures, and equivalent
command spelling remain implementation mechanics.

Treat schema drift as material when it prevents `$wyrd-implement` from
validating or extracting the task contract. Do not waive missing or renamed
sections merely because similar prose appears elsewhere.

Do not require a stub or pseudocode for a trivial mechanic whose single
implementation is established by a named source precedent. Do require the
packet to identify that precedent and the behavior to preserve.

## Plan Traceability

At a plan gate, verify:

- every requirement maps to one or more executable tasks;
- every requirement maps to objective verification;
- tasks own cohesive outcomes and have explicit dependencies;
- each task is small enough for its assigned implementation model;
- separate task packets inherit plan decisions while repeating the concrete
  contract details needed for execution;
- focused checks cover the smallest complete affected surface;
- integrated closeout covers cross-slice behavior and public journeys;
- task and phase checks use default or exact features, while the all-feature
  workspace gate runs once at whole-plan closeout;
- Cargo-backed commands are planned sequentially in a shared checkout.

A missing routine command is not automatically Major. A missing verification
path for a load-bearing behavior is.

## Severity

### Critical

Use only for:

- credible cross-tenant access, authz bypass, secret disclosure, or audit
  bypass at a load-bearing boundary
- likely durable data loss, corruption, or unrecoverable inconsistency
- an invalid irreversible migration or incompatible persisted/wire contract
- direct contradiction of a load-bearing Wyrd decision that changes public or
  durable behavior
- an architecture that cannot satisfy its stated workflow or production goal

### Major

Use only for:

- an unresolved contract, ownership, sequencing, migration, or concurrency
  decision likely to require substantial cross-boundary rework
- a serious reliability, performance, cost, or operability failure supported by
  the stated scale or data path
- a primary developer or agent workflow that will predictably fail, require
  private knowledge, or expose inconsistent durable semantics
- an assigned Luna, Terra, or Sol task that requires material redesign, scope
  invention, or verification design before coding can begin
- a new service, crate, registry, state machine, compatibility path, or
  extension mechanism whose unjustified cost is material

All Critical and Major findings block implementation. Do not weaken the
threshold to populate a review.

## Exclusions

Do not report:

- style, wording, formatting, or preference-only naming
- localized maintainability improvements
- optional documentation or example polish
- routine codegen, formatting, lint, or test commands
- inconsequential syntax discoverable from a named current-source precedent
- missing repetition of authority or existing repository rules
- hypothetical enterprise, provider, scale, or HA concerns outside the stated
  scope
- an alternative that is merely cleaner, more elegant, or more general
- reversible local debt that does not affect durable contracts, migrations,
  security, tenancy, audit, or concurrency correctness

At most three non-gating handoff notes may capture a concrete implementation
detail likely to prevent material rework. Never label them Minor findings.

## Wyrd Authority

A written-rule violation is not automatically Critical. Grade the concrete
impact:

- A contradiction that changes a durable/public contract or load-bearing
  safety boundary is Critical or Major.
- A mechanical repository rule already enforced by CI is not an architecture
  finding unless the plan explicitly requires violating it and the consequence
  is material.
- Stale or conflicting authority is an open authority conflict, not permission
  for the reviewer to invent a decision.

## Reversibility

Safe-to-fix-later issues are local, observable, reversible, and do not persist
the wrong contract or weaken a safety boundary. Issues must be fixed before
build when they cross wire or storage contracts, migrations, tenant/auth/audit
boundaries, distributed ownership, ordering, idempotency, or irreversible data
state.

## Gate Expectations

For a spec gate, require decisions about the workflow, public/durable shape,
ownership, safety, scope, and non-goals. Do not require an implementation DAG.

For a plan gate, require a decision-complete implementation specification and
execution-rehearsed task packets. Allow CodeGraph and named local patterns to
supply existing source bodies and incidental mechanics; do not use them as a
substitute for owners, material responsibilities, interfaces, consequential
control-flow semantics, test cases, proof semantics, or executable setup.

Approve concise plans only when their tasks remain directly executable by the
assigned model. Length alone is not a readiness signal; implementation detail
that transfers consequential reasoning is.

For re-review, validate prior findings and consequences of the revision. Do not
mine unchanged sections for new optional improvements.
