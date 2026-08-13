# Decision-Complete Planning

Use this reference to convert repository evidence and product intent into an
implementation specification for the assigned Luna, Terra, or Sol model.
Decision-complete means the implementation model performs coding and local
adaptation, not design.

## Contents

- [Evidence and intent](#evidence-and-intent)
- [Material decision test](#material-decision-test)
- [Required implementation detail](#required-implementation-detail)
- [Controlled implementation adaptation](#controlled-implementation-adaptation)
- [Contracts and code shape](#contracts-and-code-shape)
- [Control flow and failures](#control-flow-and-failures)
- [Example](#example)
- [Approval test](#approval-test)

## Evidence and intent

Record:

- **Verified:** established from current source, tests, manifests, generated
  contracts, commands, or runtime evidence.
- **Assumed:** reasonable but not directly verified; safe enough to remain
  visible during implementation.
- **Unknown:** not yet established.
- **Unresolved:** a material preference or conflict that blocks approval.

Investigate unknowns before asking the user. Resolve every material unknown or
preference before approval.

Define the outcome in observable terms:

- who uses it and through which entry point;
- what new behavior succeeds;
- how likely failures appear;
- what existing behavior remains unchanged;
- what is explicitly outside scope.

Assign stable `R1`, `R2`, and subsequent requirement IDs. Requirements describe
behavior rather than edits. Use those IDs in tasks, acceptance criteria, tests,
review, and closeout.

## Material decision test

A choice is material when different reasonable answers change:

- public, internal cross-owner, or durable behavior;
- crate, package, service, store, or dependency ownership;
- wire, storage, generated, SDK, CLI, MCP, or UI contracts;
- tenant, authn, authz, policy, audit, or secret boundaries;
- validation, atomicity, ordering, consistency, idempotency, or concurrency;
- compatibility, migration, rollout, cancellation, retry, or recovery;
- performance or operational behavior at the stated scale;
- build features, dependencies, or the verification needed for correctness.

Resolve material choices in the plan. The assigned implementation model may
choose reversible local mechanics such as variable names, a small private
helper, and exact syntax already determined by the nearest repository pattern.

## Controlled implementation adaptation

Decision-complete does not mean mechanically immutable. The plan locks
consequential behavior and gives the implementer explicit authority over
reversible repository alignment.

The implementer may update the active task when current source establishes:

- a corrected internal path or private symbol name;
- an equivalent existing helper, fixture, or test-support seam;
- a small adjacent private helper or fixture inside the established owner;
- an equivalent verification command that proves the same acceptance outcome
  without weaker coverage;
- progress, diagnostic evidence, and verification results.

The implementer must not independently change:

- requirements or acceptance outcomes;
- public, wire, generated, cross-owner, or persisted contracts;
- migrations, destructive behavior, or data-loss semantics;
- authentication, authorization, tenancy, policy, audit, or secret semantics;
- dependencies, Cargo features, or ownership boundaries.

Classify the former as local mechanics or bounded corrections and record them.
Classify the latter as material and return for authority before implementation.
Do not create a remediation plan for a bounded correction.

## Required implementation detail

For each task, specify enough detail to prevent re-planning:

- expected paths, modules, owners, and target symbols;
- existing seams to reuse and callers that must remain compatible;
- new or changed types, fields, variants, methods, functions, and visibility;
- responsibility of each new or materially changed symbol;
- important input, output, ownership, lifetime, and error shapes;
- operation order and state or IO boundaries;
- positive, negative, edge, concurrency, and recovery cases;
- required tests and their intended assertions;
- exact focused commands, features, and prohibited broad commands.

Do not produce line-by-line source. Do not omit code structure merely because
CodeGraph can rediscover the repository. Source discovery establishes where
the work goes; planning decides what must be built there.

When current source completely determines a local mechanic, name the precedent
instead of restating it. Example: “Follow the constructor and error-mapping
shape used by `Cards`; do not introduce a trait.”

## Contracts and code shape

Define all new or materially changed interfaces that cross a public, durable,
owner, or task boundary:

- typed identities and domain structures;
- request, response, schema, storage, and generated wire shapes;
- concrete owning structs and their dependencies;
- method signatures and responsibilities;
- validation boundaries and stable errors;
- transaction and cross-store commit boundaries;
- event, observation, audit, relationship, and status timing;
- compatibility and migration behavior;
- exact Cargo features and dependency direction.

Use typed stubs. Mark semantics as normative and incidental naming or layout as
illustrative when repository inspection may require adaptation.

```rust
/// Illustrative shape; result semantics are normative.
pub struct SourceChecks {
    client: WyrdClient,
}

impl SourceChecks {
    /// Runs a server-owned check for an exact Source reference.
    pub async fn check(
        &self,
        source: &CardRef,
    ) -> Result<SourceCheckReport, WyrdSdkError>;
}

pub struct SourceCheckReport {
    pub source: CardRef,
    pub outcome: SourceCheckOutcome,
    pub issues: Vec<SourceCheckIssue>,
}
```

A task using this example must still identify the actual current owner,
existing client transport seam, error type, and generated-surface obligations.

## Control flow and failures

Provide pseudocode whenever correctness depends on ordering, transactions,
state transitions, side effects, error mapping, concurrency, or recovery.

Good pseudocode shows:

- validation and authorization order;
- state reads and writes;
- atomic and non-atomic boundaries;
- side-effect and audit timing;
- public error mapping;
- retry, cancellation, or reconciliation behavior;
- return behavior after partial progress.

```text
check_source(context, exact_ref):
    authorize context for cards:read and sources:check
    resolve exact_ref within context.tenant
    if absent or belongs to another tenant:
        return tenant-safe not_found
    if resolved card is not Source:
        return stable wrong_kind error

    report = source_checker.check(resolved.spec)
    append scrubbed audit outcome
    return report
```

For stateful or externally visible behavior, define a compact failure matrix:

| Condition | Public behavior | Durable effect | Requirement |
|---|---|---|---|
| Valid request | Typed success | Expected committed effect | `R1` |
| Duplicate or replay | Defined result or conflict | No duplicate effect | `R2` |
| Permission failure | Stable denial | No protected side effect | `R3` |
| Dependency failure | Stable error or recoverable state | No false success | `R4` |

Do not add transaction or recovery machinery to a pure localized refactor.
Proportionality changes how much detail is required, not whether the task must
be executable.

## Example

The following is an illustrative planning fragment, not Wyrd authority:

```markdown
### Decision D2: Exact Source references only

The check endpoint accepts `CardRef` with an exact version. The server resolves
the reference through the existing tenant-scoped registry owner. It does not
accept version requirements or perform client-side resolution.

Required code shape:

- Add `SourceCheckRequest { source: CardRef }` to the existing pure contract
  owner.
- Reject non-exact references during request validation with the established
  invalid-reference Wyrd error.
- Add `Sources::check(&CardRef)` to the existing SDK handle; it only projects
  the HTTP contract.

Normative pseudocode:

    validate exact CardRef
    authorize before resolution
    resolve tenant-locally
    delegate probe to server-owned checker
    return typed report

Rejected: accepting a name and resolving the latest version in each SDK,
because that creates language-specific durable behavior.
```

This is sufficient only when the task also names the actual paths, existing
symbols, acceptance criteria, tests, and commands.

## Approval test

Before approval, confirm:

- every requirement is observable, necessary, and mapped to a task;
- every task is sized for its assigned implementation model;
- non-goals, allowed scope, and prohibited changes prevent plausible drift;
- current-state claims cite live source evidence;
- all material choices have one answer;
- required types, interfaces, responsibilities, and invariants are explicit;
- consequential control flow and failure behavior are specified;
- tests and verification prove each acceptance criterion;
- reversible mechanics and material decisions have an explicit authority
  boundary;
- assumptions do not conceal missing design;
- `$wyrd-implement` can execute every task without making a material decision.

If any item fails, keep the plan `Draft` and continue investigation or
decision resolution.
