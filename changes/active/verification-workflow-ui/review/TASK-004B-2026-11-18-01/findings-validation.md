# TASK-004B independent findings validation

## Subject and method

Base `2ee8bddcab3fff94a75ba35fbcdc4a91a9b9f49f`; candidate
`7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`.
Contract: approved SPEC-verification-workflow-ui revision 6, supported by
SPEC-wyrd-ui-main-integration revision 28; original task
`tasks/TASK-004B-ui-query-consolidation.md`.

This fresh validator read the three discovery reports, approved requirements,
selected consolidation/verification decisions, applicable repository testing
and maintainer rules, actual candidate diff, changed producers, consumers and
assertions. CodeGraph preceded source tracing. The installed SvelteKit 2.70.3
source was checked against its pinned version to establish the actual framework
boundaries; it is evidence, not product authority. HEAD remained the candidate.
Unrelated dirty skill/design files were not changed or used as candidate code.

Reviewed reports: `task-review-behavior.md`, `task-review-invariants.md`,
`task-review-domain.md`. Root's fresh execution is in `verification.md`:
67 focused UI tests pass and package/host typechecks each have zero errors or
warnings. The validator performed no additional runtime, Cargo, integration,
benchmark, or implementation work. Implementation-record journey results were
checked against their actual assertions and the affected final change paths.

## Proposal decisions

| Discovery ID | Decision | Retained ID | Reason |
|---|---|---|---|
| BEH-1 | CONFIRMED | FIND-TASK-004B-1 | Aggregate FILTER creates zero-count buckets absent from the old filtered WHERE population; unchanged StatStrip includes them in its visible sparkline. The equivalence assertion removes precisely those differences. |
| INV-1 | REVISED | FIND-TASK-004B-2 | Confirmed missing list-only invalidation. Classify MISSING rather than implying every duplicate was newly introduced: the task explicitly requires removing this existing unnecessary work. |
| INV-2 | CONFIRMED | FIND-TASK-004B-3 | Actual invalidate completion is the initial streamed data chunk, whereas the scheduler treats it as full refresh completion. |
| INV-3 | CONFIRMED | FIND-TASK-004B-4 | The application forwards a signal that ordinary client navigation does not abort. Neither framework navigation nor the application's changed owners connects that cancellation lifecycle. |
| BEH-2 | CONFIRMED | FIND-TASK-004B-5 | Required populated facet-cutoff and gateway token/currency cases never exercise the consolidated SQL at the real dependency boundary. |
| D-01 | CONFIRMED | FIND-TASK-004B-6 | REQ-018/AC-012 and its explicitly selected verification decisions require failure/authority proof not present in the EXPLAIN journeys. This is missing proof, not an established authorization/resource defect. |

No rejected proposals create follow-on work. No new blocker originated in
validation, so no additional focused discovery/validation pass is necessary.

## Retained findings

### FIND-TASK-004B-1 — INCORRECT: shared stat reads change sparse count trends

**Authority:** REQ-014 preserves individual component/visual state; the task's
preservation outcome and scenario 2 retain existing populations and values.
REQ-018 selects `design/query-verification.md`, whose section 3 requires complete
typed-result and final component-prop equivalence, not equality after discarding
changed values. This is a concrete supported-use regression under that contract.

**Location:** UI `packages/ui/src/lib/observe.ts:239–250,267–271`;
`components/StatStrip.svelte:37–39,48–50`; diagnostic assertion
`src/lib/ui-library.integration.test.ts:576–578` (all UI paths are under
`crates/wyrd/wyrd-server/wyrd-ui/`).

**Starting conditions and trigger:** load the normal Logs stats
`['logs', 'log_errors']` over bucket A with an ERROR and bucket B with only INFO.
The same issue applies to combined filtered span count stats.

