# Task review round 3

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
- Wyrd base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Wyrd candidate: `dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`.
- DataFusion original: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`.
- DataFusion candidate: `f059acf6c58b2c5fd4bc36b3358e8a56ac283476`.
- Distributed fork: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`, unchanged.
- Approved spec: `changes/active/bifrost-errors/spec.md`, revision 1.
- Original task: absent; spec and owner directions govern.
- Prior verdicts/ledgers: `review/task-review-1` and `review/task-review-2`.
- Latest remediation: `review/task-review-2/bifrost-errors-R2-enforce-analytical-scalar-cardinality.md`.

The cumulative Wyrd and pinned-fork changes were reviewed, with latest diffs
used only for navigation and prior closure. Both candidate identities remained
unchanged. This review added only this directory; no implementation, merge,
push, deployment, benchmark or synthetic load occurred.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 safe actionable caller planning faults | Narrow Oracle logical mapper; physical/runtime failures remain redacted; authorization precedes providers | Retained cross-surface negative journeys and mapper proof | PASS |
| REQ-006 live scalar/IN/EXISTS, managed run_id and covered valid query forms | Existing server owners; native scalar snapshot or scalar-to-join path | Focused Rust/Python/TS/MCP journeys and mixed-state corpus | PASS for covered valid forms |
| INV-001 tenant isolation | Authorized query cuts and bound private fragment assignments remain owners | Independent tenancy source audit and recorded boundary check | PASS |
| R1 native snapshots, first-poll readiness, exact route ownership | Shared two-pass session; existing hash-join filter; execution-local immutable snapshots | Recorded unit and literal/footer journey proofs | PASS |
| R1 compact common-query/CTE coverage and remote join | Fixed small corpus; generated framework and normal testing dependency promotion absent | Recorded corpus and representative remote journey | PASS |
| R2 native scalar cardinality when validation is inserted | single_value row-count state counts NULL/equal rows, merges globally via native Final/coalesce and registry/codec | Recorded aggregate/SQL/proto and analytical published/live negative tests | PASS for wrapped inputs |
| R2 every evaluated scalar input is validated or soundly proved singleton | New rewrite trusts max_rows; FULL JOIN and GROUPING SETS can return a false bound of one | Two traced focused reproductions expect refusal but succeed; shared producer source | FAIL, FIND-bifrost-errors-3 |
| R2 Filter/Projection share one correction owner | Both use build_join; global native merge and existing codec reused | Recorded filter/projection refusal and roundtrip cases; false-bound cases omitted | FAIL for bypass above |
| R2 Panics documentation and cumulative whitespace | Mapper/MCP docs corrected; previous verdict trailing blank removed | Standards source review; fresh cumulative diff-check clean | PASS |
| Verification and non-goals | Pins/lock/codec aligned; no flag reversal, second evaluator, new stage/resource owner or broad workload | Committed focused green checks plus fresh bounded falsification; final integrated gate remains later | FAIL for missing cardinality closure |

The native SINGLE_VALUE aggregate is accepted as a conventional implementation
of the required relational scalar dependency. A previously prescribed private
node is not an acceptance requirement. Default registry/SQL availability is
ordinary engine integration, not a new Wyrd public contract. Runtime scalar
refusal remains the existing opaque execution error; REQ-002 concerns caller
planning failures and does not require a new runtime 4xx contract.

## Independent reports and comparison

| Report | Result | Reconciliation |
|---|---|---|
| task-review-behavior.md | FAIL | Final report supersedes initial PASS; false FULL JOIN bound traced |
| task-review-invariants.md | FAIL | FULL JOIN and grouping sets consolidated at shared bound producer |
| standards-review.md | PASS | FIND-4 closed; no material new standards proposal |
| maintainer-review.md | PASS | Native aggregate/registry/rewrite reuse and compact fixtures accepted |
| system-review.md | PASS | Existing global merge, cancellation/deadline/release boundaries preserved |
| domain-review-datafusion.md | FAIL | FULL JOIN bypass reproduced with tracing; shared bound owner identified |
| domain-review-tenancy.md | PASS | Authorization and safe runtime-error boundaries retained |
| findings-validation.md | FIX_REQUIRED | Independently confirmed stable FIND-3, both sibling manifestations |

No focused follow-up was needed: final discovery reports agree on the common
source and trace its producer/sibling consumers. The unique grouping-set
proposal received independent source validation and a second focused
reproduction. Reviewer agreement was not treated as proof.

## Validated finding and prior closure

**FIND-bifrost-errors-3 remains open, CONFIRMED with correction REVISED.**
LogicalPlan::max_rows advertises a guaranteed upper bound but understates FULL
JOIN and grouping-set Aggregate outputs. Two unmatched singleton FULL inputs
produce two rows; one input row under GROUPING SETS ((x), ()) produces two
groups. Both can report one. The shared scalar rewrite therefore omits
single_value and returns successful wrong answers. Projection/alias propagation
and the existing correlated scalar validator are sibling consumers of the same
proof authority.

The minimum correction is conservative native bounds: expanding cases return
unknown unless their upper bound is established soundly, and numeric bound
arithmetic returns unknown on overflow. Existing consumers then enforce
cardinality automatically. Preserve genuine global aggregate/LIMIT singleton
proofs; add no per-query-shape Wyrd guard or second proof framework. This
implements the owner's steering that unsafe singleton proof state must not be
produced. Full evidence and dispositions are in findings-validation.md.

FIND-1 remains closed for the accepted equality transfer/snapshot scope;
FIND-2 remains closed for compact qualification coverage; FIND-4 is closed by
substantive panic documentation. No new finding ID is assigned to the grouping
manifestation or to the owner's steering. No material protocol, lifecycle,
resource, tenancy, public-contract or persistence decision is required.

## Verification limits

Committed evidence reports fork SQL/optimizer/aggregate/proto checks and Wyrd
focused units/codec/journeys/corpus/checks green. Review inspected actual test
registration and assertions. Two normal-concurrency traced SQL files were run
outside the candidates, each selecting exactly one file and exiting 1 because
the scalar query succeeded instead of refusing; their direct aggregate controls
passed. Paths and exact commands are in the domain/validation reports.
These used an existing binary with documented timestamp/source/control evidence,
not a fresh rebuild or hash attestation. Source independently establishes the
incorrect bound and missing aggregate. No broad lane was rerun here.

Cumulative git diff --check from the original base is clean. Final integrated
capability verification belongs to later change review. Footer counters prove
transfer volume, not exact physical rows scanned. Existing analytical range
optimization limits and unsupported generic dynamic-filter plan reset are not
reopened. The earlier untraced 604-byte reservation observation remains
undiagnosed; no source-backed candidate leak or weakened assertion was found.

## Verdict

**FIX_REQUIRED**

Remediation: `bifrost-errors-R3-make-scalar-cardinality-bounds-sound.md`.
Finding ID: `FIND-bifrost-errors-3`.
