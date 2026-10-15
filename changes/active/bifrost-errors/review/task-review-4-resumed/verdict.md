# Bifrost errors — resumed round-4 verdict

**FIX_REQUIRED — FIND-bifrost-errors-3 remains open.**

## Immutable subject and authority

Wyrd candidate `17697dc8c2740213565e5f913cde1edfb0cb8ce5`, integration
baseline `945a1548c47897dc94583169a6ba04163a73a749`; original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79`, pre-merge task candidate
`7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`. The cumulative task diff is
integration baseline to candidate, retaining the entire Bifrost change.
Reviewers also inspected the relevant pre-merge-to-candidate shared seams.
DataFusion cumulative base `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`,
candidate `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`; distributed fork
unchanged at `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Authority: [approved specification](../../spec.md), repository AGENTS and
applicable architecture/reference rules, prior R1–R3 remediation packets and
owner instructions. No original implementation task existed. The owner
explicitly directed continuing after the main merge, overriding the skill's
automatic stop for that merge. The interrupted review remains in
`../task-review-4/`; [subject.md](subject.md) records the resumed scope.

## Independent results

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | PASS |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| DataFusion domain | [domain-review-datafusion.md](domain-review-datafusion.md) | FAIL |
| Tenancy domain | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

[Claim comparison](claim-comparison.md) grouped BEH-1, INV-R4-1 and DF-R4-1
under the existing scalar-cardinality finding. No material conflict or untraced
common source required another discovery agent. Independent validation revised
the correction to preserve correlated outer-reference behavior; literal-only
proof would regress an existing valid query. Root packages that validated
recommendation rather than independently deciding findings.

## Reconciled acceptance matrix

The behavior report retains the detailed criterion-by-criterion matrix.

| Obligation | Source and verification assessment | Result |
|---|---|---|
| REQ-002 actionable planning 4xx, internal redaction | Typed logical-planning mapping and surface assertions retained; execution errors remain opaque | PASS |
| REQ-006 scalar/IN/EXISTS and correlated live results | Native support and first-class surface journeys retained, but analytical multi-row scalar bypass remains reachable | FAIL |
| Tenant authorization, negative flows and shipped surfaces | Existing owners, SDK/MCP assertions and relevant integrated seams inspected | PASS |
| R1 bounded available predicate transfer | Post dynamic filter, first-poll snapshot, planned partition ownership; literal/two-fragment/two-footer assertions | PASS |
| Compact common-query coverage, including joined CTE | Fixed published/live corpus with explicit expected rows; generated workload removed | PASS |
| R2 global scalar cardinality and native codec | Native single_value is correct when installed; false singleton bound can omit it | FAIL |
| Empty/NULL/single-row behavior and genuine singleton optimizations | Native accumulator, MAX/LIMIT, codec and focused controls retained | PASS |
| R3 FULL JOIN, GROUPING SETS and overflow repairs | Conservative bounds and checked arithmetic are sound; new SQL/peer cases cover them | PASS |
| R3 every bypass requires a sound singleton proof | Filter::is_scalar still accepts row-dependent opposing expressions | FAIL |
| Rustdoc, boundaries and focused checks | Source/docs and recorded focused checks accepted within limits below | PASS |

## Validated ledger and prior closure

**FIND-bifrost-errors-3 — CONFIRMED, correction REVISED; INCORRECT.**
Pinned DataFusion `datafusion/expr/src/logical_plan/plan.rs:2909–2969`,
`Filter::is_scalar`, treats equality of a unique key to any non-bare-Column
expression as a fixed key. An expression such as `abs(count_value)` varies
between filtered rows. Two grouped rows can therefore survive while
`LogicalPlan::max_rows` claims one. Both scalar-to-join rewrite positions trust
that bound and omit single_value, producing successful wrong results.
The correlated scalar invariant checker is a sibling consumer of the same
proof. The validation report contains exact source tracing and reproduction.

Fix the existing DataFusion proof producer using positive invocation-invariance
proof, preserving literals, bound parameters, typed outer references and proven
pure compositions. Input-dependent, volatile or uncertain forms must not grant
singleton proof. Existing runtime validation handles unknown bounds. No Wyrd
guard, new protocol, evaluator or proof framework is needed.

- FIND-bifrost-errors-1 stays CLOSED for its diagnosed selective-predicate loss.
- FIND-bifrost-errors-2 stays CLOSED: compact common-query corpus replaces the
  broad generated workload.
- FIND-bifrost-errors-3 stays OPEN only for this false-proof bypass; the native
  accumulator, global merge and FULL/grouping/overflow fixes are retained.
- FIND-bifrost-errors-4 stays CLOSED: missing Panics documentation is present.

No new finding ID is assigned. Human-directed coverage and safe proof
construction remain authority, not retroactively invented findings.

## Evidence and limits

The DataFusion reviewer executed one focused traced existing-binary SQL
reproduction: direct single_value refused two rows, but the grouped count/abs
scalar succeeded when refusal was expected; exactly one file ran and exited 1.
Trace retained the false-bound LEFT JOIN after two optimizer passes. Validation
read the fixture/log and independently established the current source defect.
The binary was not rebuilt or hash-attested for this audit.

Recorded implementation checks include fork SQL/expr/optimizer/aggregate/proto
suites and Wyrd focused units, codecs, remote journeys, corpus, formatting,
lints, codegen, dependency and tenancy checks. Those establish their covered
cases, not this previously missing case. Merged-server execution lanes were
not rerun; relevant integrated source seams were revalidated. No benchmarks,
synthetic load, builds or broad lanes were run during this resumed audit.
The earlier untraced memory-reservation observation remains unexplained;
it does not establish a separate defect. Runtime cardinality's opaque 500 is
preserved; approved planning-error mapping is a separate contract.

## Remediation

Route [bifrost-errors-R4-require-invariant-filter-values.md](bifrost-errors-R4-require-invariant-filter-values.md)
directly to `$wyrd-implement`, then re-review the complete cumulative candidate.
Review made no candidate code changes and did not merge, push or deploy.