**Expected/actual:** the original error-stat WHERE creates the series
`[[A,1]]`; shared grouping creates both buckets and `count(*) FILTER` returns
`[[A,1],[B,0]]`. `loadStatStrip` projects both unchanged. StatStrip excludes only
null values, so the old one-point trend has no line while the new two-point
trend draws a falling line. This alters the displayed result during an ordinary
investigation. SQL's FILTER/COUNT semantics and the unchanged consumer establish
the defect without assuming an engine failure. The audited original query has
the filter before grouping. The new comparison filters zero/null buckets out
before comparing, masking this output difference.

**Smallest testable correction:** preserve each filtered count stat's original
bucket membership in the existing shared stat query/projection owner, retaining
the consolidated reads and actual whole-range totals. Do not blanket-remove
recorded zeros from other stat types. Extend the existing real-server UI test
with separated matching/nonmatching buckets; compare the complete per-stat
series with the original SQL and the component's resulting trend points.

### FIND-TASK-004B-2 — MISSING: list-only navigation repeats support reads

**Authority:** the agreed consolidation design
`design/ui-query-consolidation.md:147–151` and task scenario 5 explicitly
partition cursor/sort/shown-column invalidation to the list. REQ-018 assigns
all-route consolidation and selects its verification decisions; section 4 of
`query-verification.md` requires narrow invalidation. REQ-003/014 retain the
existing authorized composition. This finding does not invent a new global
query budget.

**Location:** Observe `src/routes/t/[tenantKey]/observe/+layout.server.ts:38–61`
and Verify `src/routes/t/[tenantKey]/verify/+page.server.ts:51–77`;
`src/lib/observe-host.ts:89–115,156–158`.

**Starting conditions and trigger:** change list sorting, shown columns, or
paging on an already populated dashboard without changing its population.

**Expected/actual:** only the list's consumers should reread. Both server loads
track these URL inputs and unconditionally start their entire support bundle.
Observe repeats its facet/overview reads; Verify repeats discovery and bounded
Card enumeration. SvelteKit `client.js:1087–1113` reruns nodes whose tracked
search parameters change. Context WeakMaps die with each invocation and cannot
avoid this work. Existing tests check changed labels/results rather than absence
of support requests. Opening Observe details is separately excluded by
`listParams`, which does not solve these list-only parameters.

**Smallest testable correction:** partition the existing BFF composition by
list versus population dependencies while reusing the portable loaders and
host credentials. Retain full supporting refresh for range/filter, explicit
refresh/Live and authorization changes; introduce no cross-request result cache.
Use focused request assertions for Observe and Verify list-only navigation,
plus production-BFF navigation proving population changes still reload support.

### FIND-TASK-004B-3 — INCORRECT: Live can overlap deferred supporting work

**Authority:** task scenario 5 and agreed design
`ui-query-consolidation.md:150` require coalescing while a refresh remains in
flight; selected `query-verification.md` section 4 requires deterministic
coalesced-refresh proof. Primary streaming must remain available under REQ-014.

**Location:** Observe `+layout.svelte:79–87` and Verify `+page.svelte:71–79`
under the corresponding routes above.

**Starting conditions and trigger:** Live is on, the list finishes promptly,
and one aggregate/Card-name support operation lasts beyond the next 10-second
tick.

**Expected/actual:** the next automatic refresh waits for complete previous
refresh settlement. Both schedulers await only `invalidate(...).finally(...)`.
In pinned Kit's `client.js:417–499,3110–3218`, `load_data` resolves when the
initial data node arrives; later chunks resolve nested deferred promises.
`_invalidate` waits for load_route and remote-query resources, not these nested
support promises. Its completion therefore schedules another full support
bundle while the preceding one is still pending. Streamed's stale-result guard
does not stop the work. `ui-pages.test.ts:992` instead mocks invalidate as pending
for the whole imagined refresh, so its green result cannot exercise this seam.

