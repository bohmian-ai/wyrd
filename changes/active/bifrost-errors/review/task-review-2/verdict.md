# Task review round 2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`
- Base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0`
- Specification: `changes/active/bifrost-errors/spec.md`, approved revision 1.
- Original task: absent; specification and owner directions govern.
- Remediation reviewed: `review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`.
- Prior verdict and ledger: `review/task-review-1/{verdict,findings-validation}.md`.
- DataFusion: `5697cfa35192f175bbcd54f59a5fd88527c2bbf5`.
- Distributed fork: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

The complete base-to-candidate range was audited. HEAD stayed at the candidate;
only this new review directory was written. No implementation, commit, merge,
push, deployment, benchmark or synthetic load was performed.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 actionable SQL-planning errors and suggestions | Oracle's narrow logical caller-fault traversal; physical/execution failures retain safe mapping | Mapper unit and retained Rust/Python/TS/MCP negative journeys | PASS |
| Internal details and unauthorized schema stay private | Authorization before provider registration; logical vs physical diagnostic boundary | Tenancy domain source trace and retained redaction journeys | PASS |
| REQ-006 scalar, IN, EXISTS and managed run_id happy/covered edge flows | Native scalar snapshot or analytical scalar rewrite; common shared client/server path | Focused public journeys, mixed-state corpus and remote journeys | PASS for covered cases |
| REQ-006 executed scalar input cannot silently become a multi-row set | Analytical configuration bypasses scalar extraction and joins unrestricted input | New corpus covers singleton/empty/NULL but omits multi-row refusal; pinned source documents wrong results | FAIL, FIND-bifrost-errors-3 |
| INV-001 tenant isolation | Existing authenticated cut, binding, assignment, fingerprint and peer ticket owners | Independent tenancy review and recorded check:tenant-isolation | PASS |
| R1 AC1 physical scalar snapshot ready/pending/metadata/NULL/reset | DataFusion snapshot uses existing Literal/FieldMetadata; execution-local live snapshot | Recorded fork scalar tests and Wyrd focused unit | PASS |
| R1 AC2 selective remote scalar literal and bounded output on analytical config | Two optimizer passes permit native hash-join filter; build readiness precedes first probe poll | Existing remote test checks two fragments, two footer rows and both literals | PASS |
| R1 AC3 static pruning and disjoint route ownership | Planned route indices assigned to partitions before narrowing each immutable snapshot's share | Focused unit checks static pruning and union of partition shares | PASS |
| R1 AC4 retain public-surface behavior | Existing errors, shared clients and authorization retained | Recorded focused Rust/Oracle/MCP proof; previous Python/TS proof subject to later integrated verification | PASS within task proof |
| R1 AC5 remove oversized generated framework and dependency promotion | General workload/oracle/public API and modules absent; testing DataFusion stays dev-only | Actual manifests and cumulative diff; recorded dependency check | PASS |
| Owner/R1 AC6 compact common-query coverage including CTE joined from another table | Fixed small mixed published/live corpus and explicit expected rows | Recorded exact corpus test; source checks query forms and state split | PASS |
| R1 AC7 representative remote live join | Existing private-plane peer fixture and public query client | Recorded exact remote join journey | PASS |
| AC-003/R1 AC8 verification and documentation completion | Recorded owning checks; two materially changed test docs lack required panic sections | Independent standards/validation; cumulative diff-check also reports prior verdict trailing blank line | FAIL for documentation gate, FIND-bifrost-errors-4 |
| No new Wyrd evaluator, alternate planner, broad framework or benchmark | Existing session config, expression snapshots, runtime filter and fragment owners reused | Cumulative source review | PASS |

The implementation's different private mechanics are accepted where they satisfy
stronger behavior and ownership authority. In particular, assignment before
runtime pruning protects route ownership; native hash-join reuse avoids an
unnecessary nested-loop runtime-filter implementation. Neither requires a spec
revision. Unrelated Jev/CLI/traceparent text carried over into the split spec
does not expand this Bifrost review.

## Independent review results and follow-up

| Report | Discovery result | Reconciliation |
|---|---|---|
| task-review-behavior.md | PASS | Covered paths and prior closure supported; cardinality scope disposition revised by validation |
| task-review-invariants.md | PASS | Readiness, snapshot and partition ownership supported; cardinality scope revised |
| standards-review.md | FAIL | Mandatory panic documentation confirmed and batched with semantic correction |
| maintainer-review.md | PASS | Compact coverage and owner reuse supported; no material maintainer finding |
| system-review.md | PASS | Existing cancellation/deadline/release owners preserved; untraced reservation observation remains a limit |
| domain-review-datafusion.md | FAIL | Inequality optimization proposal rejected; multi-row scalar semantics confirmed |
| domain-review-tenancy.md | PASS | No tenant/disclosure finding |
| followup-review.md | RESOLVED | Distinguished existing range-optimization limitation from intrinsic scalar correctness |
| findings-validation.md | FIX_REQUIRED | Independent final ledger: FIND-3 and FIND-4 |

Fresh follow-up was required because the implementation/invariant and domain
reviews disagreed on scope of inequalities and multi-row scalars. The fresh
Ponytail validator then independently checked every proposal and correction
against producer/consumer source. Agreement was not treated as proof.

## Validated findings

- **FIND-bifrost-errors-3 — INCORRECT:** analytical scalar-to-join rewriting
  exposes unrestricted scalar input. A two-row scalar can return successful
  wrong answers instead of failing. The correction belongs in DataFusion's
  native relational scalar-input producer, reusing existing scalar extraction
  semantics and global coalescing/distribution before dependent output. Native
  relational codec support preserves the existing distributed contract.
- **FIND-bifrost-errors-4 — VIOLATION:** two materially changed test items omit
  required Panics documentation. Add the concise sections in the same semantic
  remediation; this is not a separate style round.

Full source evidence, exact locations, dispositions and correction boundaries
are in `findings-validation.md`. No rejected finding is packaged as required
remediation. No new product, public API, tenant, stage protocol, resource owner
or persistence decision is established by the bounded correction.

## Prior-finding closure and verification limits

FIND-bifrost-errors-1 is closed for the diagnosed dropped-predicate defect and
selective equality proof. FIND-bifrost-errors-2 is closed: the large framework
is replaced by owner-directed compact query coverage. Range scalar queries can
still use an existing no-key join and transfer the eligible live tail; this
review does not claim exhaustive range optimization or a newly introduced
range regression. Footer output counts prove transfer volume, not physical
storage rows scanned. The extra optimizer pass is bounded structural work;
the historical comment is not a latency measurement.

Verification relies on committed exact-command evidence, checked against test
registration and assertions; no executable reruns were performed here.
Recorded Oracle proof is 57/57, alongside the named unit/corpus/remote/Rust
query tests and checks. It cannot prove the omitted multiple-row refusal.
Later integrated change review must run the full capability gate once.

The earlier untraced 604-byte MCP reservation assertion remains unexplained.
A later green run establishes no diagnosis; no weakened assertion or
source-proven new leak was found. Generic dynamic-filter plan reset is an
existing unsupported dependency operation without an established changed
Wyrd caller. The direct scalar reset test does not prove general join reuse.

Unlike a plain clean-working-tree `git diff --check`, the cumulative
`git diff --check 526355fb8..7d6a1ff95` detects a trailing blank line in the
prior verdict. This is a non-blocking documentation note to clean up with the
batched doc correction, not another runtime finding.

## Verdict

**FIX_REQUIRED**

Remediation: `bifrost-errors-R2-enforce-analytical-scalar-cardinality.md`.
Finding IDs: `FIND-bifrost-errors-3`, `FIND-bifrost-errors-4`.
