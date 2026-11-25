# TASK-004B independent behavior review

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `2ee8bddcab3fff94a75ba35fbcdc4a91a9b9f49f`.
- Candidate: `7e2b3f6fab8c81fb85769eee37d0c7fd6b633146`.
- Reviewed directly with `git diff 2ee8bddca...7e2b3f6fa`; HEAD remained the candidate.
- Approved contract: `SPEC-verification-workflow-ui`, revision 6; supporting `SPEC-wyrd-ui-main-integration`, revision 28.
- Original task: `tasks/TASK-004B-ui-query-consolidation.md`.
- Authority: root and affected SDK/docs `AGENTS.md`; `changes/spec-driven-development.md`; `TESTING.md`; `agent_docs/{index,maintainer-style,agent-surfaces}.md`; the task's selected query-consolidation and query-verification designs. The parent's specialists own Oracle authorization/execution and hosted admission authority, and UI lifecycle/cache authority.

The workspace contains unrelated user edits, including the UI skill. They are not part of the candidate and were not changed by this review. Current user instructions retain precedence. The recorded maintainer exception for no L1 benchmark is an explicit review input, not an independently discovered omission.

## Navigation and coverage

CodeGraph exploration preceded source searches. The relevant producer-to-consumer map is:

| Owner | Changed producer | Consumers and assertion owners |
| --- | --- | --- |
| Portable Observe loaders | `facetsQuery`, `facetValues`, `loadFacetRail` | Observe layout and catalog `FacetRail`; `ui-library.test.ts`, `ui-library.integration.test.ts`, `ui-pages.test.ts` |
| Portable Observe loaders | `traceOverviewReads`, `wholeRows`, `loadTraceOverview` | Observe charts/totals; original SQL catalog and real-server comparison |
| Portable Observe loaders | `statQueries`, `loadStatStrip` | catalog `StatStrip`, Observe, Home/Card consumers; `StatStrip.svelte` → `Sparkline.svelte`; original SQL comparison |
| Portable Verify discovery | `executionDiscoveryQuery`, `loadExecutionDiscovery` | Verify list's unchanged execution relation and filters; conventional Verify, portable chart inputs; real Node/MCP/BFF and built-artifact tests |
| Portable metric loaders | `loadMetricKeys`, `loadMetric` | existing Metrics BFF and picker; metric math and real browser tests |
| Domain route reads | shared Card/version memo, account profile/keys, Repository join | Card panels, account page, Repository index; request-characterization tests |
| Host lifecycle | Observe/Verify server loads and `Streamed` | assigned to invariant specialist; this report does not substitute mock AbortSignal delivery for real navigation proof |
| Public query/hosted runtime | Oracle parser/explanation and Sandbox changes | assigned to domain specialist; shipped SDK/MCP tests and governed resource semantics |

I checked changed analytical implementation, callers and assertions, and the task's route inventory against the actual routes. The inventory distinguishes query editor, auth/layout, Cards, Repository pages/actions, settings, execution details, loaders and static/gallery paths. The directly evidenced duplicate removals are Card reads/version listing, tenant layout URL dependency, account reads, Repository reads and predicate validation. There is no new domain API, migration, dependency, or browser-side analytical aggregation in these UI changes. No separate maintainer/repo specialist is necessary for these local extensions: the primary review owns their repository-shape coverage, and the two material boundaries have specialists.

## Requirement/scenario assessment for reconciliation

`UNRESOLVED` below denotes this reviewer's handoff to the assigned specialist, not a claim that source is unavailable. Reported runtime results are implementation evidence, not reruns by this reviewer. The orchestrator is running focused fast UI tests separately.