**Smallest testable correction:** retain the page-owned scheduler but gate the
next tick on actual support settlement as well as initial invalidation. Keep
list-first rendering. Hold support across multiple tick opportunities while
allowing initial invalidation to finish, verify only one automatic refresh is
active, then verify the next tick occurs after release. Exercise both delivered
dashboard compositions through their real streamed boundary.

### FIND-TASK-004B-4 — INCORRECT: navigation does not abort streamed support

**Authority:** task scenario 5, agreed design
`ui-query-consolidation.md:150–151`, and selected `query-verification.md`
section 4 require cancellation of superseded reads in addition to protection
from stale presentation. REQ-014's selection protection alone would not justify
inventing cancellation, but the selected consolidation agreement explicitly
requires both.

**Location:** Observe `+layout.server.ts:42`, Verify `+page.server.ts:57,68`;
`src/lib/ui-pages.test.ts:937–963`.

**Starting conditions and trigger:** a client refresh returns its list while
support remains pending, then the reader changes the view or navigates away.

**Expected/actual:** the obsolete reads stop through the host's actual upstream
cancellation path. The candidate only passes `request.signal` to its executor.
Pinned Kit's `client.js:3122–3123` starts the data fetch with `{}` and no abort
signal; its independent deferred-chunk reader has no navigation-token abort.
The changed application owners add no cancellation connection. Adapter-node's
`src/exports/node/index.js:138–147` only aborts its Request before the incoming
request's end; it does not convert a later response disconnect into an abort.
Thus old supporting queries continue consuming resources. The specialist's
adapter self-check independently observed `requestSignalAborted:false` after a
completed GET's streamed response disconnected. The test named navigation
cancellation manually calls `leave.abort()` on a synthetic Request, proving only
forwarding of an abort that real navigation does not provide.

**Smallest testable correction:** connect supporting requests to an actual
browser/BFF cancellation lifecycle using the existing abort-capable executor
and loader paths, covering supersession and leaving the page. Preserve host-only
credentials, primary streaming and typed failures. A production-BFF browser
test must hold support, perform real navigation, and observe upstream read
cancellation; another synthetic manual-abort test is insufficient.

### FIND-TASK-004B-5 — EVIDENCE_GAP: populated analytical edge cases are unproved

**Authority:** task scenario 2 requires independent facet cutoffs and exact
token/currency meanings; REQ-014 retains component behavior and selected
`query-verification.md` section 3 requires populated exact-result proof with
independent fixture facts. Root AGENTS/TESTING require the real dependency
boundary for these user-facing analytical promises.

**Location:** UI `src/lib/ui-library.integration.test.ts:365–432,532–580` and
`src/lib/ui-signals.integration.ts:37–110`; mock assertions in
`ui-library.test.ts:202–251,1477–1518,1556–1565` do not close this boundary.

**Starting conditions and trigger:** multiple facets each exceed the retained
ten values plus overflow, including relevant null/tie/native-value cases; a
StatStrip reads populated token/non-token gateway usage and separate currencies.

**Expected/missing proof:** actual consolidated public SQL should preserve each
facet's independent cutoff and populated token and currency totals/series,
against original SQL and independent facts. The real Observe fixture contains
three spans and one log, so no cutoff is exercised. Gateway assertions explicitly
cover absent calls/tokens/cost, and the diagnostic comparison repeats that empty
gateway population. Mock-provided final facet/stat rows and hand-built EUR/USD
presentation do not execute the changed SQL. Source supports the intended
grouping, but required runtime assertions are absent. This is not a claim that
the ranking or currency SQL is wrong.

**Smallest testable correction:** extend the existing real-server fixture/test
owner with bounded populated cases for independent cutoff and gateway usage/
currencies. Assert complete loader results, original-query equality,
independent expected values and the selected request counts. Reuse current
authorized ingest/gateway and Node/MCP fixture mechanisms. No production changes
are required unless those focused tests expose a defect; no full new matrix or
performance harness is needed.

