# Verification provenance and limits

Immutable candidate: `39c15176955edca785cc578168c67e1efcb80ea5`; cumulative base: `5e57416d562386066c1aadda9dd01783b9cd4eb5`.

| Evidence | Provenance and result |
|---|---|
| fmt:check, lints, codegen:check, ts:napi:check, ts:test:unit, cargo nextest wyrd-queue | Caller supplied, candidate-integrated, all green |
| UI package/app check | Caller supplied, candidate-integrated, zero errors |
| UI tests and build | Caller supplied, candidate-integrated, 40/40 and green build |
| UI real-server integration file with repository Postgres | Caller supplied, candidate-integrated, 2/2 |
| `git diff --check 5e57416d5...39c151769` | Fresh orchestrator read-only check, PASS |
| R4 focused RED/GREEN | Existing remediation evidence, independently source-audited; no claim of fresh rerun |
| Geometry, chart redraw, light/dark desktop/mobile, focus/capture, human gallery acceptance | Historical evidence from original task, prior packets and references/verification.md; no fresh physical browser run |

Checks that generate files were not rerun because the caller restricts writes to the new review packet. Source inspection and any reviewer memory-only probes are distinguished in individual reports. No broad gate, fresh browser sweep, Linux release, or deployment qualification is claimed. The previous round already checked rebase preservation; unrelated PR #205/main changes and independent TASK-003 changes are not layout findings.

The worktree began with no changes. A separate concurrent packet `review/task-003-39c151769-1/` appeared during this review; it is neither owned nor modified by this review.

## Fresh reviewer-specific read-only evidence

- Repository standards independently ran `mise exec -- node crates/wyrd/wyrd-server/wyrd-ui/packages/ui/scripts/catalog.ts --check`, exit 0; it checked catalog and browser problem projections without rewriting them.
- Invariant reviewer inspected the installed Svelte compiler output in memory for the example loader expression, confirming it is compiled separately from the document getter; no output file was produced.
- Discovery otherwise relied on independently inspected source, current callers and persisted assertions, rather than claiming fresh suite execution or browser probes. See each report for its coverage and limits.