| Existing obligation / scenario | Implementation and actual proof | Assessment |
| --- | --- | --- |
| REQ-018, AC-012 / 0 | Oracle EXPLAIN changes, SDK/MCP journey assertions, typed query projections; separate domain report owns execution/auth/resource validation | UNRESOLVED pending specialist |
| REQ-001/003/014 / 1 | Task route inventory; Card-page request counts for Service/Prompt/Verifier; tenant URL tracking assertion; account session-principal concurrent requests; Repository independent-start assertion | PASS for selected domain-read consolidations; lifecycle qualifications remain specialist-owned |
| REQ-001/003/014 and retained Observe behavior / 2: facets and overview | `facetsQuery` uses distinct trace IDs, typed grouping columns and per-facet window ranking; overview uses separate span/root reads and actual whole-range grouping sets. Real Node/MCP tests exercise child-only trace selection, exact identities, scoped/foreign/denied reads. Three-span same-fixture baseline comparison verifies actual SQL, null facet values, counts and percentiles; mock loader asserts distinct whole-range root percentile | FAIL for incomplete required edge proof (BEH-2); no incorrect facet/overview computation found |
| Same / 2: stats | Populations separated by source/time/root/currency; aggregate `FILTER` replaces per-stat WHERE; independent whole-range rows and currency dimensions preserved structurally. Real-server proof covers spans/errors/root/invocation, subject scope, absent tokens/cost and empty range. It does not cover populated gateway token/currency cases | FAIL (BEH-1 regression and BEH-2 proof gap) |
| REQ-001/003/014, AC-001 / 3 | One discovery grouping-set read over the same `EXECUTIONS` relation; one list plus bounded Card labels. SQL computes totals and ranking before folding remaining Verifiers. Real comparison uses original seven facet/two chart reads and compares facets/verdicts as multisets, named Verifier rows and other-series bucket totals. Existing public-host journeys cover unfinished/terminal executions, status/verdict combinations, paging and restrictions | PASS for SQL/population consolidation; refresh/navigation lifecycle remains specialist-owned |
| Supporting UI REQ-052–057/060/061 / 4 | Metric calculations unchanged; eager key sample extracted to `loadMetricKeys`; existing real integration includes integer above 2^53, resets, delta/histogram windows; browser assertion delays attribute-key availability until picker interaction | PASS for analytical behavior and key-read removal; picker lifecycle specialist-owned |
| REQ-002/009/014 / 5 | Shared context predicate-validation assertion and refusal cases are meaningful. Controlled support waits, first-HTML/client distinction, Live, remembered sections and cancellation have test code, but real delivery semantics require specialist reconciliation | UNRESOLVED pending lifecycle specialist |
| REQ-003/005/009/014, AC-008 / 6 | Real Node/MCP and BFF assertions cover foreign/denied reads, exact execution/Verifier access and terminal-state distinctions; existing built artifact uses shared loaders. Hosted test changes and failure diagnosis are domain-specialist scope | PASS for unchanged analytical authority paths; UNRESOLVED hosted/lifecycle boundary |
| REQ-001/003/014, AC-001/008 / 7 | Real original/new SQL is kept test-only, fixed fixture bounds and public executor; no writes inside each pair. Assertions retain duplicate multiplicity. Recorded scan counts fall for four bundles; no latency improvement claimed. Helper measures normal elapsed separately from ANALYZE. Oracle DEBUG profiles and peak memory are explicitly unavailable | PASS for bounded diagnostic evidence, except stat bucket equality is weakened as described in BEH-1 and required populated cases in BEH-2 |

## Proposed blocking findings

### BEH-1 — INCORRECT: filtered count statistics acquire bucket rows that change their visible trends

- **Authority:** task Outcome requires exact existing evidence, populations and visual composition; scenario 2 requires baseline-equivalent typed values and ordering; `design/query-verification.md` §3 requires comparing final component props and complete typed results. Supporting UI retains the StatStrip's recorded bucket semantics.
- **Location:** `packages/ui/src/lib/observe.ts:239–250,267–271`; downstream `packages/ui/src/lib/components/StatStrip.svelte:37–39,48–50`; weakened comparison `src/lib/ui-library.integration.test.ts:576–578`.
- **Supported starting state and trigger:** a Logs page requests `['logs', 'log_errors']` over two buckets. Bucket A has one ERROR record; bucket B has one INFO record and no ERROR. The same issue affects combined `spans/errors/invocations` strips.
- **Expected:** the filtered stat has the original SQL's bucket population: error series `[[A,1]]`. There is no recorded filtered row for B. The existing Sparkline receives one point and draws no trend line.
- **Actual:** combining with unfiltered logs drops the shared WHERE restriction, creates both bucket groups and computes `count(*) FILTER (...)` as zero in B. `wholeRows` preserves `[[A,1],[B,0]]`, and `StatStrip.trend` filters only nulls. The error trend becomes `[1,0]` and displays a falling line. This is observable output change, not merely a different internal query result.
- **Evidence:** the original `stat-log_errors-buckets` query in the audited SQL uses `WHERE severity_number >= 17` before GROUP BY. The new SQL groups the complete log population and applies FILTER only to the aggregate. The integration comparison explicitly removes zero/null buckets on both sides, so it cannot establish the promised component/result equivalence. The unchanged component consumes precisely those extra zero rows.
- **Consequence:** normal log/span mixtures draw different statistical trends after a request-count optimization, despite the preservation contract.
- **Smallest correction:** preserve each filtered count stat's original bucket membership when projecting the shared result into its props, at the existing stat loader/query owner. Keep valid zero values for stat types where zero is recorded data; do not apply blanket zero filtering to all stats. Keep shared reads and whole-range totals. Add one real-server two-bucket case and assert complete per-stat series plus rendered trend behavior (or equivalent component points).

