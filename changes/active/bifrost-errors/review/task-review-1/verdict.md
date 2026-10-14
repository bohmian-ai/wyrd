# Task review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Approved specification: `changes/active/bifrost-errors/spec.md`, revision 1
- Original task: no task artifact exists. The specification and the owner's
  direction to require correct error mapping plus principled,
  performance-safe subquery support are the task authority.
- DataFusion fork: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`
- `datafusion-distributed` fork:
  `8fac2a978bdcf8e06457238ebfe9c7f35023c123`

The candidate remained at the stated commit throughout review. Review
artifacts are outside the immutable candidate.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 caller SQL faults expose safe planner detail as 4xx across surfaces | `oracle/mod.rs` maps only logical-planning caller faults through the stable `BifrostError` catalog; catalog resolution retains the safe typed table-not-found path | Focused mapper test passed; focused Rust, Python, TypeScript, and MCP journeys passed in the Bifrost lanes | PASS |
| REQ-002 internal and unauthorized failures remain redacted | The mapper traverses only logical-planning wrappers; authorization completes before provider registration and planning | Tenant-isolation check and cross-surface unauthorized-table journeys passed | PASS |
| REQ-006 scalar, `IN`, and `EXISTS` subqueries return correct live-table rows | DataFusion retains the authoritative leader `FilterExec`; focused journeys cover scalar, membership, existence, and managed `run_id` cases | Rust, Python, TypeScript, and MCP row-equivalence journeys passed | PASS for rows |
| Owner requirement: subquery support is principled and adds no avoidable query-path regression | `LiveScribeExec` removes every scalar-dependent predicate before fragment serialization, so selective live queries transfer the complete surviving outer tail to Oracle | No candidate test proves materialized scalar pushdown or bounded remote work | FAIL (`FIND-bifrost-errors-1`) |
| INV-001 tenant isolation | Nested references are resolved and authorized before session provider registration; fragments retain tenant-bound assignments | `check:tenant-isolation` and domain tenancy review passed | PASS |
| AC-001 user journeys on every shipped surface | Focused Rust, Python, TypeScript, and MCP journeys exist | Relevant language and agent-facing journeys passed | PASS |
| AC-002 negative flows applicable to this split change | Unauthorized-table diagnostics remain detail-free; malformed or invalid SQL uses stable errors | Focused negative journeys passed | PASS |
| AC-003 format, lints, codegen, and owning lanes | No generated contract drift; candidate builds under the owning feature sets | Format, all-feature Clippy, dependency checks, tenant isolation, units, integration, Python, TypeScript, MCP, and `codegen:check` passed. The first `verify:bifrost` aggregate reported one Oracle journey failure; a traced rerun passed all 56 Oracle tests, so the original failure remains unexplained | FAIL pending remediation verification |
| Bounded task scope | The candidate adds a general 927-line analytical qualification framework and promotes DataFusion into the normal `wyrd-testing` dependency cone | The broad workload does not observe scalar fragment materialization or bound remote work | FAIL (`FIND-bifrost-errors-2`) |

## Independent review results

| Review | Result | Reconciliation |
|---|---|---|
| Behavior | FAIL | Unknown-table proposal rejected; scalar handling revised into finding 1 |
| Invariants | PASS | No retained invariant finding |
| Repository standards | PASS | No retained standards finding |
| Maintainer | FAIL | Broad workload confirmed as finding 2; other proposals rejected or merged into finding 1 |
| System resilience | FAIL | Full-tail transfer revised into finding 1 |
| DataFusion domain | FAIL | Shared DataFusion snapshot/Wyrd fragment ownership confirmed; unrelated distributed stage-cut work excluded |
| Tenancy domain | PASS | No tenancy, RBAC, or disclosure finding |

The discovery reports conflicted on unknown-table behavior, error-root reuse,
and ownership of scalar-subquery handling. A fresh focused follow-up resolved
those conflicts from source. A fresh structured Ponytail validator then
confirmed the two findings below and rejected all other proposals.

## Validated finding ledger

### FIND-bifrost-errors-1 — INCORRECT

Scalar-dependent live predicates return correct rows but are removed from the
Scribe fragment. The complete live tail surviving unrelated predicates is
therefore scanned, transferred, decoded, and filtered at Oracle. DataFusion's
existing `PhysicalExpr::snapshot` contract is the correct owner for converting
a populated `ScalarSubqueryExpr` into a serializable literal; Wyrd owns invoking
that recursive snapshot at its runtime Scribe-fragment and route-selection
boundary. The leader filter remains the correctness authority. This correction
does not require a Wyrd scalar evaluator or a `datafusion-distributed` change.

### FIND-bifrost-errors-2 — DRIFT

The general support-workload framework and its two broad journeys cover
unrelated analytical SQL and widen the normal dependency and test cone without
proving error mapping or bounded scalar pushdown. Remove that framework and
retain the focused cross-surface journeys plus one narrow remote pushdown proof.

## Verification limits

No capacity benchmark measured the current full-tail regression; source tracing
establishes it directly, and the remediation requires a bounded-work assertion.
The first aggregate Oracle failure did not reproduce with tracing, so no cause
is claimed. The remediation must leave the focused Oracle lane green and the
later change review must run the complete Bifrost capability gate once.

## Prior-finding closure

This is the first review round. There are no prior `FIND-*` findings to close.

## Verdict

**FIX_REQUIRED**

