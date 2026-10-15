# Bifrost errors — round-6 task verdict

**PASS. No retained material findings. FIND-bifrost-errors-1 through -5 are closed.**

## Immutable subject and authority

- Wyrd candidate: `95b350d078226e811ac298230c233bf935975d42`.
- Cumulative integration baseline: `945a1548c47897dc94583169a6ba04163a73a749`.
- Original task base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Previous reviewed candidate: `8a54d81c346db464b624e81ebe8958c63bb7e228`.
- Native cumulative base: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`.
- Current native fork: `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`.
- Previous native fork: `8c3f18e9806ae23e8840a710af775c058dfad66a`.
- Distributed fork unchanged: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Reviewed the complete integration-baseline-to-candidate change and cumulative
native fork, using the latest delta to locate R5 closure. Original history and
main integration remain in earlier subjects; no new merge changed this round.
Both source HEADs remained unchanged throughout review.

Authority: [approved revision-1 spec](../../spec.md), applicable repository and
native rules/design/testing guidance, owner directions, all prior verdicts,
ledgers and R1–R5 remediations. No original implementation task existed.
The latest task is [R5](../task-review-5/bifrost-errors-R5-preserve-fixed-correlated-expressions.md).
The owner requires compact common-query coverage, native root corrections,
preserved valid queries, safe proof construction and no benchmarks.
See [subject](subject.md) for the navigation map and execution boundaries.

## Independent review and follow-up decision

| Fresh role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | PASS |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | PASS |
| Repository standards | [standards-review.md](standards-review.md) | PASS |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| Native DataFusion/OLAP | [domain-review-datafusion.md](domain-review-datafusion.md) | PASS |
| Tenancy/security | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | PASS; explicitly empty ledger |

All discovery ledgers were empty. [Claim comparison](claim-comparison.md)
identified no material disagreement, unreviewed reachable path or untraced
shared cause requiring another focused discovery agent. The fresh validator
nevertheless independently inspected source, full cumulative diffs, callers,
authorities, test assertions and recorded proof. Agreement was not treated
as validation. Root packages that independently validated result.

## Reconciled acceptance matrix

The behavior and invariant reports preserve full criterion matrices.

| Obligation | Implementation and verification evidence assessed | Result |
|---|---|---|
| REQ-002 actionable caller planning 4xx | Typed logical cause traversal retains stripped planner detail; exact unit and Rust/Python/TS/MCP single/join/subquery assertions | PASS |
| REQ-002 internal redaction and authorized suggestions | Physical planning uses fixed safe detail; execution remains opaque; table authorization precedes provider registration; forbidden-column negatives retained | PASS |
| REQ-006 scalar/IN/EXISTS and live run_id correlation | Native query owners retained, managed correlation fixtures and first-class/MCP expected-row-versus-join journeys | PASS |
| INV-001 and applicable AC-002 | Authenticated cut, scoped permissions, binding-preserving route pruning and authenticated fragment preflight unchanged; negative disclosure proof retained | PASS |
| AC-001 strongest shipped-surface journeys | Real SDK/server/MCP fixtures retained; current public remote positive and negative queries exercise the changed native proof | PASS within recorded execution evidence |
| R1 selective predicate transfer | Native readiness/snapshot, two-pass hash-key extraction, Pre/Post capture, first-poll resolution and partition-before-prune ownership; exactly two fragments/two footer rows and static/resolved literals asserted | PASS |
| Owner common-query coverage, joined CTE | Compact fixed mixed published/live two-table corpus with explicit join/subquery/group/window/NULL/CTE answers; broad generated oracle removed | PASS |
| R2 global scalar cardinality | Registered native single_value in shared uncorrelated Filter/Projection rewrite; row-count partial/final merge including equal/NULL; native codec mechanism retained | PASS |
| R3 sound FULL/grouping/overflow bounds | Conservative unknown for expanding cases and checked products/sums; native bound/SQL and peer refusal controls | PASS |
| R4 unsafe opposing values cannot produce singleton proof | Positive Filter proof rejects input dependence, volatility and opaque scopes; count/abs Filter/Projection and six peer refusals retained | PASS |
| R5 ordinary fixed correlated expressions remain valid | Approved pure family uses native complete child traversal; 23-shape fixed/inner/volatile matrix, both equality orders, PK/grouped/sibling SQL positives and negatives | PASS |
| R5 public grouped CASE lookup | Existing live join journey reuses one client and asserts matched 107→108 plus unmatched 199→NULL | PASS |
| FIND-4 documentation and AC-003 focused checks | Panics docs retained; pin/lock/codec agree; exact recorded unit/SQL/journey and format/lint/generated/boundary checks assessed | PASS within limits below |
| Non-goals and performance scope | Existing native/Wyrd owners reused; no evaluator/framework/new protocol/consumer guards or analyzer weakening; structural pruning/transfer proof | PASS |

Unrelated Jev/traceparent clauses inherited from the parent split do not expand
this Bifrost task into adjacent implementations; their behavior is untouched.

## Final ledger and prior closure

**Retained finding ledger: empty. No remediation task is required.**

- **FIND-bifrost-errors-1 — CLOSED:** available selective predicates survive
  until native values are ready and travel as literals; planned partition
  ownership precedes runtime narrowing. Closure covers the diagnosed equality
  transfer path without claiming universal inequality optimization.
- **FIND-bifrost-errors-2 — CLOSED:** generated workload/public oracle machinery
  and dependency promotion are absent; compact common-query coverage remains.
- **FIND-bifrost-errors-3 — CLOSED:** native global row counting plus corrected
  FULL/grouping/overflow and row-dependent Filter bounds enforce the diagnosed
  scalar semantics without false singleton bypasses.
- **FIND-bifrost-errors-4 — CLOSED:** required mapper/MCP Panics documentation
  and earlier whitespace repair remain.
- **FIND-bifrost-errors-5 — CLOSED:** ordinary pure fixed expressions retain
  correlated admission, while all value-bearing children must independently
  be fixed. Unknown/opaque scopes and volatile/input-dependent descendants
  cannot grant proof. The native analyzer/rewrite/runtime remain unchanged.

The shared Filter producer now satisfies both relevant consequences of its
proof: valid correlated admission and safe uncorrelated runtime-check omission.
Native child traversal covers CASE base/conditions/results/ELSE and every
list/pattern/range/truth operand. No query-specific Wyrd patch survives.

## Verification evidence and limits

Latest implementation evidence records the exact fixed-value unit, 1/1 native
subquery SQL file, expr265, optimizer886, single_value4 and proto **library17**,
plus Wyrd focused counts 2/4/3/1 and formatting/lints/codegen/dependency/tenant
checks. These were assessed against source and assertions, not rerun. Library17
is not represented as a fresh run of the historical external proto265 suite;
no current all-520 SQL run is claimed. Older results support unchanged portions
without being relabeled as current-pin executions. Cumulative Wyrd and native
diff-checks pass on the reviewed commits.

The documented first duplicate-key journey failure matches repeated API-key
provisioning in the existing helper. Reusing one client fixes that setup cause
and preserves query/assertion/timeout/retry behavior. Passing-run trace counts
were not captured and are not claimed as proof.

This review ran no builds, executable test reruns, broad lanes, benchmarks or
synthetic load. Performance assessment is structural: native planning-time
proof, retained singleton fast paths, build readiness, static pruning and
bounded fragment transfer; historical planning-cost comments are not current
measurements. Opaque runtime scalar refusal remains outside the approved
caller-planning-detail contract. The historical untraced memory-reservation
observation remains unexplained and establishes no new diagnosed defect here.

**Task acceptance passes.** Integrated change review and its final capability
verification remain a separate step. This task review does not complete the
active change, implement code, commit, merge, push or deploy.
