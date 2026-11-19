-- Original UI SQL from 4dd767d8f4eb5c20bdffea587e3ba2be1d38eb36
-- Design input; not executed against a server.

-- verify-list | listQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
ORDER BY recorded_at DESC, execution_id ASC
LIMIT 51;

-- verify-facet-status | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT status AS value, count(*) AS n
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY status
ORDER BY n DESC, value
LIMIT 11;

-- verify-facet-verdict | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT verdict AS value, count(*) AS n
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY verdict
ORDER BY n DESC, value
LIMIT 11;

-- verify-facet-verifier_card_uid | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT verifier_card_uid AS value, count(*) AS n
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY verifier_card_uid
ORDER BY n DESC, value
LIMIT 11;

-- verify-facet-service_name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT service_name AS value, count(*) AS n
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY service_name
ORDER BY n DESC, value
LIMIT 11;

-- verify-facet-subject_name | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT subject_name AS value, count(*) AS n
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY subject_name
ORDER BY n DESC, value
LIMIT 11;

-- verify-facet-run_id | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT run_id AS value, count(*) AS n
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY run_id
ORDER BY n DESC, value
LIMIT 11;

-- verify-facet-group_id | facetQuery
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z"]
SELECT group_id AS value, count(*) AS n
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY group_id
ORDER BY n DESC, value
LIMIT 11;

-- verify-chart-verdict | executionsBy
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, recorded_at) AS at, coalesce(verdict, 'no verdict') AS verdict, count(*) AS executions
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY 1, 2
ORDER BY at;

-- verify-chart-verifier | executionsBy
-- Parameters: ["2026-11-18T12:00:00.000Z","2026-11-18T13:00:00.000Z","1 minute"]
SELECT date_bin($3::interval, recorded_at) AS at, verifier_card_uid AS verifier, count(*) AS executions
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
WHERE recorded_at >= $1::timestamptz AND recorded_at < $2::timestamptz
GROUP BY 1, 2
ORDER BY at;

