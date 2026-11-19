-- Original UI SQL from 4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36
-- Design input; not executed against a server.

-- metrics-apart-chart | metricQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","m.requests","1 minute",60]
WITH pts AS (
SELECT date_bin($4::interval, time_unix_nano) AS w, time_unix_nano AS t, start_time_unix_nano AS s, series_id, metric_type, unit, aggregation_temporality, is_monotonic, CAST(int_value AS DECIMAL(38, 0)) AS i, COALESCE(CAST(int_value AS DOUBLE), double_value) AS x
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz AND metric_name = $3 AND metric_type IN ('gauge', 'sum')), steps AS (
SELECT *, lag(t) OVER series IS NULL AS head, lag(s) OVER series AS sp, lag(x) OVER series AS xp, lag(i) OVER series AS ip
FROM pts
WINDOW series AS (
PARTITION BY series_id
ORDER BY t)), ends AS (
SELECT w, series_id, metric_type, unit, aggregation_temporality, is_monotonic, sum(x) AS xt, last_value(x
ORDER BY t) AS x1, sum(CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE x END WHEN s = sp AND x >= xp THEN x - xp ELSE x END) AS xd, sum(i) AS it, last_value(i
ORDER BY t) AS i1, sum(CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE i END WHEN s = sp AND i >= ip THEN i - ip ELSE i END) AS id, bool_and(i IS NOT NULL AND (head OR ip IS NOT NULL)) AS exact
FROM steps
GROUP BY w, series_id, metric_type, unit, aggregation_temporality, is_monotonic), per AS (
SELECT w, encode(series_id, 'hex') AS series, metric_type, unit, aggregation_temporality, is_monotonic, CASE WHEN aggregation_temporality = 1 THEN xt WHEN metric_type = 'sum' AND is_monotonic THEN xd ELSE x1 END AS v, CASE WHEN exact THEN CASE WHEN aggregation_temporality = 1 THEN it WHEN metric_type = 'sum' AND is_monotonic THEN id ELSE i1 END END AS vi
FROM ends), grouped AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, series, CAST(CASE WHEN count(vi) = count(*) THEN CASE WHEN metric_type = 'sum' THEN sum(vi) ELSE avg(vi) END END AS DECIMAL(38, 4)) AS gi, CASE WHEN metric_type = 'sum' THEN sum(v) ELSE avg(v) END AS gv
FROM per
GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, series), valued AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, series, COALESCE(CAST(gi AS DOUBLE), gv) / CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN $5 ELSE 1 END AS value, CASE WHEN NOT (metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1)) THEN gi END AS exact
FROM grouped), measured AS (
SELECT *, CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN sum(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series) ELSE avg(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series) END AS measure
FROM valued), ranked AS (
SELECT *, dense_rank() OVER (
ORDER BY measure DESC NULLS LAST, metric_type, unit, aggregation_temporality, is_monotonic, series) AS rank
FROM measured), plotted AS (
SELECT *, COALESCE(CAST(exact AS VARCHAR), CAST(value AS VARCHAR)) AS text, COALESCE(exact, CASE WHEN abs(value) < 1e19 THEN CAST(value AS DECIMAL(38, 0)) END) AS tie
FROM (
SELECT *, max(rank) OVER () AS groups
FROM ranked)
WHERE rank <= 20)
SELECT w AS at, metric_type, unit, aggregation_temporality, is_monotonic, series, value, groups, last_value(text) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series
ORDER BY w ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS latest, first_value(text) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series
ORDER BY value DESC NULLS LAST, tie DESC NULLS LAST) AS max, CASE WHEN count(exact) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series) = count(*) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series) THEN CAST(avg(exact) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series) AS VARCHAR) ELSE CAST(avg(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series) AS VARCHAR) END AS mean
FROM plotted
ORDER BY at;

