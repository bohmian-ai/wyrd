-- Original UI SQL from 4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36
-- Design input; not executed against a server.

-- stat-traces-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL;

-- stat-traces-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- stat-spans-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz;

-- stat-spans-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- stat-errors-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND status_code = 2;

-- stat-errors-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND status_code = 2
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- stat-duration_p95-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT approx_percentile_cont(duration_nano, 0.95) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL;

-- stat-duration_p95-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, approx_percentile_cont(duration_nano, 0.95) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND parent_span_id IS NULL
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- stat-invocations-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND gen_ai_operation_name = 'invoke_agent';

-- stat-invocations-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, start_time_unix_nano) AS at, count(*) AS value
FROM bifrost.traces.spans
WHERE start_time_unix_nano >= $1::timestamptz AND start_time_unix_nano < $2::timestamptz AND gen_ai_operation_name = 'invoke_agent'
GROUP BY date_bin($3::interval, start_time_unix_nano)
ORDER BY at;

-- stat-logs-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS value
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz;

-- stat-logs-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, time_unix_nano) AS at, count(*) AS value
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY date_bin($3::interval, time_unix_nano)
ORDER BY at;

-- stat-log_errors-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS value
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz AND severity_number >= 17;

-- stat-log_errors-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, time_unix_nano) AS at, count(*) AS value
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz AND severity_number >= 17
GROUP BY date_bin($3::interval, time_unix_nano)
ORDER BY at;

-- stat-tokens-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT sum(CAST(u['quantity'] AS DECIMAL(38, 12))) AS value
FROM (
SELECT started_at, workload_card_uid, subject_card_uid, unnest(usage) AS u
FROM bifrost.gateway.calls) AS usage
WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz AND u['unit'] = 'tokens';

-- stat-tokens-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, started_at) AS at, sum(CAST(u['quantity'] AS DECIMAL(38, 12))) AS value
FROM (
SELECT started_at, workload_card_uid, subject_card_uid, unnest(usage) AS u
FROM bifrost.gateway.calls) AS usage
WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz AND u['unit'] = 'tokens'
GROUP BY date_bin($3::interval, started_at)
ORDER BY at;

-- stat-calls-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT count(*) AS value
FROM bifrost.gateway.calls
WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz;

-- stat-calls-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, started_at) AS at, count(*) AS value
FROM bifrost.gateway.calls
WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz
GROUP BY date_bin($3::interval, started_at)
ORDER BY at;

-- stat-cost-total | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT sum(CAST(cost AS DECIMAL(38, 12))) AS value, currency
FROM bifrost.gateway.calls
WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz AND cost IS NOT NULL
GROUP BY currency
ORDER BY currency;

-- stat-cost-buckets | statQueries
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, started_at) AS at, sum(CAST(cost AS DECIMAL(38, 12))) AS value, currency
FROM bifrost.gateway.calls
WHERE started_at >= $1::timestamptz AND started_at < $2::timestamptz AND cost IS NOT NULL
GROUP BY date_bin($3::interval, started_at), currency
ORDER BY at;