### FIND-TASK-004B-6 — EVIDENCE_GAP: EXPLAIN negative-boundary journeys are missing

**Authority:** REQ-018, AC-012, the public query terminal contract in
`architecture/bifrost-design.md:1560–1576`, and selected
`design/query-verification.md` section 1 explicitly require table/tenant
authority, governed resource/output refusal, deadline/cancellation and typed
complete-stream failure for the explained query. Existing bare SELECT proof
cannot by itself prove the new explanation adapters.

**Location:** `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1075–1261`
and scenario 0's recorded limitation; shipped MCP/Python/TS explanation journeys
cover positive execution and invalid mutation, not these missing variants.

**Starting conditions and trigger:** a caller explains a denied table or a
foreign tenant's same-named data; an explained/analyzed query is interrupted by
deadline/cancellation or refused by resource/plan-output limits.

**Expected/missing proof:** authorization holds before disclosure; governed
refusal/interruption produces the existing typed failure and cleanup, never a
complete successful partial plan. The new Oracle journey proves bound CTEs,
selected classes, remote execution, expression failure, unsupported syntax and
denied payload, but not the listed table/tenant/lifecycle/limit cases. Its
helper uses a successful fixed 30-second deadline. Actual shipped-language/MCP
assertions add no equivalent cases. The task explicitly acknowledges missing
deadline/cancellation execution. Newly introduced ExplainExec substitution and
AnalyzeExec/redaction wrapping make inherited bare SELECT assertions inadequate
for complete AC-012 proof. No implementation exploit or incorrect refusal is
established here.

**Smallest testable correction:** extend existing public Oracle/query journey
owners with their current denial/tenant and governed deadline/cancellation/limit
fixtures for the relevant explanation forms. Assert typed failures, no complete
partial plan and cleanup of owned work. Retain positive SDK/MCP projection
evidence and add focused adapter assertions only where needed for propagation.
Run the affected focused selectors with candidate-bound results; no separate
harness, engine control or production rewrite is justified by this gap alone.

## Reconciled acceptance matrix

The rows below map existing obligations and scenarios; they do not add them.
PASS applies only to the described subclaim. A FAIL for missing required proof
does not assert incorrect production behavior.

