# Independent focused lifecycle validation

Reviewer: fresh `r1_lifecycle_check`, separate from the remediation reviewer.
This report records its returned focused source validation. No files were
changed and no builds or tests were run by the reviewer.

Subject: HEAD `79a4ef66190e572c47aa93351e7362329f7bc73f` plus the current-tree
test/evidence changes described in `verdict.md`. Authority: original scenario 5,
agreed consolidation design, prior FIND-TASK-004B-4, and R1 Revision 1.

## FIND-TASK-004B-4 — closure rejected

`packages/ui/src/lib/node.ts:63` awaits `bifrost.stream` before registering
the abort listener at line 68. TypeScript `Bifrost.stream` at
`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1401–1421` accepts a deadline but no signal
and awaits native query startup. Rust Bifrost `query.rs:213–217` waits for
the response before returning its stream. Navigation in this interval cannot
stop pending work through this executor.

The Card branch at `node.ts:56–59` does not pass cancellation to `cards.list`.
Verify support includes this operation at `src/lib/server/support.ts:35–39`.

`ui-pages.integration.test.ts:83–104` selects only SQL, flushes response heads,
then blocks bodies. It exercises neither missing stage. Original task scenario
5 includes delayed Card names and cancellation of obsolete work; the agreed
design requires superseded reads to cancel. A documented deadline fallback
does not satisfy this requirement.

## Proposed duplicate validation — confirmed for Observe, rejected for Verify

Observe's list request creates a context at
`observe/+layout.server.ts:39` and calls its list loader at line 48.
`loadRecordTable` validates nonblank `where_sql` at `observe.ts:1375`.
The subsequent support request creates another context at
`observe/support/+server.ts:24`; facets validate the same predicate at
`observe.ts:358`, with overview sharing this second context's memo.
`predicateFor` memoizes by context identity at `observe.ts:167–185`.
One ordinary client population refresh therefore performs two validations;
SSR still uses a shared context.

The original scenario 5 and consolidation design explicitly require one
applicable validation per refresh. This is a fix-induced regression, not a
new product expectation. Reject the Verify variant: `verifyInputs` at
`verify-host.ts:32–45` carries no custom SQL, which its schema tests refuse.

Smallest correction: restore one validation in Observe's private composition
while preserving standalone loader/endpoint safety and table/tenant authority.
Do not trust a browser assertion that SQL is already validated and do not add a
cross-request result cache. Test the list-plus-support refresh together.

## Approved loading acceptance — incomplete

`Streamed.follow` preserves settled values during a pending successor at
`streamed.svelte.ts:53–68` and exposes no pending flag. Both pages render
ordinary populated controls when support exists. Old counts/charts under a new
population lack an updating indication. Observe also omits the overview when
support is absent at `observe/+layout.svelte:295–327`.

R1 Revision 1 requires explicit loading and separately requires maintainer
rulings for visible differences. The evidence's annotation cannot supply that
ruling. Preserve previous content if desired, but identify its pending status;
no particular marker design is prescribed by this validation.
