# UI query audit: original SQL

Actual SQL returned by the pinned UI builders, with example parameters. No server queries were executed.

Source: `4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36`. This local snapshot supplies the original SQL audit for TASK-004B.

| Family | Concrete SQL | Statements |
|---|---|---:|
| traces | [traces.sql](traces.sql) | 12 |
| verification | [verification.sql](verification.sql) | 10 |
| logs | [logs.sql](logs.sql) | 6 |
| metrics | [metrics.sql](metrics.sql) | 6 |
| variants | [variants.sql](variants.sql) | 15 |
| statstrip | [statstrip.sql](statstrip.sql) | 20 |

SQL, binds, builder inputs, dependencies and source hashes: [queries.json](queries.json).

## Original page schedules

- **Traces:** five overview queries, five facets and `trace-list` concurrently; then `trace-counts` for only the displayed traces. Its example IDs/bounds are replaced from those rows, including the original one-hour slack.
- **Verification:** all ten statements in `verification.sql` concurrently. Each retains the actual execution-input/status/checkpoint/result relation.
- **Logs:** all six statements in `logs.sql` concurrently.
- **Selected metric:** the six statements in `metrics.sql` concurrently. Separate-series mode uses its plot variants, then labels only the returned plotted series.
- **Additional cases:** detail, paging, nested grouping, 24-hour windows and StatStrip are standalone query cases.

Reconcile this snapshot with the completed predecessor before equivalence
assertions. Retain original SQL as test-only reference for changed builders.
This catalog covers analytical SQL; the parent design defines all-route audit.
Resolve example identities to populated fixture rows, including scalar metrics,
histograms and queued executions. Aggregates over zero matches cannot prove
populated equivalence or work reduction. Keep valid zeros and explicit empty
cases. Follow [query verification](../query-verification.md) for engine evidence.
No query measurements have been collected by this source audit.

## Statements

| Query | Source builder | File / line |
|---|---|---|
| `trace-spans` | `traceOverviewQueries` | traces.sql:4 |
| `trace-errors` | `traceOverviewQueries` | traces.sql:12 |
| `trace-duration` | `traceOverviewQueries` | traces.sql:21 |
| `trace-totals` | `traceOverviewQueries` | traces.sql:29 |
| `trace-durations` | `traceOverviewQueries` | traces.sql:36 |
| `trace-facet-service_name` | `facetQuery` | traces.sql:42 |
| `trace-facet-name` | `facetQuery` | traces.sql:51 |
| `trace-facet-status_code` | `facetQuery` | traces.sql:60 |
| `trace-facet-http_response_status_code` | `facetQuery` | traces.sql:69 |
| `trace-facet-gen_ai_operation_name` | `facetQuery` | traces.sql:78 |
| `trace-list` | `listQuery` | traces.sql:87 |
| `trace-counts` | `countsQuery` | traces.sql:95 |
| `verify-list` | `listQuery` | verification.sql:4 |
| `verify-facet-status` | `facetQuery` | verification.sql:29 |
| `verify-facet-verdict` | `facetQuery` | verification.sql:55 |
| `verify-facet-verifier_card_uid` | `facetQuery` | verification.sql:81 |
| `verify-facet-service_name` | `facetQuery` | verification.sql:107 |
| `verify-facet-subject_name` | `facetQuery` | verification.sql:133 |
| `verify-facet-run_id` | `facetQuery` | verification.sql:159 |
| `verify-facet-group_id` | `facetQuery` | verification.sql:185 |
| `verify-chart-verdict` | `executionsBy` | verification.sql:211 |
| `verify-chart-verifier` | `executionsBy` | verification.sql:236 |
| `logs-list` | `listQuery` | logs.sql:4 |
| `logs-timeline` | `logTimelineQuery` | logs.sql:12 |
| `logs-facet-severity_text` | `facetQuery` | logs.sql:20 |
| `logs-facet-service_name` | `facetQuery` | logs.sql:29 |
| `logs-facet-event_name` | `facetQuery` | logs.sql:38 |
| `logs-facet-exception_type` | `facetQuery` | logs.sql:47 |
| `metrics-facet-service_name` | `facetQuery` | metrics.sql:4 |
| `metrics-facet-subject_card_uid` | `facetQuery` | metrics.sql:13 |
| `metrics-names` | `metricNamesQuery` | metrics.sql:22 |
| `metrics-chart` | `metricQueries` | metrics.sql:31 |
| `metrics-histogram` | `histogramQuery` | metrics.sql:78 |
| `metrics-group-keys` | `groupKeysQuery` | metrics.sql:117 |
| `metrics-apart-chart` | `metricQueries` | variants.sql:4 |
| `metrics-apart-histogram` | `histogramQuery` | variants.sql:51 |
| `metrics-labels` | `seriesLabelsQuery` | variants.sql:90 |
| `metrics-24h-chart` | `metricQueries` | variants.sql:102 |
| `metrics-nested-chart` | `metricQueries` | variants.sql:149 |
| `trace-child-match` | `listQuery` | variants.sql:196 |
| `verify-filter-status` | `listQuery` | variants.sql:207 |
| `verify-filter-verdict` | `listQuery` | variants.sql:232 |
| `trace-page-after` | `listQuery` | variants.sql:257 |
| `trace-page-before` | `listQuery` | variants.sql:265 |
| `trace-waterfall` | `waterfallQuery` | variants.sql:273 |
| `span-detail` | `detailQuery` | variants.sql:280 |
| `log-detail` | `detailQuery` | variants.sql:286 |
| `spans-list` | `listQuery` | variants.sql:292 |
| `metric-points-list` | `listQuery` | variants.sql:300 |
| `stat-traces-total` | `statQueries` | statstrip.sql:4 |
| `stat-traces-buckets` | `statQueries` | statstrip.sql:10 |
| `stat-spans-total` | `statQueries` | statstrip.sql:18 |
| `stat-spans-buckets` | `statQueries` | statstrip.sql:24 |
| `stat-errors-total` | `statQueries` | statstrip.sql:32 |
| `stat-errors-buckets` | `statQueries` | statstrip.sql:38 |
| `stat-duration_p95-total` | `statQueries` | statstrip.sql:46 |
| `stat-duration_p95-buckets` | `statQueries` | statstrip.sql:52 |
| `stat-invocations-total` | `statQueries` | statstrip.sql:60 |
| `stat-invocations-buckets` | `statQueries` | statstrip.sql:66 |
| `stat-logs-total` | `statQueries` | statstrip.sql:74 |
| `stat-logs-buckets` | `statQueries` | statstrip.sql:80 |
| `stat-log_errors-total` | `statQueries` | statstrip.sql:88 |
| `stat-log_errors-buckets` | `statQueries` | statstrip.sql:94 |
| `stat-tokens-total` | `statQueries` | statstrip.sql:102 |
| `stat-tokens-buckets` | `statQueries` | statstrip.sql:110 |
| `stat-calls-total` | `statQueries` | statstrip.sql:120 |
| `stat-calls-buckets` | `statQueries` | statstrip.sql:126 |
| `stat-cost-total` | `statQueries` | statstrip.sql:134 |
| `stat-cost-buckets` | `statQueries` | statstrip.sql:142 |
