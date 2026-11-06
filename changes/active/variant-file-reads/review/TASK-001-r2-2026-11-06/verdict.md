# TASK-001 remediation acceptance verdict

**FIX_REQUIRED — FIND-TASK-001-1 remains partially open.** Findings 2–5 are
closed. One bounded workspace correction remains; no specification change is
required. Review does not implement, merge, push or deploy.

## Immutable subject and acceptance context

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/variant-file-reads`.
- Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
- Prior candidate: `9e8f1d9d0d38fa4f33bbcdffc9d656fa08bb3994`.
- Current candidate: `96fea41e147c799354f797469260391d2fc237ff`.
- Reviewed delta: `git diff <prior-candidate> <current-candidate>`, five
  descendant commits without rebase; cumulative original-base range is context.
- [Approved spec](../../spec.md), revision 2; [original task](../../tasks/TASK-001-memory-variant-execution.md).
- [Prior verdict](../TASK-001-r1-2026-11-06/verdict.md),
  [prior validation](../TASK-001-r1-2026-11-06/findings-validation.md),
  [implemented R1 and evidence](../TASK-001-r1-2026-11-06/TASK-001-R1-memory-execution-corrections.md).
- [Scope](scope.md) records authority/evidence hashes, navigation, carry-forward
  and specialist applicability before discovery. Source, candidate and reviewed
  authority/evidence remain unchanged. Pre-existing benchmark-scaling deletions
  and untracked packet status are outside the immutable implementation range.

This is the first remediation review. There are not two unsuccessful remediation
reviews, so the convergence-assessment trigger does not apply. The sole remaining
issue is incomplete closure of an original finding, not a new expectation.

## Coverage and independent reports

| Role | Scope and report | Result |
|---|---|---|
| Fresh invariant-rev | [Resource/lifecycle review](task-review-invariants.md): actual core pool/tracker, query/output/workspace ownership, original generation charges, refusal and source/stream lifetime | FAIL: residual FIND-TASK-001-1; core linkage/retained-output portions pass. |
| Fresh followup-rev | [Scalar reachability follow-up](followup-review.md): configured server planning versus prior public-regression claim, internal snapshot correction and P2/P4 meaning | RESOLVED: owner fix correct; prior public regression unproven. |
| Fresh ponytail-rev | [Independent validation](findings-validation.md): all five closure decisions, actual source/callers, mandatory rules, report coverage and correction burden | FIX_REQUIRED: one retained stable ID; other four closed. |

The validator independently confirmed applicability and substantive coverage.
Prior behavior/maintainer/security/system/standards reports supply source-backed
coverage for unaffected contracts; the validator checked every changed surface
and the affected consumers rather than relying on unchanged paths alone.

New shared reservation ownership required fresh invariant review. One material
conflict in scalar reachability required the focused follow-up. No broad team
restart is warranted: the security guard remains within the audited shared
validator; no new authentication, deployment, recovery, lock, persistence, SDK,
generated or dependency contract appears. LiveReadOwners holds existing charges
and one existing reservation without creating another resource authority. These
omissions were checked by the independent validator. No missing selected report,
new validator-discovered blocker or unresolved conflict requires another review.

## Prior finding closure

| Stable ID | Disposition | Source/proof conclusion |
|---|---|---|
| FIND-TASK-001-1 | PARTIALLY CLOSED / REVISED | Actual follower session uses the existing core-backed pool; retained output increments, sharing exclusion, refusal cleanup and source/stream lifetime pass. Adapted partial-filter workspace can still allocate and disappear without a positive reservation. |
| FIND-TASK-001-2 | CLOSED; prior diagnosis narrowed | Outputs snapshot recursively per execution before the no-predicate return; P2 proves pending refusal and changing literals. The actual server planner rewrites the cited public scalar queries to joins, so P4 is public correctness evidence, not a fix-dependent reproduction. |
| FIND-TASK-001-3 | CLOSED | Shared signed_requirements now checks every nested column ordinal/name and propagates Boolean filter typing before tail access; recording-tail tests cover invalid nested/filter cases. |
| FIND-TASK-001-4 | CLOSED | Unneeded parser/builder/payload fields/output type are crate-private; existing test-support exposes only real decoded-filter observation. Necessary nominal resolver plumbing remains public. |
| FIND-TASK-001-5 | CLOSED | Cited panic/alignment invariants and changed leaf schema/statistics docs are corrected; cited test indexing/expect are removed or documented. |

The prior public scalar-output regression rationale is withdrawn. The earlier
review traced a default physical planner without establishing the actual server's
configuration: installed datafusion-distributed disables physical uncorrelated
scalar subqueries and enables their logical rewrite to joins. The internal
snapshot invariant and bounded five-line correction are coherent, but neither
P4's GREEN result nor default-session RED proves the formerly claimed public
failure. No manufactured RED, extra SQL feature or deletion/refactor is required.

## Reconciled acceptance matrix

The [independent matrix](findings-validation.md#reconciled-acceptance-matrix)
records detailed source, test and carry-forward evidence for each obligation.

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Baseline cause and reuse of existing source/adapter owners | Prior behavior/maintainer proof plus unchanged whole-workflow ownership | PASS |
| Ordered local/remote requirements, aliases/schema, exact-versus-pruning | Prior V1/V3/V4 and current snapshot/closure-validation source, P2/P3/P4/P5 | PASS |
| Learned scalar/nested/list/null/IN, residual/full-root/fallback, zero-column rows | Prior validated evaluator/source proof and V2/V3, current P6; result semantics unchanged | PASS |
| Single core Bifrost accounting source; retained outputs and original charges | Actual production follower pool/root trace, P1 exact growth/drop/sharing/refusal, LiveReadOwners through source rewrites/streams | PASS for these portions |
| All included current-batch workspace governed/releasable | Successful adapted partial filter bypasses the only workspace callback; zero output contributes no final charge | FAIL — FIND-TASK-001-1 |
| Authenticated closure, tenant/table/version/fences, permissions/deadlines | Prior security/transport proof plus new recursive closure/type guard and P3 | PASS |
| Generation/staged lease, original source lifetime and cancellation | Prior invariant/system evidence plus current owners/drop path, P1/P6 | PASS for retained/source ownership |
| Fixed cohort, exact rows and conservative statistics | Prior source/statistics acceptance; no corresponding semantic change | PASS |
| Unknown/zero and memory/compressed-IO meanings | Prior V6/producer evidence; unchanged measurements | PASS |
| Staged/published preservation and excluded writer/persistence/SDK behavior | Prior reader/cache/lease coverage plus actual delta; full file proof remains TASK-002 | PASS for TASK-001 scope |
| Private payload/test-support, changed-item documentation and assertion diagnosis | Current visibility/caller/docs source, supplied original diagnosis, P5, reported clippy/lints | PASS |
| Performance/whole-change scope separation | L1 and full qualification remain TASK-003/change review; no gain claim from owner tests | PASS |

## Current independently validated finding

**FIND-TASK-001-1 — REVISED / VIOLATION: adapted filtering workspace bypasses
the core budget.** Discovery is current invariant review of residual prior INV-001;
the fresh validator independently confirmed the path, materiality and correction.
There are no new finding IDs.

At `scribe/memtable.rs::ReadableSource::project:2335–2358,2394–2399` and
`AdaptedRequirements::evaluate:2640–2665,2710–2716`, a supported partial filter
such as `value > 0` can adapt successfully, copy the selected physical batch and
skip logical fallback. Only fallback rows use the workspace reservation. For
filter-only COUNT, outputs are empty, so the copied input disappears before the
final `query.try_grow(0)`. Neither the query view nor core tracker sees the held
copy; the read can succeed when remaining capacity cannot cover that workspace.
P1 has no filter and does not exercise this branch. One-batch retention is not
the approved budget enforcement.

Complete the existing workspace reservation at that shared evaluation owner,
using the same core-backed pool and alias-aware sizing. Cover the materialized
physical/filter batch for its lifetime, refuse uncovered growth, release temporary
ownership and reconcile any buffers retained by outputs without double charging.
Keep source charges and all four closed fixes unchanged. This is a bounded local
completion, not a new tracker, allocator hook, source workflow or budget project.

The required proof extends the existing core-backed P1 with an adapted partial
filter and empty outputs: insufficient remaining capacity refuses without residue;
a fitting case preserves exact cardinality and demonstrates positive core/pool
accounting while workspace is held, followed by release. A final zero alone is
insufficient. [The ledger](findings-validation.md#final-deduplicated-current-ledger)
records complete source/correction-burden evidence.

## Verification limits and handoff

Available implementation evidence reports P1–P3 RED/GREEN, P4–P6 GREEN, 75/75
owner tests, 58/58 shard tests and format/lint/dependency/tenant/diff checks.
Reviewers inspected connected source and assertions, without rerunning builds,
tests, faults or benchmarks. These GREEN results are credible for the exercised
paths but omit the retained filtering-workspace branch. Forced adapter fallback
and some unaffected failure properties retain prior static-only limitations.

Route [TASK-001-R2](TASK-001-R2-filtering-workspace-accounting.md) directly to
`wyrd-implement`. Subsequent review checks this stable finding and affected paths,
carrying forward the four closed findings and accepted unaffected properties.
This review does not implement or start another design/approval workflow.