-- metrics-apart-histogram | histogramQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","m.requests","1 minute"]
WITH pts AS (
SELECT date_bin($4::interval, time_unix_nano) AS w, time_unix_nano AS t, start_time_unix_nano AS started, metric_type, unit, aggregation_temporality, is_monotonic, encode(series_id, 'hex') AS series, explicit_bounds AS bounds, series_id, CAST(histogram_count AS BIGINT) AS n, histogram_sum AS v, CAST(bucket_counts AS BIGINT[]) AS b
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz AND metric_name = $3 AND metric_type = 'histogram'), steps AS (
SELECT *, lag(t) OVER series IS NULL AS head, lag(started) OVER series AS sp, lag(n) OVER series AS np, lag(v) OVER series AS vp, lag(b) OVER series AS bp
FROM pts
WHERE aggregation_temporality = 2
WINDOW series AS (
PARTITION BY series_id
ORDER BY t)), counted AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds, unnest(generate_series(1, CAST(cardinality(b) AS BIGINT))) AS i, unnest(b) AS c, CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN b END WHEN sp = started AND n >= np THEN bp END AS bb, n - CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN n ELSE 0 END WHEN sp = started AND n >= np THEN np ELSE 0 END AS dn, v - CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN v ELSE 0 END WHEN sp = started AND n >= np THEN vp ELSE 0 END AS dv
FROM steps
UNION ALL
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds, unnest(generate_series(1, CAST(cardinality(b) AS BIGINT))) AS i, unnest(b) AS c, NULL AS bb, n AS dn, v AS dv
FROM pts
WHERE aggregation_temporality <> 2), buckets AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds, i, sum(CAST(c AS BIGINT) - COALESCE(CAST(array_element(bb, i) AS BIGINT), 0)) AS c, sum(CASE WHEN i = 1 THEN dn END) AS dn, sum(CASE WHEN i = 1 THEN dv END) AS dv, count(CASE WHEN i = 1 THEN dn END) AS kn, count(CASE WHEN i = 1 THEN dv END) AS kv
FROM counted
GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds, i), windows AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds, sum(dn) AS count, CASE WHEN sum(kv) = sum(kn) THEN sum(dv) END AS sum, array_agg(CAST(c AS VARCHAR)
ORDER BY i) AS buckets
FROM buckets
GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds), ranked AS (
SELECT *, dense_rank() OVER (
ORDER BY measure DESC, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds) AS rank
FROM (
SELECT *, sum(count) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, series, bounds) AS measure
FROM windows
WHERE count > 0))
SELECT w AS at, metric_type, unit, aggregation_temporality, is_monotonic, series, bounds, count, sum, buckets, groups
FROM (
SELECT *, max(rank) OVER () AS groups
FROM ranked)
WHERE rank <= 20
ORDER BY at;

-- metrics-labels | seriesLabelsQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","m.requests","00000000000000000000000000000001"]
-- After: metrics-apart-chart, metrics-apart-histogram; replace example IDs/bounds with real preceding results.
SELECT encode(series_id, 'hex') AS series, to_json(resource_attributes) AS resource, scope_name, scope_version, to_json(scope_attributes) AS scope, to_json(attributes) AS attributes
FROM (
SELECT *, row_number() OVER (
PARTITION BY series_id
ORDER BY hour_unix_nano DESC) AS newest
FROM bifrost.metrics.series
WHERE hour_unix_nano >= date_bin(INTERVAL '1 hour', $1::timestamptz) AND hour_unix_nano < $2::timestamptz AND metric_name = $3 AND series_id IN (decode($4, 'hex')))
WHERE newest = 1;

