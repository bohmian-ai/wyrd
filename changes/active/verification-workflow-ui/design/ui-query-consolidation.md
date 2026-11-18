# UI query consolidation and responsive loading

Status: finalized for task authoring from the maintainer's requested audit and
task split. Execute TASK-004B after TASK-004A finishes; no implementation or
performance qualification is claimed by these notes.

## Concrete inputs and outcome

[Original query catalog and schedules](query-audit/README.md) ·
[Exact original SQL and binds](query-audit/queries.json)

An engineer navigates the UI, changes filters or pages a list. Today
independent panel loaders issue repeated analytical queries and the host awaits
them together. Verification repeats its execution-state join ten times per
refresh. The intended outcome is fewer real scans and less CPU/allocation work,
with useful data visible before supporting sections finish and unchanged
filters, counts, ordering, recorded judgments and access boundaries.

TASK-004A supplies the completed integrated revision; it is the only new
scheduling prerequisite. Existing real-server UI fixtures produce populated
signals and verification records for this task. A running development stack
or live GitHub connection is not required. This task changes portable SQL/loaders and UI consumers. The maintainer also
authorized EXPLAIN/EXPLAIN ANALYZE through the existing public query operation;
[verification decisions](query-verification.md) define that bounded addition.

## All-page and route audit

After TASK-004A, enumerate every route file and portable loader. Record actual
reads, inputs, dependencies, shared owner, change/no-change rationale and test.
The SQL catalog below covers analytical builders, not the whole-route audit.

| Route family | Audit boundary |
|---|---|
| Root, tenant layout, login/API-key/signup/reset | Session/tenant metadata reuse within a request; preserve authorization freshness and credential transitions |
| Cards index/detail and embeds | Facts, relationships, Verifier definitions/runs, prompt messages/variables/used-by/diff; reuse identical authorized reads |
| Verify and execution detail/cancel/download | Discovery consolidation and bounded detail; preserve access checks and mutation order |
| Repositories index/detail/GitHub undeclared | Inventory, policy, activity and bounded Verifier names; remove duplicate reads |
| Observe Traces/Spans/Logs/Metrics | Selected budgets below, shared statistics and on-demand inspectors |
| Settings/account/GitHub connection | Duplicate metadata only; retain account/connect/disconnect action semantics |
| Query editor, `/ui/load`, custom/MCP hosts | Shared dispatch and metadata; preserve authored SQL exactly and credential boundaries |
| Static, client-only and gallery routes | Record actual dependencies or no-query status; no forced changes |

Reuse existing owners; no universal query planner or cross-request cache.
Reads may share only within the same authorized request, tenant, credential
context and effective arguments, preserving existing freshness requirements.

## Completed analytical query audit and selected budgets

Counts exclude authentication, optional predicate validation and the explicitly
listed Card read. They describe source requests, not measured physical scans.
Snapshot: `4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36`; reconcile any intervening
predecessor changes before characterization and keep the actual source revision.

| Page/component | Current SQL | Selected SQL / request budget |
|---|---|---|
| Traces | [12 reads](query-audit/traces.sql): five overview, five facets, list, dependent counts | Two overview aggregates + one facet aggregate + list + counts = **5** |
| Spans mode | Same overview, five span facets, list; no dependent trace counts | Two overview + one facet aggregate + list = **4** |
| Verification | [10 reads](query-audit/verification.sql): seven facets, two charts, list; one separate Verifier Card list | Combined facets/charts aggregate + list = **2**; keep the bounded Card-name read independent |
| Logs | [6 reads](query-audit/logs.sql): four facets, timeline, list | One facet aggregate + timeline + list = **3** |
| Metrics rail | [Two point-population facets + series-name query](query-audit/metrics.sql) | One facet aggregate + series names = **2** |
| Selected metric | Rail plus scalar plot, histogram plot, eager group-key sample; labels follow separate-series plots | **4 initial**, **5** with labels; group keys on opening the control; points list only while its inspector is open |
| StatStrip | [Two reads per stat](query-audit/statstrip.sql) | One bucket/total aggregate per distinct compatible source/population |
| Detail, waterfall, execution inspector | [Exact on-demand evidence](query-audit/variants.sql), domain reads | Keep demand-driven bounded reads; no per-row hydration |
| Query/authored SQL panels | Caller-provided bounded SQL | Preserve SQL and existing client-side presentation semantics |
| Cards/Repositories | Typed Card/domain reads; bounded embedded components | Reuse their read owners; optimize shared embedded loaders without adding an evidence cache |

Implementation owners: `packages/ui/src/lib/observe.ts`, `schemas.ts`,
`loaders.ts`, portable Node/MCP executors, and Observe/Verify host load/render
owners. Current source reuses one in-flight connection per executor; preserve
that reuse without a global credential-bearing cache.

## Consolidated SQL decisions

Use standard SQL `GROUPING SETS`, conditional aggregates and bounded window
ranking in the existing builders. Keep results typed and shape them into the
existing component props. A CTE or UNION of old queries is not proof of shared
work: record executed-operator diagnostics and actual timings. Fewer
requests alone do not establish an optimization.

