# Conformance and Adaptation

Use this reference for the implementation-to-plan comparison and finding
classification.

## Traceability

For every in-scope requirement and acceptance criterion, identify:

| Requirement / AC | Implementation owner | Test / artifact | Verification | Result |
|---|---|---|---|---|

Use `PASS`, `FAIL`, or `UNVERIFIED`. A changed file without a requirement,
acceptance criterion, or necessary verification role is scope drift.

Check:

- observable behavior and non-goals;
- ownership, interfaces, invariants, ordering, transactions, side effects,
  errors, retries, idempotency, cancellation, and recovery where material;
- negative cases and required test tier;
- affected callers, consumers, generated projections, and support surfaces;
- prohibited material changes and later-task scope;
- living-task execution updates and their repository evidence.

## Adaptation classes

### Local mechanic

Accept without plan mutation when semantics and ownership remain intact:

- private helper extraction or local organization;
- current private path or symbol equivalent;
- mechanical caller/test updates;
- rustdoc, formatting, or diff-caused lint repair;
- use of an existing fixture or support export.

### Bounded correction

Accept when independently supported and recorded in the living task:

- an equivalent non-weaker command, target, or filter;
- a narrow adjacent private fixture or support file inside the established
  owner;
- repository-managed local services, migrations, or test variables;
- a small touched-file defect repaired during implementation;
- a proven unrelated baseline failure that did not prevent required proof.

These are not `UNAPPROVED_DEVIATION`. If one remains incomplete, use
`RESUME_IMPLEMENTATION`.

### Material deviation

Use `REPLAN_REQUIRED` when correctness requires changing:

- a public, wire, generated, or persisted contract;
- a migration or destructive/data-loss behavior;
- a dependency or Cargo feature;
- authentication, authorization, tenancy, policy, audit, or secret semantics;
- owner or dependency direction;
- requirements, scope outcomes, or acceptance criteria.

Private file lists and helper names become material only when the plan
explicitly makes their exact shape normative for a stated safety or ownership
reason.

In `$wyrd-implement-plan` binding mode, return `ROOT_DECISION_REQUIRED` instead
of `REPLAN_REQUIRED`. The root implementer owns the material decision,
canonical revision, advisory plan review, implementation, and verification.

## Finding classifications

- `MISSING_IMPLEMENTATION`: an approved behavior or acceptance criterion is
  absent.
- `IMPLEMENTATION_DEFECT`: source or tests implement approved intent
  incorrectly.
- `BOUNDED_CORRECTION_REQUIRED`: reversible work or proof remains inside the
  approved boundary.
- `MATERIAL_PLAN_CONFLICT`: repository evidence invalidates a material plan
  decision or implementation changed one.
- `SCOPE_DRIFT`: unrelated, prohibited, or later-task work entered the diff.
- `MISSING_VERIFICATION`: mandatory proof is absent or weaker than required.
- `REPOSITORY_RULE_VIOLATION`: current Wyrd authority is violated; classify
  the remedy as bounded or material from its concrete consequence.

Consolidate duplicate symptoms under one root cause. Every finding must name
the exact authority, source evidence, concrete impact, required outcome, and
verification needed. Do not prescribe a new material design during review.

## Task-state interpretation

- `Ready`: eligible for an active or legacy implementation review.
- `Complete`: expected for completion review; validate the status claim.
- `Blocked`: review only the blocker claim and completed unaffected work.
- `Planned`: not eligible for implementation-conformance approval.

Do not reject a valid implementation merely because the task recorded
repository facts, private paths, fixtures, commands, or progress through its
controlled execution-update mechanism.
