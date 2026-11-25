-- Original UI SQL from 4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36
-- Design input; not executed against a server.

-- logs-list | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT time_unix_nano, severity_text, service_name, body_text, exception_type, encode(trace_id, 'hex') AS trace_id, encode(span_id, 'hex') AS span_id, observed_time_unix_nano, wyrd_request_id, severity_number
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
ORDER BY time_unix_nano DESC, observed_time_unix_nano ASC, wyrd_request_id ASC
LIMIT 51;

-- logs-timeline | logTimelineQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, time_unix_nano) AS at, CASE WHEN severity_number >= 17 THEN 'error' WHEN severity_number >= 13 THEN 'warn' WHEN severity_number >= 9 THEN 'info' WHEN severity_number >= 1 THEN 'debug' ELSE 'unset' END AS level, count(*) AS records
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY date_bin($3::interval, time_unix_nano), CASE WHEN severity_number >= 17 THEN 'error' WHEN severity_number >= 13 THEN 'warn' WHEN severity_number >= 9 THEN 'info' WHEN severity_number >= 1 THEN 'debug' ELSE 'unset' END
ORDER BY at;

-- logs-facet-severity_text | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT severity_text AS value, count(*) AS n, max(severity_number) AS severity_number
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY severity_text
ORDER BY n DESC, value
LIMIT 11;

-- logs-facet-service_name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT service_name AS value, count(*) AS n
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY service_name
ORDER BY n DESC, value
LIMIT 11;

-- logs-facet-event_name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT event_name AS value, count(*) AS n
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY event_name
ORDER BY n DESC, value
LIMIT 11;

-- logs-facet-exception_type | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT exception_type AS value, count(*) AS n
FROM bifrost.logs.records
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY exception_type
ORDER BY n DESC, value
LIMIT 11;