- **Span overview:** one aggregate over matching spans groups by `(bucket)`
  and `()` for charts and whole-range counts/distincts/percentiles. A second
  aggregate covers root duration buckets/totals. Keep root and matching-span
  populations separate: a matching child must still find its nonmatching root.
  Retain the existing trace-membership subquery where filters require it.
  Never add bucket distincts or average bucket percentiles into range totals.
  Check grouping-set execution diagnostics and query timings; extra aggregate work must not
  erase the benefit of shared scanning.
- **Facets:** group once per allowlisted facet in one grouping-set query. Tag
  rows with `GROUPING(...)` so a real null differs from an inactive grouping
  column. Keep each facet's native typed value, count and order. Rank per facet
  by count descending and original value ordering; return eleven per facet.
  Traces retain `COUNT(DISTINCT trace_id)`, other signals retain their current
  row-count meaning, and log severity retains its max-number annotation.
  All active filters, including a facet's own filter, continue to apply.
- **Verification:** one aggregate over the existing `EXECUTIONS` relation has
  the seven facet sets and both bucket/verdict and bucket/Verifier sets. Keep
  the separate keyset list. Preserve status-event ordering, latest checkpoint
  selection, direct/queued branches, result identity and lower-bound slack.
  Events/results remain unbounded above because accepted work can finish later.
  Do not apply LIMIT before status/verdict filtering or replace the relation
  with latest result rows. Move top-three-Verifiers/remaining-series numerical
  aggregation from host `ranked()` into SQL; only UID/name/version formatting
  remains in the host. Bound chart output after computing the full-range ranks.
- **Metrics:** keep scalar and histogram algorithms separate. Preserve reset,
  delta/gauge semantics, exact large integers/decimals, histogram bounds and
  limits. Series names, key suggestions and separate-series labels continue
  reading `bifrost.metrics.series`; plots read points. Preserve current point
  facet count meaning. Full attribute roots remain limited to bounded key
  sampling, plotted labels and requested detail. Key discovery runs when its
  control opens, via the existing authenticated Metrics BFF owner; it must
  still become available and retain distinct loading/refusal states.
- **StatStrip:** compute buckets and total through grouping sets, combining
  compatible stats as columns. Keep root/span/log/unnested usage populations
  separate where semantics differ; currencies are never added together.

The shared aggregate's failure affects the sections derived from that query;
it does not erase an independently successful list. Do not fall back to a burst
of the old queries on failure or silently relax the request budgets. If actual
execution evidence rejects this approach, report the cause/options before redesign.

## Loading and navigation

```text
One refresh: absolute range + allowlisted filters + validated predicate
                 ↓
Main list / plot starts       Supporting aggregate(s) start
         ↓                              ↓
Primary content renders       Each section settles independently
         ↓
Bounded visible-trace counts / plotted-series labels, when needed
```

Resolve the range once. Validate a nonblank custom predicate once per applicable
page refresh through the existing validation operation before dependent queries;
standalone public loaders still validate their own input. Share normalized
results only through private composition, never a public unchecked escape hatch
or cross-request cache. Preserve the existing refusal when a Metrics predicate
names a point-only column unavailable on the series read.

Keep planning in portable loaders, transport/credentials with existing hosts,
and composition in the BFF. Use existing SvelteKit promise/await and component
loading/error states so list/plot does not wait for facets/charts/Card names or
trace counts. All automatic sections still load; measure full completion too.
Do not calculate statuses, percentiles, sorting, paging or aggregate counts in
JavaScript, or fetch full evidence merely to populate a list.

Partition invalidation by actual inputs: range/filter changes reload affected
sections, cursor/sort/shown-column changes reload the list, and opening details
does not reload the dashboard. Coalesce Live refresh while one is in flight.
Cancel superseded reads and prevent late old responses replacing newer state.
Keep URL/Back/reload state and the approved visual composition in both themes;
use existing loading states rather than redesigning the page.

## Verification

[Query verification](query-verification.md) defines result equivalence, actual
request counts, existing engine diagnostics and loading/navigation proof using
existing UI tests. No benchmark runs, `dev:full`, storage matrix or SLO
requalification belongs to this task. Product contracts remain unchanged; this
task neither waives nor repeats earlier tasks' acceptance qualification.

## Governing contracts

- Verification UI specification revision 5: REQ-001–003/014 and AC-001/008/009
  preserve discovery populations, exact navigation, authority and supported hosts.
- Original UI integration specification revision 28 retains Observe, Metrics,
  sorting/paging (revision 23), REQ-052–057/060/061/062 and paired-host proof.
- Existing backend Query/Variant behavior remains unchanged. This is a private
  SQL/loader optimization of approved UI behavior, with no new endpoint, domain field, migration or storage mechanism.
- Revision 6 adds REQ-018/AC-012: public EXPLAIN/EXPLAIN ANALYZE through the
  existing query API, implemented before consolidation verification.
