# Bifrost errors — round-5 verdict

**FIX_REQUIRED — FIND-bifrost-errors-5.** Prior FIND-1 through FIND-4 are closed.

## Immutable subject

Wyrd candidate `8a54d81c346db464b624e81ebe8958c63bb7e228`, integration
baseline `945a1548c47897dc94583169a6ba04163a73a749`; original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79`, previous reviewed candidate
`17697dc8c2740213565e5f913cde1edfb0cb8ce5`. The cumulative task diff is
integration baseline to candidate, retaining all Bifrost implementation and
excluding main work already incorporated in the prior resumed review.
Native fork cumulative base `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`,
previous `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`, current
`8c3f18e9806ae23e8840a710af775c058dfad66a`. Distributed fork unchanged:
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Authority: [approved revision-1 spec](../../spec.md), owner directions,
repository/native rules and applicable architecture, prior verdicts and
R1–R4 remediations. No original implementation task existed. The new owner
question about recurring causes was handled with the 5why skill alongside
the acceptance audit; it did not cancel the review. See [subject](subject.md)
for starting map, boundaries and verification restrictions.

## Independent results and reconciliation

| Fresh role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Standards | [standards-review.md](standards-review.md) | PASS |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| DataFusion domain | [domain-review-datafusion.md](domain-review-datafusion.md) | FAIL |
| Tenancy domain | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

BEH-R5-1, INV-R5-1 and DF-R5-1 propose the same producer/sibling-consumer
regression. [Claim comparison](claim-comparison.md) found no material conflict
or untraced shared source requiring additional focused discovery. The native
primary-key and grouped-key reproduction resolved the reachability uncertainty.
Validation independently confirmed the finding, revised its correction to
cover the ordinary pure expression family together, and assigned FIND-5.
Scoped PASS reports do not establish compatibility of every bound consumer;
their general runtime-fallback statements are limited to consumers that have
that fallback. Root packages the independent ledger, not a self-validated finding.

## Reconciled acceptance matrix

The two implementation reports contain full criterion matrices.

| Obligation | Implementation and verification assessment | Result |
|---|---|---|
| REQ-002 actionable planning 4xx, internal redaction | Typed cause mapping and first-class/MCP assertions retained; authorization precedes provider suggestions | PASS |
| REQ-006 scalar/IN/EXISTS/live run_id cases | Native query paths and shipped surface journeys retained | PASS for diagnosed original cases |
| REQ-006/owner/R4 preserve valid correlated queries | Fixed outer CASE loses proof; correlated analyzer refuses before runtime fallback | FAIL |
| INV-001, applicable AC-002 authorization negatives | Existing identity/cut/fragment owners and redaction tests retained | PASS |
| AC-001 journeys on shipped surfaces | Existing real SDK/server/MCP paths retained, current focused peer proof assessed | PASS within recorded evidence |
| R1 selective predicate transfer | Native readiness/snapshot, Pre/Post capture, first-poll resolution, route ownership; exact literal/two-footer assertions | PASS |
| Compact common-query corpus and joined CTE | Fixed two-table mixed-state cases replace broad framework; explicit expected rows and native tools | PASS |
| R2 global scalar cardinality/codec | single_value zero/one/multiple semantics and native partial/final merge retained | PASS |
| R3 FULL JOIN/GROUPING SETS/overflow | Sound conservative bounds and checked arithmetic; focused native/peer controls | PASS |
| R4 row-dependent/volatile expressions cannot grant singleton proof | Closed positive classifier correctly rejects hidden input dependence; count/abs filter/projection refuse | PASS |
| R4 sound fixed positive lookup compatibility | Direct/arithmetic/cast/function controls pass, but omitted ordinary pure forms regress correlated analysis | FAIL |
| FIND-4 rustdoc and AC-003 focused gates/pins | Required docs present; pin/lock/codec agree; recorded nonzero checks assessed | PASS within stated limits |
| Non-goals/performance scope | Native owners retained, no new protocol/evaluator/query generator; structural transfer proof, no benchmark claim | PASS |

## Final ledger and prior closure

**FIND-bifrost-errors-5 — CONFIRMED, correction REVISED — REGRESSION.**
Native `datafusion/expr/src/logical_plan/plan.rs:2963–2987` rejects every
CASE as unproven even when all children are fixed outer references/literals.
`logical_plan/invariants.rs:188–194` requires a proven <=1 bound for the
nonaggregated correlated lookup and therefore rejects the query. Unlike
the uncorrelated shared scalar rewrite, this consumer has no runtime
single_value fallback. Valid rows become a planning error.

The validated correction extends the same positive Filter proof to ordinary
pure CASE, BETWEEN, IN-list, LIKE/SIMILAR TO and null/truth predicates only
when all value-bearing children are fixed. Unsafe/opaque forms remain unproven.
No weakened analyzer, Wyrd guard, evaluator, new proof type or protocol is needed.

- FIND-1 stays CLOSED: diagnosed selective predicate loss is repaired and
  bounded literal/footer transfer remains asserted.
- FIND-2 stays CLOSED: broad workload machinery/dependency promotion is absent;
  compact fixed common-query coverage remains.
- FIND-3 is now CLOSED: global scalar validation and the false FULL/grouping/
  row-dependent-filter bounds are repaired. FIND-5 is a new compatibility
  consequence, not a relabeling of successful wrong scalar output.
- FIND-4 stays CLOSED: required Panics docs and whitespace repair remain.

## Causal analysis and proof limits

The [validated 5why consolidation](findings-validation.md) distinguishes
independent original error-mapping/predicate-transfer issues from the connected
scalar chain: missing row-count enforcement, then false bounds bypassing it,
then incomplete positive proof rejecting valid correlation. The last two use
the same guaranteed-bound authority. Current tests cover selected positive
forms and uncorrelated fallback but omit pure CASE through the correlated
admission consumer. This explains the coverage gap without inventing a deeper
organizational cause or requiring a new testing framework.

One focused native fixture was executed with an existing binary, tracing on
and normal single-thread selection. Final NOT NULL fixture selected 1/1 files:
direct primary-key/grouped controls passed, both CASE variants failed analysis,
exit 1. The earlier nullable fixture mistake is explicitly excluded from proof.
Exact fixture/command/log and binary mtime/size limits are in the behavior and
validation reports. No rebuild/hash attestation or new live Wyrd execution is
claimed; current source independently establishes the failure and public
grouped-key reachability.

Recorded implementation native suites, focused Wyrd selectors (2/4/3/1) and
checks credibly cover their assertions, not this missing case. Cumulative Wyrd
and fork diff-checks pass. No benchmarks, synthetic load, builds or broad lanes
were run during review. The historical untraced memory-reservation observation
remains unexplained and is not a new finding. Execution-time cardinality's
opaque 500 remains outside the approved planning-detail contract.

## Remediation

Route [bifrost-errors-R5-preserve-fixed-correlated-expressions.md](bifrost-errors-R5-preserve-fixed-correlated-expressions.md)
directly to `$wyrd-implement`, then re-review the complete cumulative candidate.
Review did not implement, merge, commit, push or deploy candidate changes.
