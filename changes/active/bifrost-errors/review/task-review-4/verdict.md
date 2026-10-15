# Task review round 4

## Verdict

**BLOCKED** — the source candidate changed during discovery. This is not a
task acceptance result. No remediation task or independently validated new
finding ledger is produced in this interrupted round.

## Subject and immutable-source violation

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
- Original base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Assigned candidate: `7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`.
- Observed replacement HEAD: `17697dc8c2740213565e5f913cde1edfb0cb8ce5`.
- Replacement commit: `Merge remote-tracking branch 'origin/main' into split/bifrost-errors`.
- Replacement parents: assigned candidate and `945a1548c47897dc94583169a6ba04163a73a749`.
- Pinned fork: `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`, unchanged.
- Distributed fork: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.
- Approved spec: `changes/active/bifrost-errors/spec.md`, revision 1.
- Original task: none; spec and owner directions govern.
- Remediation reviewed: `review/task-review-3/bifrost-errors-R3-make-scalar-cardinality-bounds-sound.md`.
- Prior verdicts/ledgers: `review/task-review-1`, `-2`, `-3`.

HEAD matched the assigned candidate at discovery start. Behavior and domain
reviewers subsequently detected the replacement; orchestration confirmed it.
The merge changed 131 paths including shared server state, fixtures, mise lanes,
contracts, SDKs and dependency lock state. Source reads cannot be treated as
one completed immutable-candidate audit. The skill expressly requires BLOCKED
if the candidate changes during review. Discovery was stopped, without resetting
or altering the user's merge or source. Only this review directory was written.

## Acceptance and discovery status

| Obligation group | Available evidence | Acceptance status |
|---|---|---|
| Original error mapping, authorization, tenancy and public surfaces | Cumulative discovery source and retained test evidence | Not finalized: immutable subject invalidated |
| Native snapshots, runtime filter readiness, route ownership and compact corpus | Prior validated ledgers plus round-4 discovery | Not finalized: prior closure is not newly certified |
| R3 FULL JOIN/grouping/overflow proof repair | Latest fork source, native bound/SQL tests and committed peer evidence | Appears corrected in discovery; not a final closure result |
| Complete sound scalar singleton proof | Discovery identified a sibling Filter::is_scalar proposal and focused reproduction | Proposed issue remains unvalidated in this round |
| Format/lints/codegen/boundary and cumulative diff checks | Committed R3 command evidence; original candidate diff-check clean at start | Not certification of the replacement candidate |
| Independent validation and reconciled finding ledger | Not started after immutable-source violation | Missing; BLOCKED |

| Required report | Recorded discovery result | Use in this blocked round |
|---|---|---|
| task-review-behavior.md | BLOCKED | Preserves source-change evidence and proposed scalar proof issue |
| task-review-invariants.md | BLOCKED | Preserves producer/sibling proposal and limits |
| standards-review.md | PASS at discovery scope | Preliminary evidence, not overall acceptance |
| maintainer-review.md | PASS at discovery scope | Preliminary evidence, not overall acceptance |
| system-review.md | PASS at discovery scope | Preliminary evidence, not overall acceptance |
| domain-review-datafusion.md | BLOCKED | Preserves focused reproduction/provenance and proposed gap |
| domain-review-tenancy.md | PASS at discovery scope | Preliminary evidence, not overall acceptance |

No follow-up or structured Ponytail validation was started after discovery
lost its immutable subject. The orchestrator has not promoted a discovery
proposal into a validated finding or written implementation remediation from
it. Existing FIND IDs and earlier ledgers remain intact.

## Evidence preserved for the next immutable review

The domain reviewer ran one authorized normal-concurrency traced SQL file
outside both candidate repositories, with no compilation or benchmark. Its
direct single_value control refused two rows, but a scalar over a grouped
unique key filtered by a row-dependent expression succeeded instead of
refusing. Trace retained the filter and omitted single_value. The proposed
common source is Filter::is_scalar accepting unique-column equality to a
non-Column expression without proving invocation-constant value; max_rows
then treats it as singleton and the shared scalar rewrite skips validation.

This is a proposal relevant to prior FIND-3, not a validated additional ID.
Exact query, command, selected-file result and existing-binary provenance are
in domain-review-datafusion.md; output is retained at
`/tmp/bifrost-r4-filter-proof/output.log`. A fresh validator on a stable subject
must determine scope and correction at the owning proof helper rather than
adding a Wyrd/query-shape guard. No pass is implied by the implementation's
existing green suites, nor is the proposal packaged as final remediation here.

## Verification limits and next review subject

No benchmark, synthetic load, raised concurrency or broad verification rerun
occurred. The one focused reproduction used an existing fork binary with
recorded provenance, not a new rebuild. No diagnosis is added for the earlier
untraced reservation observation.

A new task-review invocation must establish the intended immutable Wyrd commit
(the merged commit if that is now the intended target) and its pinned fork,
then finish cumulative acceptance and independent validation without candidate
movement. This review does not modify or roll back that target.

Finding IDs: none independently validated in this interrupted round.
Remediation task: none.
