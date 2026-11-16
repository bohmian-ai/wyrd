# TASK-001 PostgreSQL boundary audit

## Subject and current authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Immutable candidate: `cd905f4cd929785bda25696326d58b4a4ae88dee`.
Original base: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb`.
Use `git diff <base>...<candidate>` for the implementation, with unchanged
reachable dependencies available as context. No source changes or remediation
implementation are authorized by this audit.

This is a focused continuation of the TASK-001 review after explicit user
corrections on 2026-11-06, not a fresh repository-wide review or implementation
remediation round. The user requests an audit of PostgreSQL usage against the
approved plan/spec and a remediation task listing every deviation.

Current user instructions override contradictory older text:

- The new `vala.scribe_state_heads` table and its per-execution SQL ownership /
  accepted-checkpoint authority are rejected as a deviation from the design.
  Moving that table's writes into a background Scribe task does not satisfy
  this instruction. Record the directed removal as USER-PG-001, separately
  from independently discovered review findings; do not retroactively claim
  the prior review identified that design violation.
- Execution state and accepted recovery position belong to Bifrost through
  the existing records, shared writer, WAL and batch-commit machinery.
- The user accepts the rare delayed-old-worker/replacement timing gap just
  discussed and does not want a new PostgreSQL ownership/lease protocol to
  close it. Do not resurrect the old FIND-TASK-001-20 demand for a new
  queue-to-Scribe authority handoff. This accepts that specific residual risk;
  it does not authorize unrelated data loss, cross-tenant access, or removing
  normal queue cancellation/renewal/deadline controls.
- Direct final input/output/result delivery intentionally does not await
  Bifrost acknowledgement before returning the result. FIND-TASK-001-18 was
  removed. Do not require that wait in the audit or remediation.

The existing approved spec18/supplement2/TASK-001 are the baseline for other
boundaries. The active packet and architecture amendments are uncommitted
inputs; hash them and preserve them. Do not change authority or source during
review. Earlier unchanged evidence is carried forward only for the contracts
it actually covers.

## Audit scope and navigation

Audit the entire verification workflow touched by TASK-001, not just new SQL
lines: direct entry, submitted/scheduled/group admission, poller/runner,
engine/checkpoints/publications, canonical inputs/results/state reads, public
inspection/content/cancel, terminal/error handling, and the Bifrost adapter,
SQL/fence/WAL and existing background/control dependencies those paths call.
Follow each PostgreSQL access to its actual query/table and field use. Include
reads, writes, migrations, stored-function/trigger calls and indirect calls;
classifying a whole module or every queue column as allowed is insufficient.
Unrelated platform SQL is out of scope unless reachable from these workflows.

Before source searches use CodeGraph. Root AGENTS and relevant nested rules
apply, including `crates/wyrd/wyrd-sql/AGENTS.md`. Read applicable architecture,
`changes/spec-driven-development.md`, maintainer/Rust guides and TESTING.

Starting owners (expand from actual calls):

- `verification/state.rs`: RunState claim/fresh/accepted/read, StateScribe
  confirmed/write, Held/Pending, and their service/runner/hosted consumers.
- `components/verification/service.rs`, `service/inspection.rs`,
  `verification/{runner,inputs,results,leases,submissions,schedules,groups}.rs`.
- `wyrd-sql::queries::verifier_executions` and admission/renewal/cancellation/
  principal-revocation/binding/group/Operator dependencies.
- New `vala-sql::scribe_state_heads` migration/queries; changed
  `scribe_batch_commits`, Bifrost contracts/ingress/shards/replay, transport
  proto and state-fenced verification table definitions.
- Existing Scribe batch metadata/catalog/WAL admission and source recovery
  machinery, compared at original base and candidate: distinguish existing
  generic Bifrost internals from added per-execution coordination authority.

Likely allowed categories must be verified against exact authority and actual
fields, not treated as a blanket exemption: queue admission/claims/leases/
renewal/cancellation/retries, accepted initial input custody, existing auth/
registry/File control-plane operations, bindings/groups/independent Operator
coordination, and pre-existing generic Bifrost catalog/batch-commit metadata.
Recoverable execution payloads, selected results/full errors, publication
custody and execution checkpoint-head authority must not be quietly assigned
to queue SQL or a new generic-sounding SQL table.

## Carried context and required reports

Previous review: `../task-001-bifrost-cd905f4cd/`.
Carry forward its immutable subject and independent verification (25 state/
outbox units, 1 engine checkpoint regression, 68 server integration tests, fmt
and diff checks), with their actual assertion limits. It does not prove the
new ownership audit. Historical finding18 is withdrawn. Finding20's proposed
new authority-handoff decision is superseded by the user instruction above.
Finding25's relocation-only correction is superseded: rejected SQL authority
must be removed, not relocated. Other prior findings are unchanged context;
trace only any specifically affected PostgreSQL ownership gap, not the whole
original review again.

Discovery assignments, independent and fresh:

| Role | Boundary / reason | Exact report |
| --- | --- | --- |
| repo-rev | PostgreSQL call-site/table/field inventory and applicable owner permissions throughout the workflow, including indirect operations and request/background placement | `postgres-usage-audit.md` |
| invariant-rev | Execution/recovery authority from producer to consumers: SQL versus Bifrost state, terminal/error/publication/identity and accepted-position reliance; existing-owner removal/reuse consequences | `execution-storage-audit.md` |

Both reports must contain an explicit per-operation inventory with caller,
query/table/fields, workflow phase, read/write, allowed/deviation, governing
source, introduced/pre-existing status and consequence. Expand relevant
callers/consumers and group shared causes rather than one finding per caller.
Propose only source-backed material deviations or explicit mandatory rules,
with smallest safe correction/proof and PASS/FAIL/BLOCKED; report coverage and
limits. Do not read each other's report or exchange conclusions. Root only.
No implementation, no Cargo runs or invented scripts; write only your assigned
report. You are not alone; preserve other work.

After discovery the orchestrator compares claims and a fresh ponytail-rev
independently validates source, authority, applicability, prior finding changes
and minimum correction burden, writing `findings-validation.md`. The output is
an audit verdict and one self-contained remediation task; user-directed
removal stays separately identified from independently validated findings.
A bounded read-only comprehension pass checks the resulting packet before
handoff. No new table/service/queue/writer or stronger rare-gap guarantee may
be smuggled into remediation. If the current architecture text contradicts
user authority, name exact amendments in the task instead of silently leaving
conflicting mandates for the implementer.

## Specialist applicability for this continuation

A fresh domain-data specialist is additionally selected for persisted metadata
ownership and the removal/reuse boundary at existing batch-commit/WAL/recovery
owners; its report is `bifrost-data-audit.md`. The original data report's
claim that the new table is acceptable cannot carry forward under the user's
rejection. Original RLS/generic batch atomicity/replay source evidence remains
available only for unchanged pre-existing mechanisms.

No fresh comprehensive behavior, security, system, maintainer or concurrency
team is selected: this is an authority/SQL-use audit on identical source, not a
new implementation review. Prior reports cover unchanged public grants/File
custody, runtime/caller/lifecycle behavior, short-lock/shared-future invariants
and whole-workflow owner reuse. Specific affected identity/terminal/request-SQL
and removed-head dependencies are assigned to the fresh inventory/invariant/data
passes. The directed acceptance of the rare worker gap supersedes its earlier
hardening demand; it does not require another discovery pass seeking the same
risk. The fresh validator must verify that this targeted coverage and these
carry-forward limits are sufficient; a real new boundary gap requires targeted
coverage, not an automatic whole-task restart.