### BEH-2 — EVIDENCE_GAP: required populated boundary cases do not reach the new SQL

- **Authority:** task scenario 2 explicitly calls for enough values for independent facet cutoffs and exact token/currency meanings through real-server host integration. `design/query-verification.md` §3 requires actual populated results, exact null/large-value/filter semantics and independent fixture facts; query-audit README explicitly says aggregates over zero matches cannot prove populated equivalence. Root `AGENTS.md` and `TESTING.md` prohibit replacing the required dependency boundary with a mock.
- **Location:** `src/lib/ui-library.integration.test.ts:365–432,532–580`; fixture `src/lib/ui-signals.integration.ts:37–110`; mock-only substitute `src/lib/ui-library.test.ts:202–251,1477–1518,1556–1565`.
- **Starting conditions/trigger:** multiple facets each exceed the retained ten plus overflow value, including tied counts/null/native values; a StatStrip reads real gateway calls with token and non-token usage and at least two currencies.
- **Expected proof:** the consolidated public query/loader returns each facet's own independently ranked cutoff and preserves populated token totals and separate currency totals/series, compared with original SQL and independent fixture facts.
- **Missing proof:** the real Observe fixture supplies three spans and one log. No facet can exercise the cutoff. Its gateway cases explicitly assert no calls/tokens/cost; the diagnostic comparison uses that same empty gateway population. The unit facet test supplies three final rows from a recording executor, and the stat mock supplies a single EUR row. Rendering a hand-built EUR/USD result proves presentation only. No assertion in these owners executes the changed ranking/aggregate SQL against the required populated edge cases.
- **Consequence:** material parts of the approved exactness promise remain unverified, even though basic three-span results and structural SQL checks pass. This finding does not assert the SQL is incorrect.
- **Smallest correction:** extend the existing real-server integration fixture/test owner with bounded populated data for these two cases; compare complete loader results and original SQL with independent expected values and selected request counts. Reuse existing Node/MCP hosts, authorized ingest/gateway fixtures and original SQL references; no production machinery is required without a demonstrated defect. Run focused affected selectors, not the full journey matrix solely for this evidence gap.

## TDD and verification limits

Behavioral acceptance does not erase recorded process history. Scenario 3's import failure, the StatStrip signature failure, and parser/redaction compilation failures are not behavioral RED. Streamed failure and remembered-section regressions were written alongside their fixes and do not establish test-first order. Preserve these as TDD violations; do not require historical RED reconstruction or production churn to repair them.

The retained task evidence reports fast UI and full UI integration passes and affected backend/SDK lanes. Oracle/Python/TypeScript backend runs precede the final hosted charge-order commit; the MCP lane and capacity/hosted lanes follow it. The domain specialist must assess affected-path carry-forward. This reviewer ran source/diff checks, including a clean `git diff --check`, and did not rerun integration, Cargo, benchmarks, browser screenshots, or generated checks. Focused UI execution is orchestrator-owned.

Recommendation to the independent validator: retain BEH-1 only if the preservation contract applies to sparse StatStrip series, and independently verify BEH-2 against any additional candidate-bound evidence outside the inspected UI owners. Neither proposal authorizes a redesign or new product behavior.