-- metrics-24h-chart | metricQueries
-- Parameters: ["2026-11-17T13:00:00.000Z","2026-11-18T13:00:00.000Z","m.requests","15 minutes",900]
WITH pts AS (
SELECT date_bin($4::interval, time_unix_nano) AS w, time_unix_nano AS t, start_time_unix_nano AS s, series_id, metric_type, unit, aggregation_temporality, is_monotonic, CAST(int_value AS DECIMAL(38, 0)) AS i, COALESCE(CAST(int_value AS DOUBLE), double_value) AS x
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz AND metric_name = $3 AND metric_type IN ('gauge', 'sum')), steps AS (
SELECT *, lag(t) OVER series IS NULL AS head, lag(s) OVER series AS sp, lag(x) OVER series AS xp, lag(i) OVER series AS ip
FROM pts
WINDOW series AS (
PARTITION BY series_id
ORDER BY t)), ends AS (
SELECT w, series_id, metric_type, unit, aggregation_temporality, is_monotonic, sum(x) AS xt, last_value(x
ORDER BY t) AS x1, sum(CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE x END WHEN s = sp AND x >= xp THEN x - xp ELSE x END) AS xd, sum(i) AS it, last_value(i
ORDER BY t) AS i1, sum(CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE i END WHEN s = sp AND i >= ip THEN i - ip ELSE i END) AS id, bool_and(i IS NOT NULL AND (head OR ip IS NOT NULL)) AS exact
FROM steps
GROUP BY w, series_id, metric_type, unit, aggregation_temporality, is_monotonic), per AS (
SELECT w, encode(series_id, 'hex') AS series, metric_type, unit, aggregation_temporality, is_monotonic, CASE WHEN aggregation_temporality = 1 THEN xt WHEN metric_type = 'sum' AND is_monotonic THEN xd ELSE x1 END AS v, CASE WHEN exact THEN CASE WHEN aggregation_temporality = 1 THEN it WHEN metric_type = 'sum' AND is_monotonic THEN id ELSE i1 END END AS vi
FROM ends), grouped AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, CAST(CASE WHEN count(vi) = count(*) THEN CASE WHEN metric_type = 'sum' THEN sum(vi) ELSE avg(vi) END END AS DECIMAL(38, 4)) AS gi, CASE WHEN metric_type = 'sum' THEN sum(v) ELSE avg(v) END AS gv
FROM per
GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic), valued AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, COALESCE(CAST(gi AS DOUBLE), gv) / CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN $5 ELSE 1 END AS value, CASE WHEN NOT (metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1)) THEN gi END AS exact
FROM grouped), measured AS (
SELECT *, CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN sum(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic) ELSE avg(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic) END AS measure
FROM valued), ranked AS (
SELECT *, dense_rank() OVER (
ORDER BY measure DESC NULLS LAST, metric_type, unit, aggregation_temporality, is_monotonic) AS rank
FROM measured), plotted AS (
SELECT *, COALESCE(CAST(exact AS VARCHAR), CAST(value AS VARCHAR)) AS text, COALESCE(exact, CASE WHEN abs(value) < 1e19 THEN CAST(value AS DECIMAL(38, 0)) END) AS tie
FROM (
SELECT *, max(rank) OVER () AS groups
FROM ranked)
WHERE rank <= 20)
SELECT w AS at, metric_type, unit, aggregation_temporality, is_monotonic, value, groups, last_value(text) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic
ORDER BY w ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS latest, first_value(text) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic
ORDER BY value DESC NULLS LAST, tie DESC NULLS LAST) AS max, CASE WHEN count(exact) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic) = count(*) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic) THEN CAST(avg(exact) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic) AS VARCHAR) ELSE CAST(avg(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic) AS VARCHAR) END AS mean
FROM plotted
ORDER BY at;

-- metrics-nested-chart | metricQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","m.requests","route","1 minute",60]
WITH pts AS (
SELECT date_bin($5::interval, time_unix_nano) AS w, time_unix_nano AS t, start_time_unix_nano AS s, series_id, metric_type, unit, aggregation_temporality, is_monotonic, variant_get_text(attributes, $4) AS g1, CAST(int_value AS DECIMAL(38, 0)) AS i, COALESCE(CAST(int_value AS DOUBLE), double_value) AS x
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz AND metric_name = $3 AND metric_type IN ('gauge', 'sum')), steps AS (
SELECT *, lag(t) OVER series IS NULL AS head, lag(s) OVER series AS sp, lag(x) OVER series AS xp, lag(i) OVER series AS ip
FROM pts
WINDOW series AS (
PARTITION BY series_id
ORDER BY t)), ends AS (
SELECT w, series_id, metric_type, unit, aggregation_temporality, is_monotonic, g1, sum(x) AS xt, last_value(x
ORDER BY t) AS x1, sum(CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE x END WHEN s = sp AND x >= xp THEN x - xp ELSE x END) AS xd, sum(i) AS it, last_value(i
ORDER BY t) AS i1, sum(CASE WHEN head THEN CASE WHEN s < $1::timestamptz THEN 0 ELSE i END WHEN s = sp AND i >= ip THEN i - ip ELSE i END) AS id, bool_and(i IS NOT NULL AND (head OR ip IS NOT NULL)) AS exact
FROM steps
GROUP BY w, series_id, metric_type, unit, aggregation_temporality, is_monotonic, g1), per AS (
SELECT w, encode(series_id, 'hex') AS series, metric_type, unit, aggregation_temporality, is_monotonic, g1, CASE WHEN aggregation_temporality = 1 THEN xt WHEN metric_type = 'sum' AND is_monotonic THEN xd ELSE x1 END AS v, CASE WHEN exact THEN CASE WHEN aggregation_temporality = 1 THEN it WHEN metric_type = 'sum' AND is_monotonic THEN id ELSE i1 END END AS vi
FROM ends), grouped AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, g1, CAST(CASE WHEN count(vi) = count(*) THEN CASE WHEN metric_type = 'sum' THEN sum(vi) ELSE avg(vi) END END AS DECIMAL(38, 4)) AS gi, CASE WHEN metric_type = 'sum' THEN sum(v) ELSE avg(v) END AS gv
FROM per
GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, g1), valued AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, g1, COALESCE(CAST(gi AS DOUBLE), gv) / CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN $6 ELSE 1 END AS value, CASE WHEN NOT (metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1)) THEN gi END AS exact
FROM grouped), measured AS (
SELECT *, CASE WHEN metric_type = 'sum' AND (is_monotonic OR aggregation_temporality = 1) THEN sum(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1) ELSE avg(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1) END AS measure
FROM valued), ranked AS (
SELECT *, dense_rank() OVER (
ORDER BY measure DESC NULLS LAST, metric_type, unit, aggregation_temporality, is_monotonic, g1) AS rank
FROM measured), plotted AS (
SELECT *, COALESCE(CAST(exact AS VARCHAR), CAST(value AS VARCHAR)) AS text, COALESCE(exact, CASE WHEN abs(value) < 1e19 THEN CAST(value AS DECIMAL(38, 0)) END) AS tie
FROM (
SELECT *, max(rank) OVER () AS groups
FROM ranked)
WHERE rank <= 20)
SELECT w AS at, metric_type, unit, aggregation_temporality, is_monotonic, g1, value, groups, last_value(text) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1
ORDER BY w ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING) AS latest, first_value(text) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1
ORDER BY value DESC NULLS LAST, tie DESC NULLS LAST) AS max, CASE WHEN count(exact) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1) = count(*) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1) THEN CAST(avg(exact) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1) AS VARCHAR) ELSE CAST(avg(value) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, g1) AS VARCHAR) END AS mean
FROM plotted
ORDER BY at;

