# TASK-004B-R1 validated findings and closure

Subject and execution limits are in `verdict.md`. The fresh remediation
reviewer checked all original findings. A separate fresh focused reviewer
validated the new affected-path proposal and remaining lifecycle acceptance.
Their source-backed reports are `task-review-remediation.md` and
`lifecycle-validation.md`. The orchestrator did not act as a source reviewer.

## Decisions

| ID / proposal | Decision |
|---|---|
| FIND-TASK-004B-1 | CLOSED within recorded source/assertion/execution evidence. |
| FIND-TASK-004B-2 | CLOSED for list-only support invalidation. New predicate duplication is separately FIND-7. |
| FIND-TASK-004B-3 | CLOSED for required automatic refresh sequencing. |
| FIND-TASK-004B-4 | OPEN; partial implementation independently confirmed. Preserve the existing ID. |
| FIND-TASK-004B-5 | CLOSED within recorded populated analytical execution evidence. |
| FIND-TASK-004B-6 | CLOSED within recorded explained-query negative-path evidence. |
| Duplicate custom predicate validation | CONFIRMED for Observe only, retained as FIND-TASK-004B-7. Verify variant REJECTED. |
| Explicit loading/visible differences | Approved Revision 1 acceptance remains incomplete. This is human-approved scope, not a newly discovered finding ID. |

## FIND-TASK-004B-4 — INCORRECT: supporting cancellation is incomplete

**Authority:** original TASK-004B scenario 5, selected consolidation design's
superseded-read cancellation, prior validated FIND-4 and R1 Revision 1 real
upstream cancellation. The original scenario explicitly includes Card names.

**Location:** UI `packages/ui/src/lib/node.ts:56–68`,
`src/lib/server/support.ts:35–39`, and browser gate
`src/lib/ui-pages.integration.test.ts:83–104`.

**Conditions and trigger:** a caller leaves or changes population while a SQL
query awaits response headers, or while Verify's Card-name lookup is pending.

**Expected:** actual obsolete upstream work stops, with no late overwrite.
**Actual:** SQL's abort listener is installed only after awaiting stream startup;
the Card list receives no cancellation signal. A SQL deadline eventually bounds
the first case, but does not cancel it on navigation. The current proxy tests
only post-header SQL bodies and cannot establish either missing case.

**Evidence and consequence:** both independent reviewers traced the executor,
TS/native/Rust stream startup and support producer. Stale display protection
does not reclaim the obsolete query/request work. The recorded limitations
acknowledge missing behavior; they are not approved exceptions.

**Smallest correction:** complete cancellation at the existing executor/
transport owners for pending startup and supporting Card requests. Extend
existing production-BFF tests to hold each missing stage, navigate or supersede,
and observe actual cancellation/cleanup. Preserve the working post-header path,
typed errors, credentials, independent content and cancellation ownership.

## FIND-TASK-004B-7 — REGRESSION: Observe validates one predicate twice

**Authority:** original task scenario 5 and consolidation design require one
applicable predicate validation per refresh, with safe standalone loaders.

**Location:** `src/routes/t/[tenantKey]/observe/+layout.server.ts:39,48`,
`observe/support/+server.ts:24`, and portable
`packages/ui/src/lib/observe.ts:167–185,358,1375`.

**Conditions and trigger:** a client Observe population refresh has a valid,
nonblank custom `where_sql`. The list request and following support request
both load the same applicable population.

**Expected:** one applicable predicate validation serves that refresh;
standalone requests still validate and refusals issue no dependent query.
**Actual:** list and support each construct a new context. The list validates
through `loadRecordTable`; facets validate again in the support context.
`predicateFor`'s WeakMap only shares validation within one context. SSR's shared
context remains correct. Verify has no supported custom-SQL input and is
excluded from this finding.

**Evidence and consequence:** the remediation reviewer discovered the new
split-request seam and the separate focused reviewer independently confirmed
both validation calls and their distinct contexts. Ordinary client filtering
restores a duplicate analytical-control request explicitly removed by this task.
This is a supported-use regression, not speculative future context misuse.

**Smallest correction:** restore one trusted validation across Observe's private
refresh composition, retaining validation for standalone loaders/endpoints and
table/tenant authorization. Do not accept untrusted browser claims of prior
validation or add a cross-request query-result cache. Assert the combined
list/support request count with custom SQL, plus standalone/refused cases.

## Approved Revision 1 acceptance still outstanding

Loading is not explicit when old support remains under new filters/ranges, and
Observe's initial overview is absent while loading. Both source reviewers
confirmed these paths. They cannot be marked PASS merely because they predate
this remediation. Correct explicit loading or obtain an explicit maintainer
change to the acceptance expectation. The full-page dark-background difference
also remains awaiting the documented maintainer ruling; no new styling scope
is inferred from that pending decision.

The associated screenshots remain temporary evidence. Record stable artifact
locations and any actual rulings before claiming visual acceptance. No ruling,
artifact move or screenshot review was performed by this re-review.
