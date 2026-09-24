---
id: TASK-011-R1
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 38
requirements: [REQ-153, REQ-154, REQ-156, INV-012, AC-034]
depends_on: [TASK-011]
parent_task: TASK-011
remediates: [FIND-TASK-011-1, FIND-TASK-011-2, FIND-TASK-011-3, FIND-TASK-011-4, FIND-TASK-011-5, FIND-TASK-011-6, FIND-TASK-011-7]
---

# Close TASK-011 statistical and proof gaps

## Authority and subject

Implement the seven validated findings in [findings-validation.md](findings-validation.md)
against [approved specification revision 38](../../spec.md) and the
[original TASK-011](../../tasks/TASK-011-conventional-psi-spc.md). The immutable
reviewed source was `338f33235f81c30dfe3a570dc26934fe7bb77048..6e3bac0370a31b19d767ac4d20830d430c3f2ff5`;
[verdict.md](verdict.md) records why it did not pass. Revision 38, committed as
`61a930e2`, resolves the one contract decision: direct PSI/SPC batches already
contain only selected observations. Review the complete cumulative candidate
against the original task after remediation.

## Diagnosis and correction

| Finding | Current behavior and consequence | Required outcome and smallest correction |
|---|---|---|
| FIND-TASK-011-1 | `vala-drift/src/feature.rs::target_complete` treats an all-null configured-feature row as unrelated. Direct PSI can score after dropping it; direct SPC can shift subgroup boundaries. The server selects the present-null series and returns inconclusive. | Apply revision 38 at the existing direct PSI/SPC scorer boundary: every row in the already selected batch participates in completeness. A missing configured column, null, or non-finite selected value yields one unscored inconclusive report. Remove nullness-based row-membership inference; callers exclude unrelated records before constructing the direct batch. Keep the server's series-based selection and Custom behavior. |
| FIND-TASK-011-2 | `SpcScorer::finish` can emit a drifting feature plus a partial/empty feature; common verdict aggregation chooses `Drift`, allowing an alert for an incomplete target. | In the existing SPC scorer, any zero-subgroup or trailing-partial feature makes the entire SPC report unscored and inconclusive. Preserve the common verdict aggregator for complete reports and Custom. |
| FIND-TASK-011-3 | The server checks completeness and scores each feature in separate query-service reads, each pinned to a different Oracle cut. An observation arriving in a manual window between reads can yield a scored incomplete population or mixed feature populations. | Obtain PSI/SPC completeness and all feature aggregates from one consistent Oracle cut through the existing authenticated, audited query-service stream. Prefer one fixed server-built statement; preserve the exact tenant, subject, series, and half-open window filters, bounded result handling, and existing result/dispatch flow. Do not add a direct Oracle, cross-query snapshot service, or public query surface. |
| FIND-TASK-011-4 | `SpcProfile` changed the served schema, but the recorded Bifrost server lane does not run `pg_openapi_contract`. | Run `mise run test:principals:integration`, record its result, and correct any actual served-contract failure without weakening the gate. |
| FIND-TASK-011-5 | TASK-011 names 23 Vala tests but records only `test(=<path>)`, so their claimed individual runs cannot be reproduced. | Record each named test's real fully qualified expression, exact `mise exec -- cargo nextest run --locked -p vala-drift --lib -E 'test(=...)'` command, and result. Run any command without demonstrable prior execution. |
| FIND-TASK-011-6 | TASK-011 still says `status: ready` although implementation was submitted for review. | Set its status to `review` in the committed evidence update. Leave the approved spec and prior review record intact. |
| FIND-TASK-011-7 | Python and TypeScript journeys prove manual binding dispatch and reject an old authored field, but AC-034 requires each SDK journey to prove scheduled failure/Operator dispatch and refusal of a stored legacy fit. | Reuse the existing `WyrdTestServer` and Rust `VerificationFixture` test-only mechanisms for due binding and fit retirement; project only the narrow test controls needed into the Python and TypeScript test servers. Assert both outcomes through each real SDK. Do not add product routes or duplicate scheduler/baseline logic. |

## Acceptance and proof

1. Direct PSI and SPC each receive an otherwise sufficient selected batch with one null-only row and produce an inconclusive report with no scored details or feature rows. Unrelated records are excluded before direct scoring; equivalent server observations agree. Existing complete-data scores and fixed subgroup order remain unchanged.
2. With one signaled complete SPC feature and another empty or trailing-partial feature, direct and server scoring produce a wholly unscored inconclusive result and no Operator dispatch. Complete features still signal under the existing NIST X-bar/S limits.
3. PSI and SPC reads cannot combine a completeness check and scores from different Oracle cuts. A focused interleaved-ingest proof covers an incomplete observation entering at the former read seam; ordinary tenant/window/query/audit journeys remain green.
4. Rust, Python, and TypeScript journeys each prove scheduled failed-result dispatch and stored-legacy refusal. Historical results remain readable; direct manual runs still dispatch nothing.
5. The served OpenAPI gate, exact individually named Vala tests, task status, format/lints, Vala and relevant Bifrost lanes, codegen, and `git diff --check` are recorded as passing. Every specifically named new test in the evidence includes and runs its exact focused repository command.

## Preserved behavior and non-goals

Keep revision 37's PSI bins and threshold policy, fixed rational SPC subgroups,
NIST X-bar/S formulas, typed chart evidence, null-to-inconclusive policy, and
new-version/refit boundary. Do not add a PSI missing bin, a new observation
format, another chart, imputation, a legacy scorer, baseline migration, or a
new public API. Keep tenant isolation, audited query service, fit resource
limits, runtime leases/permits/shutdown, result publication, and existing
Operator dispatch semantics.

## Verification

Use the smallest focused direct/scorer, server query, and SDK journey checks
that exercise the gaps above, then `mise run fmt`, `mise run lints`,
`mise run test:vala`, the relevant `test:bifrost` server and Rust/Python/TypeScript
journey lanes, `mise run test:principals:integration`, `mise run codegen:check`,
and `git diff --check` over the cumulative task range. Record exact commands
and exit results in TASK-011 and this remediation task. Repository-managed
Postgres wrappers are required for focused database tests.
