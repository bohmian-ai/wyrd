# TASK-004B — FIX_REQUIRED

The candidate reduces analytical reads, but does not yet preserve sparse stat
trends or deliver the agreed invalidation, Live and cancellation lifecycle.
Required populated analytical and EXPLAIN failure-boundary proof is incomplete.
Independent validation retained six findings; the full diagnosed records appear
once in [findings-validation.md](findings-validation.md).

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Branch: `task/wyrd-ui-main-integration/TASK-005`.
- Base: `2ee8bddcab3fff94a75ba35fbcdc4a91a9b9f49f`.
- Candidate: `7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`.
- Reviewed range: `git diff 2ee8bddca...7e2b3f6fa`, 11 commits.
- Approved specification: `SPEC-verification-workflow-ui`, revision 6;
  supporting `SPEC-wyrd-ui-main-integration`, revision 28.
- Original task: [TASK-004B](../../tasks/TASK-004B-ui-query-consolidation.md).

Authority includes applicable root/SDK/docs AGENTS, spec-driven development,
maintainer style, TESTING, the approved specs and selected consolidation/query
verification designs, Bifrost's Oracle explanation/terminal contract, Wyrd's
hosted capacity contract, and applicable security posture. Current user
instructions take precedence, including the explicit reported maintainer
override for the absent hosted L1 benchmark. No source or authority was changed.
HEAD remained the candidate; unrelated user edits were preserved.

## Independent assignments

| Reviewer | Boundary and report |
|---|---|
| `behavior_rev` | Requirements/scenarios, analytical SQL and consumers, route audit, repository rules/maintainability and unassigned behavior: [primary report](task-review-behavior.md) |
| `invariant_rev` | Streamed sections, cache/context lifetimes, invalidation, Live and real navigation cancellation: [lifecycle report](task-review-invariants.md) |
| `domain_rev` | Public EXPLAIN, tenant/security/admission/terminal boundaries, hosted charge order and capacity proof: [domain report](task-review-domain.md) |
| Fresh `ponytail_rev` | Independent source validation of every proposal and requirement/proof coverage: [validated findings](findings-validation.md) |

The calling agent orchestrated execution and packaged the validated result;
it did not substitute its own review for these independent passes. Separate
repo/maintainer/system reviewers were omitted because the primary owns local
shape, repository and consumer checks, while the two specialists cover the
material shared-state, security, execution and failure boundaries. No new
storage, migration or deployment owner requires another specialist. All selected
reports are present; validation found no new blocker requiring another pass.

## Reconciled coverage

This compact matrix summarizes the validator's source-backed matrix. A passing
subclaim does not waive a failing requirement in the same scenario.

| Existing requirement / scenario | Implementation and proof | Status |
|---|---|---|
| REQ-018, AC-012 / 0 | Oracle parser/planner/Explanation; bound CTE, plain vs ANALYZE, selected distributed execution, redaction, invalid syntax, payload denial and shipped-client assertions. Table/tenant and governed failure variants remain FIND-6. | FAIL: proof incomplete |
| REQ-001/003/014 / 1 | Route inventory, Card/version context memo, standalone contexts, tenant layout dependency removal, concurrent account/Repository reads; request assertions and fresh UI checks. | PASS for selected route duplicate fixes |
| REQ-001/003/014 / 2 | Facet ranking/grouping sets and separate span/root overview reads; real three-span baseline comparisons and whole-range assertions. Independent cutoff proof remains FIND-5. | FAIL: edge proof incomplete |
| Same / 2: StatStrip | Shared population reads/whole-range sets; actual consumer receives extra zero count buckets (FIND-1); populated gateway/currency proof absent (FIND-5). | FAIL |
| REQ-001/003/014, AC-001 / 3 | One discovery read over unchanged EXECUTIONS population; original seven facet/two chart comparisons, top-three/remaining groups, unfinished/terminal/restricted rows. | PASS for analytical composition |
| Supporting UI REQ-052–057/060/061 / 4 | Lazy key loader and existing Metrics BFF; unchanged exact integer/reset/delta/histogram math; picker-on-use browser proof. | PASS |
| REQ-002/009/014 / 5: settled subclaims | Context predicate validation/refusals, full first HTML, list-first client result, latest-section sequencing/error, tenant-address Back restoration; focused assertions and recorded BFF journeys. | PASS for these subclaims |
| Same / 5: pending work | List-only changes rerun support, Live completes at the initial chunk, and browser navigation never supplies the forwarded abort (FIND-2/3/4); actual pinned framework path contradicts mock assumptions. | FAIL |
| REQ-003/005/009/014, AC-008 / 6 | Real Node/MCP/BFF foreign/denied/restricted and terminal ownership assertions; independent primary survives support failure. EXPLAIN variants remain scenario 0. | PASS for covered authority/host behavior |
| Hosted capacity authority / final correction | Charge before local admission/telemetry wait, RAII release and attributed hold; reported SDK capacity 3/3 each, hosted 37/37 unit and 53/53 integration, MCP 50/50 after correction. | PASS within functional evidence and benchmark override |
| REQ-001/003/014, AC-001/008 / 7 | Fixed-fixture original/new multiset and scan/work comparisons; no speedup claimed. Stat comparison masks zero buckets and populated edge cases remain absent. | PASS for bounded work measurement; exactness fails via FIND-1/5 |

