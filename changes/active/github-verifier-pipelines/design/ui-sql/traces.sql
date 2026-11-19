-- Original UI SQL from 4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36
-- Design input; not executed against a server.

-- trace-spans | traceOverviewQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, count(*) AS spans
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- trace-errors | traceOverviewQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, count(*) FILTER (
WHERE status_code = 2) AS errors
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- trace-duration | traceOverviewQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, approx_percentile_cont(duration_nano, 0.95) / 1e6 AS p95, approx_percentile_cont(duration_nano, 0.5) / 1e6 AS p50
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- trace-totals | traceOverviewQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS spans, count(DISTINCT trace_id) AS traces, count(DISTINCT trace_id) FILTER (
WHERE status_code = 2) AS error_traces, approx_percentile_cont(duration_nano, 0.95) / 1e6 AS span_p95_ms
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz;

-- trace-durations | traceOverviewQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT approx_percentile_cont(duration_nano, 0.95) / 1e6 AS p95_ms, approx_percentile_cont(duration_nano, 0.5) / 1e6 AS p50_ms
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL;

-- trace-facet-service_name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT service_name AS value, count(DISTINCT trace_id) AS n
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY service_name
ORDER BY n DESC, value
LIMIT 11;

-- trace-facet-name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT name AS value, count(DISTINCT trace_id) AS n
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY name
ORDER BY n DESC, value
LIMIT 11;

-- trace-facet-status_code | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT status_code AS value, count(DISTINCT trace_id) AS n
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY status_code
ORDER BY n DESC, value
LIMIT 11;

-- trace-facet-http_response_status_code | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT http_response_status_code AS value, count(DISTINCT trace_id) AS n
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY http_response_status_code
ORDER BY n DESC, value
LIMIT 11;

-- trace-facet-gen_ai_operation_name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT gen_ai_operation_name AS value, count(DISTINCT trace_id) AS n
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY gen_ai_operation_name
ORDER BY n DESC, value
LIMIT 11;

-- trace-list | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT start_time_unix_nano, name, service_name, duration_nano, status_code, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL
ORDER BY start_time_unix_nano DESC, encode(trace_id, 'hex') ASC, encode(span_id, 'hex') ASC
LIMIT 51;

-- trace-counts | countsQuery
-- Parameters: ["2026-11-18T11:00:00.000Z","2026-11-18T14:00:00.000Z","00000000000000000000000000000001","00000000000000000000000000000002"]
-- After: trace-list; replace example IDs/bounds with real preceding results.
SELECT encode(trace_id, 'hex') AS trace_id, count(*) AS spans, count(*) FILTER (
WHERE status_code = 2) AS errors
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND trace_id IN (decode($3, 'hex'), decode($4, 'hex'))
GROUP BY trace_id;

