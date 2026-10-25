# Claim comparison and validation handoff

All five selected discovery reports are complete. Maintainer scope returned PASS with no proposed findings; behavior, invariant, standards and system reports returned FAIL proposals. No intended final verdict is supplied to validation.

## Grouping by alleged violated invariant

- Real exporter representation and resulting repeated collection timeout: BEH-01, INV-01, SYS-001. One claimed shared producer cause; budget amplification is a consequence, not a separate optimization requirement.
- Required Bifrost performance evidence: BEH-02 overlaps INV-06 only for per-request scan/time association. Validator should separate/consolidate correction boundaries and avoid duplicate scan work.
- Collector accounting: INV-02.
- Bounded collection/overflow: INV-03.
- Complete mixed populations: INV-04.
- Numerator/resource measurement windows: INV-05 (Bifrost established; sibling inspection requested, not independently assumed defective).
- L2 offered query traffic: BEH-03.
- Gateway common timing controls: BEH-05.
- Gateway cancellation and owned descendants: SYS-002.
- Optional profiler finalization affects budget/drain: SYS-003.
- Mandatory rustdoc: REP-001, batch bounded corrections with functional work.
- Missing original qualification evidence: BEH-04. Determine appropriate gate treatment; remediation must not prescribe full original user-journey suites as new focused remediation proof.

## Follow-up selection

No additional discovery follow-up selected: reports present no material conflicting source claims or unresolved opposing authority interpretation. Independent validation must trace every proposed failing path; agreement is not proof. If validator discovers a new blocker or a material unresolved path, obtain separate focused source validation before retaining it.

## Human-directed work (not retroactive findings)

User requires: finish signed integer fix/test first, fast real-release smoke for each engine (parsed nonempty integer timing plus CPU/throughput evidence, precise missing-evidence diagnostics, mandatory before every L1/L2 run), prove smoke, diagnose valid Bifrost L1 budget costs, then separate clean-tree bench:l1:bifrost, verification, gateway sequentially with a report after each. Do not run with --allow-dirty. Commit often with configured identity, no push. Exact previous timings deleted; do not invent historical timing attribution. Thresholds/windows/security/durability cannot be weakened.

## Verification supplied independently by orchestrator

See verification.md: pre-existing repair export regression 1/1 PASS, nine shared experiment tests PASS, exact V10/V11/V12 tests each PASS, lints PASS. fmt:check FAIL only the outside-candidate repair test formatting. No source mutation, release smoke, L1/L2 or production journey execution. Candidate unchanged; repair diff digest recorded. Baseline audit found no broken-run source reference; limits recorded in behavior report.

## Focused follow-up added during validation

Validator identified a specific coverage gap in the existing REP-001 documentation cause: materially changed Verification Bench Workload prepare/measure/finish methods may share the cited missing Errors/cancellation docs. A fresh followup-rev was selected only to independently confirm/reject these sibling locations, with report followup-review.md. This supersedes the initial no-follow-up selection for that bounded path. It does not reopen comprehensive discovery or create a separate style-only correction round.

Follow-up scope expanded only to a second validator-identified sibling gap in existing INV-05: Verification L2 may count drained completions after observe freezes the measurement CPU window. Fresh follow-up independently confirmed both Verification paths in followup-review.md (RESOLVED). These are additions to existing shared causes, not new independent findings. Gateway was not expanded because its denominator lifecycle differs.
