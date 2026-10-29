# R1 re-review: source fixes verified; formal target BLOCKED

The revised R1 implementation satisfies its bounded correction in the
inspected working tree. No new material finding remains in R1. Formal approval
of a new integrated candidate is BLOCKED because no commit contains the fix.
The prior whole-change FIX_REQUIRED result also remains through R2/R3.

## Subject and independent scope

- Root: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Original base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`.
- Prior target / unchanged HEAD: `462ebee97c81772103054f7d090053ac7054bafa`.
- New candidate commit: unavailable; source remains uncommitted.
- MCP source blob: `5d87248722ecaac30fe4a0fa2bd07df5ed306176`.
- Storage routes blob: `0ecab2483e0dcdf9cab16ed0a317938e0950af79`.
- Authority: approved storage spec revision 2, original TASK-001–003,
  revised `../change-462ebee97/TASK-CHANGE-R1-review-gates.md`, current
  maintainer direction and applicable repository/architecture rules.

One fresh reviewer independently inspected the actual two-file diff, complete
affected handlers, argument types, MCP fixture/test, config/capacity provenance
and the existing reservation owner. Its [change-review.md](change-review.md)
contains the closure matrix and source assessment. [scope.md](scope.md)
records the bounded round; unchanged prior acceptance and findings carry
forward rather than restarting the full audit. No new runtime, public-contract
or sensitive-boundary change warranted further specialists or discovery.

## Closure and remaining findings

| Finding / requirement | Reconciled evidence | Disposition |
|---|---|---|
| FIND-CHANGE-001 | All three handlers carry operation spans; app state/caller and applicable headers/body scrubbed; safe ID/part-number arguments inspected; executable bodies unchanged | R1 source correction verified |
| FIND-CHANGE-003, MCP subset | First DOCUMENT constant has meaningful rustdoc; literal/consumers unchanged | R1 source correction verified; benchmark subset remains R2 |
| FIND-CHANGE-004, MCP subset | Ordinary setup/flush, existing reservation share held across both MCP calls, stable code/retry assertions retained, guard released before flush/zero-input readback | R1 source correction verified; SDK subset remains R3 |
| Revised scope / non-goals | No one-record startup, new harness, control, dependency, production admission change, sleep or load generator | PASS for inspected source |
| Applicable focused proof | Fresh exact MCP selector passed; format and whitespace passed; implementation lints PASS recorded | Credible bounded evidence |
| Immutable target | Current HEAD does not contain these source changes | BLOCKED for formal new-target approval |
| Remaining whole-change ledger | R2 and R3 are unchanged by this diff | Original FIX_REQUIRED persists |

The reservation's `expect` expresses this isolated fixture's settled-share
invariant. Current source establishes no conflicting reserved producer in that
setup. If the invariant fails, the test fails loudly; it cannot falsely pass
the refusal assertions. No speculative waiting, retry, new control or broader
queue work is justified by a conceivable future producer.

There is no new R1 remediation task. Retain the original R2/R3 packets and
stable IDs; do not close their shared finding subsets merely because MCP passed.

## Verification and limit

[verification.md](verification.md) records exact commands and stable blob
identities. Fresh focused MCP journey: one selected test passed in 2.667 seconds,
33 excluded, with both repository Postgres migration checks passing.
`mise run fmt:check` and `git diff --check` passed. Lints PASS is recorded
implementation evidence, not a newly executed review check. No full aggregate
or benchmark was repeated for these bounded changes.

The invoked change-review skill requires “unambiguous base and target commits.”
Stable observed blobs identify the inspected source but cannot supply the
missing candidate commit. A later committed candidate with exact source
correspondence can carry this closure evidence forward. The reviewer did not
stage, commit, alter source/tests, update branches/PRs, merge, push, deploy or
complete the active packet. Only this new review directory was written.