-- trace-child-match | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","child-service"]
SELECT start_time_unix_nano, name, service_name, duration_nano, status_code, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL AND trace_id IN (
SELECT trace_id
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND service_name = $3)
ORDER BY start_time_unix_nano DESC, encode(trace_id, 'hex') ASC, encode(span_id, 'hex') ASC
LIMIT 51;

-- verify-filter-status | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","waiting"]
SELECT recorded_at, verifier_card_uid, subject_name, run_id, group_id, service_name, status, verdict, execution_id, subject_kind, subject_version, subject_card_uid, result_id, binding_id, workload_card_uid
FROM (
SELECT i.execution_id, i.recorded_at, i.verifier_card_uid, i.binding_id, i.group_id, i.origin, i.run_id, i.subject_kind, i.subject_name, i.subject_version, i.subject_card_uid, i.workload_card_uid, i.service_name, i.service_version, i.deployment_environment, r.result_id, r.verdict, r.ended_at, CASE WHEN i.origin = 'direct' THEN CASE WHEN r.result_id IS NOT NULL THEN 'completed' ELSE 'not recorded' END ELSE coalesce(q.status, 'pending') END AS status
FROM bifrost.verification.execution_inputs i
LEFT JOIN (
SELECT execution_id, last_value(status
ORDER BY occurred_at, event_id) AS status
FROM bifrost.verification.execution_events
WHERE event = 'status' AND occurred_at >= $1::timestamptz - INTERVAL '1 hour'
GROUP BY execution_id) q ON q.execution_id = i.execution_id
LEFT JOIN (
SELECT e.execution_id, e.value -> 'terminal' ->> 'result_id' AS result_id
FROM bifrost.verification.execution_events e JOIN (
SELECT execution_id, max(sequence) AS sequence
FROM bifrost.verification.execution_events
WHERE event = 'checkpoint' AND occurred_at >= $1::timestamptz - INTERVAL '1 hour'
GROUP BY execution_id) l ON l.execution_id = e.execution_id AND l.sequence = e.sequence
WHERE e.event = 'checkpoint' AND e.occurred_at >= $1::timestamptz - INTERVAL '1 hour') s ON s.execution_id = i.execution_id
LEFT JOIN bifrost.verification.results r ON r.execution_id = i.execution_id AND r.ended_at >= $1::timestamptz - INTERVAL '1 hour' AND (i.origin = 'direct' OR r.result_id = s.result_id)) AS executions
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz AND status = $3
ORDER BY recorded_at DESC, execution_id ASC
LIMIT 51;