| Existing obligations / scenario | Implementation and assertions/execution | Status |
|---|---|---|
| REQ-018 / AC-012 / 0: accepted grammar, binding, planning versus execution, selected path, redaction, shipped projection | Oracle `parse_select`, `plan_logical`, `Explanation`; published explanation journey and parser/redaction assertions; MCP/TS/Python positive and invalid-SQL journeys; recorded Oracle/MCP/SDK lane results | PASS for covered behavior |
| Same / 0: table/tenant authority and governed explanation failures | Existing payload-denial/expression-failure proof only; missing cases in FIND-TASK-004B-6 | FAIL |
| REQ-001/003/014 / 1: route audit and domain duplicate removal | Task's route inventory, Card WeakMaps/request-context construction, fresh contexts for standalone loaders, tenant-origin untracking, concurrent account/Repository reads; request assertions and fresh UI checks | PASS |
| REQ-001/003/014 / 2: facet/overview preservation and selected read counts | Grouping sets/per-facet ranking and separate spans/roots whole-range sets; populated three-span child-match/scoped/foreign/denied tests and original SQL comparisons; cutoff proof remains FIND-TASK-004B-5 | FAIL for missing edge proof; basic correctness/count evidence passes |
| Same / 2: StatStrip complete result/visual preservation | Population grouping and currency separation source; current empty gateway/basic span real-server proof; sparse count regression FIND-TASK-004B-1 and populated proof gap FIND-TASK-004B-5 | FAIL |
| REQ-001/003/014, AC-001 / 3: Verify discovery SQL and populations | `executionDiscoveryQuery`/`loadExecutionDiscovery`, same EXECUTIONS relation and list filters; original seven facets/two charts comparison, unfinished/terminal and restricted populations, SQL-ranked top three/remaining buckets | PASS for analytical composition; lifecycle rows below qualify refresh/navigation |
| Supporting UI REQ-052–057/060/061 / 4: metric math and lazy keys | Unchanged exact metric/window/reset/histogram owners; `loadMetricKeys` and existing metrics BFF; browser picker-on-use assertion and existing large-integer/reset integration proof | PASS |
| REQ-002/009/014 / 5: validation, first HTML/client stream, stale result/failure, Back state | Context predicate memo, standalone validation/refusals; held support test, `Streamed` sequence guard/failure tests, tenant-address remount test and recorded production-BFF Back journeys; root's fresh focused tests pass | PASS for these subclaims |
| Same / 5: narrow invalidation, full Live coalescing, real navigation cancellation | Actual route/framework/executor paths versus mock tests; FIND-TASK-004B-2/3/4 | FAIL |
| REQ-003/005/009/014, AC-008 / 6: analytical authority, hosts and honest terminals | Existing real Node/MCP/BFF foreign/denied/restricted and retained-output/cancel ownership assertions; successful primary result survives section failure through Streamed; built/custom host reuse | PASS for covered unchanged authority paths; explanation proof remains scenario 0 failure |
| Applicable hosted capacity authority / 6 and final fix | `Sandbox::execute` retained charge before local admission/telemetry waits; RAII guard release; attributed hold; recorded Rust/TS/Python capacity 3/3, hosted unit 37/37 and integration 53/53, MCP 50/50 after fix | PASS within recorded functional proof and benchmark override |
| REQ-001/003/014, AC-001/008 / 7: bounded diagnostic work evidence | Fixed same-fixture original/new public SQL pairs, multiset comparisons, request/scan/work and normal elapsed records, no speedup claim; weak stat bucket comparison and missing populated cases qualify acceptance | PASS for bounded work measurements; full exactness remains FAIL via FIND-TASK-004B-1/5 |

## Boundary completeness, waits and limits

Primary coverage plus the selected lifecycle and domain specialists covers the
material boundaries. Separate repo/maintainer/system reviewers add no uncovered
owner here: the primary covered local shape and projection consumers; the
specialists covered shared lifecycle, security/tenant authority, query execution
and hosted resource failure paths. No new storage/migration/deployment owner
requires additional expertise. No prior closure evidence is carried forward.

The reviewed changes add or move **no PostgreSQL, Scribe, object-storage or Vault
wait call sites**. Hosted changes move a synchronous retained-memory charge
ahead of existing process-local admission/telemetry waits; test hold attribution
and sampling are process-local. EXPLAIN reuses ordinary cut/catalog planning and
ANALYZE selected data execution, explicitly sanctioned by REQ-018 and Bifrost's
public-query explanation contract; audit remains staged without waiting. No new
PostgreSQL write, audit-acknowledgement wait or Vault request is introduced.

The current caller's explicit maintainer override excludes the missing L1
benchmark as a blocker. No benchmark or hosted throughput/latency claim is made.
The small query fixture's operator/scan evidence does not qualify full hosted
performance, peak RSS, or broad latency. Oracle/TS/Python query-lane evidence
predates the final hosted-only correction; their query production paths did not
change in that correction. MCP/capacity/hosted evidence follows it. Full journeys
were not independently rerun during this review.

## Behavioral verdict and TDD

Recommended behavioral verdict: **FIX_REQUIRED**, for FIND-TASK-004B-1 through
FIND-TASK-004B-6. Requirements and correction boundaries are settled; no material
contract clarification or new product requirement is needed.

TDD compliance remains separately violated: the Verify import failure, StatStrip
signature failure and parser/redaction compilation failures are not behavioral
RED; Streamed failure and remembered-section tests were written with their
fixes. Later passing checks cannot restore test-first history. Preserve this
record; do not require retroactive RED evidence or production churn to repair
historical process compliance.
