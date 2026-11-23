-- Original UI SQL from 4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36
-- Design input; not executed against a server.

-- metrics-facet-service_name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT service_name AS value, count(*) AS n
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY service_name
ORDER BY n DESC, value
LIMIT 11;

-- metrics-facet-subject_card_uid | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT subject_card_uid AS value, count(*) AS n
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz
GROUP BY subject_card_uid
ORDER BY n DESC, value
LIMIT 11;

-- metrics-names | metricNamesQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT metric_name, metric_type, unit
FROM bifrost.metrics.series
WHERE hour_unix_nano >= date_bin(INTERVAL '1 hour', $1::timestamptz) AND hour_unix_nano < $2::timestamptz
GROUP BY metric_name, metric_type, unit
ORDER BY metric_name, metric_type, unit
LIMIT 51;

-- metrics-chart | metricQueries
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

-- metrics-histogram | histogramQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","m.requests","1 minute"]
WITH pts AS (
SELECT date_bin($4::interval, time_unix_nano) AS w, time_unix_nano AS t, start_time_unix_nano AS started, metric_type, unit, aggregation_temporality, is_monotonic, explicit_bounds AS bounds, series_id, CAST(histogram_count AS BIGINT) AS n, histogram_sum AS v, CAST(bucket_counts AS BIGINT[]) AS b
FROM bifrost.metrics.points
WHERE time_unix_nano >= $1::timestamptz AND time_unix_nano < $2::timestamptz AND metric_name = $3 AND metric_type = 'histogram'), steps AS (
SELECT *, lag(t) OVER series IS NULL AS head, lag(started) OVER series AS sp, lag(n) OVER series AS np, lag(v) OVER series AS vp, lag(b) OVER series AS bp
FROM pts
WHERE aggregation_temporality = 2
WINDOW series AS (
PARTITION BY series_id
ORDER BY t)), counted AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, bounds, unnest(generate_series(1, CAST(cardinality(b) AS BIGINT))) AS i, unnest(b) AS c, CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN b END WHEN sp = started AND n >= np THEN bp END AS bb, n - CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN n ELSE 0 END WHEN sp = started AND n >= np THEN np ELSE 0 END AS dn, v - CASE WHEN head THEN CASE WHEN started < $1::timestamptz THEN v ELSE 0 END WHEN sp = started AND n >= np THEN vp ELSE 0 END AS dv
FROM steps
UNION ALL
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, bounds, unnest(generate_series(1, CAST(cardinality(b) AS BIGINT))) AS i, unnest(b) AS c, NULL AS bb, n AS dn, v AS dv
FROM pts
WHERE aggregation_temporality <> 2), buckets AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, bounds, i, sum(CAST(c AS BIGINT) - COALESCE(CAST(array_element(bb, i) AS BIGINT), 0)) AS c, sum(CASE WHEN i = 1 THEN dn END) AS dn, sum(CASE WHEN i = 1 THEN dv END) AS dv, count(CASE WHEN i = 1 THEN dn END) AS kn, count(CASE WHEN i = 1 THEN dv END) AS kv
FROM counted
GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, bounds, i), windows AS (
SELECT w, metric_type, unit, aggregation_temporality, is_monotonic, bounds, sum(dn) AS count, CASE WHEN sum(kv) = sum(kn) THEN sum(dv) END AS sum, array_agg(CAST(c AS VARCHAR)
ORDER BY i) AS buckets
FROM buckets
GROUP BY w, metric_type, unit, aggregation_temporality, is_monotonic, bounds), ranked AS (
SELECT *, dense_rank() OVER (
ORDER BY measure DESC, metric_type, unit, aggregation_temporality, is_monotonic, bounds) AS rank
FROM (
SELECT *, sum(count) OVER (
PARTITION BY metric_type, unit, aggregation_temporality, is_monotonic, bounds) AS measure
FROM windows
WHERE count > 0))
SELECT w AS at, metric_type, unit, aggregation_temporality, is_monotonic, bounds, count, sum, buckets, groups
FROM (
SELECT *, max(rank) OVER () AS groups
FROM ranked)
WHERE rank <= 20
ORDER BY at;

-- metrics-group-keys | groupKeysQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","m.requests"]
SELECT to_json(attributes) AS attributes, to_json(resource_attributes) AS resource, to_json(scope_attributes) AS scope
FROM bifrost.metrics.series
WHERE hour_unix_nano >= date_bin(INTERVAL '1 hour', $1::timestamptz) AND hour_unix_nano < $2::timestamptz AND metric_name = $3
LIMIT 100;