## Validated blockers and disposition

All IDs below mean `FIND-TASK-004B-<n>`; full requirements, locations, triggers,
evidence and minimum corrections are in the validation report.

| ID | Classification | Accepted conclusion |
|---|---|---|
| FIND-TASK-004B-1 | INCORRECT | Filtered count statistics acquire zero buckets and draw different sparse trends. |
| FIND-TASK-004B-2 | MISSING | Paging/sort/column-only navigation repeats unrelated supporting reads. |
| FIND-TASK-004B-3 | INCORRECT | Live awaits initial streamed invalidation rather than complete support settlement. |
| FIND-TASK-004B-4 | INCORRECT | Navigation does not connect to upstream cancellation of pending support. |
| FIND-TASK-004B-5 | EVIDENCE_GAP | Independent facet cutoffs and populated token/multi-currency aggregates lack real-server proof. |
| FIND-TASK-004B-6 | EVIDENCE_GAP | EXPLAIN table/tenant and deadline/cancellation/resource/output failure journeys are missing. |

INV-1's classification was revised to MISSING because this is unfinished
assigned consolidation rather than necessarily a newly introduced duplicate.
Other proposals were confirmed. No rejected or optional proposals create work.
No material approved-contract ambiguity was found. The remediation is
[TASK-004B-R1-query-lifecycle-and-proof.md](TASK-004B-R1-query-lifecycle-and-proof.md).
Implementation is not initiated by this review.

## Verification and hot-path inventory

Fresh review execution: 67/67 focused UI tests, plus package and host checks
with zero errors/warnings; see [verification.md](verification.md). Test source
and framework tracing establish why those green mocks do not prove the missing
production lifecycle. The lifecycle specialist also exercised the installed
adapter boundary: disconnecting after the initial response left its Request
signal un-aborted.

No PostgreSQL, Scribe, object-storage or Vault wait call sites are added or
moved. Sandbox moves a synchronous process-ledger charge ahead of existing
process-local admission/telemetry waits; the test hold changes only local
attribution/sampling. EXPLAIN reuses existing cut/catalog planning and ANALYZE
the selected execution's data IO, explicitly sanctioned by REQ-018 and
`architecture/bifrost-design.md`'s public-query explanation contract. No new
PostgreSQL write, audit-ack wait or Vault call is introduced; audit stays staged.

No L1 benchmark was run, under the reported explicit maintainer override.
Hosted latency/throughput remains unqualified. Small-fixture operator/scan
comparisons do not establish broader speedup or peak memory. Full journeys,
Rust/Python lints and generated-output gates were not independently rerun here.
Recorded Oracle/TS/Python query lanes precede the last hosted-only correction;
their query production paths are unchanged by it. MCP/capacity/hosted evidence
follows that correction. These limits do not replace the missing assertions.

## TDD and closure

Behavioral verdict: **FIX_REQUIRED**. Historical TDD compliance is separately
violated: Verify's import error, StatStrip's signature failure and parser/
redaction compile failures do not establish behavioral RED; Streamed failure
and remembered-section tests were written alongside their fixes. Preserve this
history without demanding retroactive RED or production churn.

This is the first task-review round. There is no prior closure or convergence
assessment to carry forward. Review neither implements, merges, commits,
pushes, nor deploys. The task has not passed its completion gate.
