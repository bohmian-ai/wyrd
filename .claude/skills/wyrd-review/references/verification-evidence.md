# Verification Evidence

Use this reference to judge command results, equivalent proof, environment
recovery, and verification failures.

In `$wyrd-implement-plan` or terminal integration-binding mode, audit only
recorded evidence and source-visible command/test definitions. Do not execute
commands or perform independent verification; the controlling orchestrator
owns any required rerun.

## Evidence hierarchy

Prefer, in order:

1. trustworthy recorded output tied to the reviewed diff;
2. source-validated test and task definitions;
3. targeted independent non-mutating checks needed to resolve a material
   uncertainty.

Do not rerun expensive or environment-backed commands merely to duplicate
credible evidence. Rerun when evidence is stale, incomplete, internally
inconsistent, or does not identify the relevant target, features, tests, or
result.

## Equivalent proof

A replacement command is non-weaker only when it:

- proves the same acceptance criterion;
- exercises the same relevant code path, target, and features;
- preserves the required unit, integration, or user-journey tier;
- preserves negative flows, environmental dependencies, assertions, and
  consumers;
- records the original recipe defect and corrected command in the living task.

Examples include selecting `--lib` when an unrelated explicit integration
target makes the original Cargo recipe invalid, correcting a stale filter that
selects zero tests, or using the owning `mise` task that provisions Postgres
instead of a raw command with an empty shell environment.

Do not accept a unit test in place of a required integration or user journey,
or a command that silently drops relevant features, assertions, negative
flows, or consumers.

## Failure routing

| Evidence state | Verdict contribution |
|---|---|
| Diff-caused compile, test, Clippy, rustdoc, or format failure | `RESUME_IMPLEMENTATION` |
| Missing repository-managed setup or local test variables | `RESUME_IMPLEMENTATION` |
| Defective recipe with equivalent proof passing | Accept bounded correction |
| Proven unrelated baseline failure | Record; continue unaffected conformance without a new finding or follow-up unless baseline repair is explicitly in scope |
| Mandatory proof unavailable with no equivalent | `REVIEW_BLOCKED` |
| Proof exposes a material contract or architecture conflict | `REPLAN_REQUIRED` |

An unset local environment variable is not a planning defect when `mise.toml`
or checked-in setup defines it. A task-created Clippy failure is implementation
work. An unrelated Clippy failure does not erase passing focused evidence.

## Coverage integrity

Confirm that:

- test filters select the named tests rather than succeeding with zero tests;
- required setup, fixtures, feature gates, and exports are reachable;
- assertions prove the behavior and failure mode named by the acceptance
  criterion;
- generated checks originate from their source;
- no ignored test, weakened assertion, lint allowance, mock substitution,
  sleep, or swallowed error hides the behavior under review.

Record exact commands and results, but judge their semantic proof rather than
their textual identity.
