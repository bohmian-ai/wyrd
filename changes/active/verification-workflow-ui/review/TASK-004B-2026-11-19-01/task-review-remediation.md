# TASK-004B-R1 independent remediation review

Reviewer: fresh `r1_ponytail_rev`. This report records that reviewer's returned
assessment; the orchestrator packages it without substituting its own source
review. No implementation, tests or builds were performed during this review.

## Subject and authority

- Prior candidate: `7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`.
- Current HEAD: `79a4ef66190e572c47aa93351e7362329f7bc73f`.
- User-requested subject: current tree, including uncommitted changes to
  `streamed.test.ts`, `ui-pages.integration.test.ts`, `ui-pages.test.ts`, and R1
  evidence. This is not an assessment of committed HEAD alone.
- Contract: SPEC-verification-workflow-ui revision 6, supporting UI revision 28,
  original TASK-004B, prior validated findings, and TASK-004B-R1 including its
  approved Revision 1 browser acceptance.

The reviewer inspected the remediation diff, affected callers/consumers,
assertions and recorded executions using CodeGraph before source tracing.
Unchanged accepted areas were carried forward narrowly from the prior review.

## Prior finding closure

| Finding | Assessment | Source/assertion evidence |
|---|---|---|
| FIND-TASK-004B-1 | Closed within supplied execution evidence | `observe.ts` projects only buckets in each stat's original matched population. `ui-library.integration.test.ts:533–610` compares complete series against original SQL and renders StatStrip sparse trends; meaningful zeros elsewhere remain. |
| FIND-TASK-004B-2 | Closed | Client list loads omit support; page effects read population-specific support independently. Population keys exclude list-only choices. Browser assertions at `ui-pages.integration.test.ts:1020–1037,2020–2052` check no support rereads and unchanged supporting displays. |
| FIND-TASK-004B-3 | Closed for automatic refresh | Page schedulers await invalidation and `readSupport()`. Separately derived `live` avoids cleaning up its own refresh. Browser helper at `ui-pages.integration.test.ts:899–959` holds support across two opportunities, tests failure/success settlement, pause and departure, with exact five-list-read assertion. |
| FIND-TASK-004B-4 | Not closed | SQL cancellation is attached after response headers; Card-name enumeration does not receive the signal. The proxy holds only SQL bodies after forwarding headers. |
| FIND-TASK-004B-5 | Closed within supplied execution evidence | Populated real OTLP fixture covers independent cutoff/null/native ranking. Gateway journey at `ui-library.integration.test.ts:620` compares metered/unmetered calls and EUR/USD complete totals/series with original SQL and independent values. |
| FIND-TASK-004B-6 | Closed within supplied execution evidence | Test-only Rust changes cover both explanation forms' denied/foreign tables and planning deadlines, analyzed cancellation/cleanup, analyzed resource refusal and MCP plan-row/analyzed-byte ceilings. |

## Remaining paths

At `packages/ui/src/lib/node.ts:63`, `bifrost.stream()` is awaited before the
abort listener at lines 65–68. Startup abandoned before headers remains active
until its deadline. The Card branch at lines 56–59 calls `Cards.list` without
cancellation, and Verify support includes it in `server/support.ts:34–39`.
The test proxy forwards SQL response heads before holding their bodies. This
proves a narrower phase, not cancellation of all required support work.

Approved explicit-loading acceptance also remains unresolved. Streamed retains
the last settled value while its successor is pending without exposing pending
state. A new filter/range can therefore display old counts/charts without
identification. Observe renders its overview only when `support?.overview`
exists (`observe/+layout.svelte:295`). The task's requested visual rulings do
not establish acceptance. Correct explicit loading or obtain the stated
maintainer ruling; pre-existing behavior is not an exemption.

The reviewer also proposed a fix-induced duplicate-predicate-validation path
after splitting list/support contexts. That proposal was sent to a separate
focused reviewer before retention; see `lifecycle-validation.md`.

## Limits and process

No scratchpad/counts.txt/appendFileSync matches remain in inspected UI sources.
Reported executions were inspected against assertions, not independently rerun.
The remediation's malformed-JSON test was written after its fix; preserve this
TDD violation in addition to the original task's separate historical violations.
No new server hot-path production code or external-wait site was found; the
prior benchmark override remains a disclosed performance limit.

Recommendation: FIX_REQUIRED. Do not restart the closed analytical/EXPLAIN work.