-- verify-filter-verdict | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","failed"]
SELECT recorded_at, verifier_card_uid, subject_name, run_id, group_id, service_name, status, verdict, execution_id, subject_kind, subject_version, subject_card_uid, result_id, binding_id, workload_card_uid
FROM (
SELECT i.execution_id, i.recorded_at, i.verifier_card_uid, i.binding_id, i.group_id, i.origin, i.run_id, i.subject_kind, i.subject_name, i.subject_version, i.subject_card_uid, i.workload_card_uid, i.service_name, i.service_version, i.deployment_environment, r.result_id, r.verdict, r.ended_at, CASE WHEN i.origin = 'direct' THEN CASE WHEN r.result_id IS NOT NULL THEN 'completed' ELSE 'not recorded' END ELSE coalesce(q.status, 'pending') END AS status
FROM bifrost.verification.execution_inputs i
LEFT JOIN (
SELECT execution_id, last_value(status
ORDER BY occurred_at, event_id) AS status
FROM bifrost.verification.execution_events
WHERE event = 'status' AND occurred_at >= $1::timestamptz - INTERVAL '1 hour'
GROUP BY execution_id) q ON q.execution_id = i.execution_id
LEFT JOIN (
SELECT e.execution_id, e.value -> 'terminal' ->> 'result_id' AS result_id
FROM bifrost.verification.execution_events e JOIN (
SELECT execution_id, max(sequence) AS sequence
FROM bifrost.verification.execution_events
WHERE event = 'checkpoint' AND occurred_at >= $1::timestamptz - INTERVAL '1 hour'
GROUP BY execution_id) l ON l.execution_id = e.execution_id AND l.sequence = e.sequence
WHERE e.event = 'checkpoint' AND e.occurred_at >= $1::timestamptz - INTERVAL '1 hour') s ON s.execution_id = i.execution_id
LEFT JOIN bifrost.verification.results r ON r.execution_id = i.execution_id AND r.ended_at >= $1::timestamptz - INTERVAL '1 hour' AND (i.origin = 'direct' OR r.result_id = s.result_id)) AS executions
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz AND verdict = $3
ORDER BY recorded_at DESC, execution_id ASC
LIMIT 51;

-- trace-page-after | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","2026-11-18T12:30:00.000000000Z","00000000000000000000000000000001","0000000000000001"]
SELECT start_time_unix_nano, name, service_name, duration_nano, status_code, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL AND (start_time_unix_nano < $3::timestamptz OR (start_time_unix_nano = $3::timestamptz AND (encode(trace_id, 'hex') > $4 OR (encode(trace_id, 'hex') = $4 AND (encode(span_id, 'hex') > $5)))))
ORDER BY start_time_unix_nano DESC, encode(trace_id, 'hex') ASC, encode(span_id, 'hex') ASC
LIMIT 51;

-- trace-page-before | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","2026-11-18T12:30:00.000000000Z","00000000000000000000000000000001","0000000000000001"]
SELECT start_time_unix_nano, name, service_name, duration_nano, status_code, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL AND (start_time_unix_nano > $3::timestamptz OR (start_time_unix_nano = $3::timestamptz AND (encode(trace_id, 'hex') < $4 OR (encode(trace_id, 'hex') = $4 AND (encode(span_id, 'hex') < $5)))))
ORDER BY start_time_unix_nano ASC, encode(trace_id, 'hex') DESC, encode(span_id, 'hex') DESC
LIMIT 51;

-- trace-waterfall | waterfallQuery
-- Parameters: ["00000000000000000000000000000001"]
SELECT encode(span_id, 'hex') AS span_id, encode(parent_span_id, 'hex') AS parent_span_id, name, service_name, start_time_unix_nano, duration_nano, status_code
FROM bifrost.traces.spans
WHERE trace_id = decode($1, 'hex')
ORDER BY start_time_unix_nano, span_id;

-- span-detail | detailQuery
-- Parameters: ["00000000000000000000000000000001","0000000000000001"]
SELECT *
FROM bifrost.traces.spans
WHERE trace_id = decode($1, 'hex') AND span_id = decode($2, 'hex');

-- log-detail | detailQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T12:00:00.000Z","019a9660-0000-7000-8000-000000000001"]
SELECT *
FROM bifrost.logs.records
WHERE time_unix_nano = $1::timestamptz AND observed_time_unix_nano = $2::timestamptz AND wyrd_request_id = $3;

-- spans-list | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT start_time_unix_nano, name, service_name, duration_nano, status_code, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id, encode(parent_span_id, 'hex') AS parent_span_id
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
ORDER BY start_time_unix_nano DESC, encode(trace_id, 'hex') ASC, encode(span_id, 'hex') ASC
LIMIT 51;

-- metric-points-list | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT time_unix_nano, metric_name, metric_type, unit, CAST(int_value AS VARCHAR) AS int_value, double_value
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
ORDER BY time_unix_nano DESC
LIMIT 50;

