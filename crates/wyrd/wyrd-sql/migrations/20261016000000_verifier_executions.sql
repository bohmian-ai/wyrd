-- Move the Verifier queue to building-block executions.
--
-- `wyrd.verifier_runs` becomes `wyrd.verifier_executions`, keyed by the
-- server-issued `execution_id`. A caller-submitted execution carries its
-- optional application `run_id`, its named input declarations, and its route
-- overrides beside the accepted authority it was submitted under; a scheduled
-- execution keeps its binding and window. Operator dispatches reference the
-- execution, fitted Drift baselines are keyed by requirement, and Verifier
-- readiness reads the building-block spec.
--
-- History is kept. Queued work created under the retired Verifier contract
-- cannot execute under the new one, so every non-terminal row settles
-- `cancelled` with a stated reason; settled rows, their dispatches, and their
-- frozen failure contexts are untouched. Rows of the retired `observation`
-- origin and `observations_ready` activation stay as recorded; new ones are
-- refused by `NOT VALID` checks that existing rows are exempt from.

UPDATE wyrd.verifier_runs
   SET status = 'cancelled',
       error = jsonb_build_object(
           'code', 'verifier_contract_retired',
           'message', 'queued under a retired Verifier contract; it cannot execute'),
       next_attempt_at = NULL, lease_token = NULL, lease_expires_at = NULL,
       settled_at = statement_timestamp(), updated_at = statement_timestamp()
 WHERE status IN ('pending', 'running', 'retrying');

ALTER TABLE wyrd.verifier_runs RENAME TO verifier_executions;
ALTER TABLE wyrd.verifier_executions RENAME COLUMN run_id TO execution_id;

ALTER TABLE wyrd.verifier_executions
    ADD COLUMN run_id          TEXT,
    ADD COLUMN inputs          JSONB,
    ADD COLUMN route_overrides JSONB,
    ADD COLUMN group_id        TEXT
        CHECK (group_id IS NULL OR octet_length(group_id) BETWEEN 1 AND 256),
    ADD COLUMN group_through   TIMESTAMPTZ,
    ADD COLUMN group_members   BIGINT CHECK (group_members IS NULL OR group_members > 0),
    ADD COLUMN group_sources   JSONB,
    ADD COLUMN group_verdict   TEXT
        CHECK (group_verdict IS NULL OR group_verdict IN ('passed', 'failed', 'inconclusive'));

-- Exactly one frozen input: a Drift window, a historical Eval record, or a
-- caller submission's or group snapshot's input declarations and route
-- overrides. A submitted execution keeps its opaque caller `group_id`; a
-- `group` execution judges one pinned snapshot of its binding's group, where
-- `group_through` and `group_members` are the newest eligible source row time
-- and distinct source input/result rows it covers, `group_sources` names each
-- source slot's selected identities or the read ceiling it exceeded, and
-- `group_verdict` is the settled verdict the next group result is compared
-- with for `on_verdict_change`.
ALTER TABLE wyrd.verifier_executions DROP CONSTRAINT verifier_runs_check4;
ALTER TABLE wyrd.verifier_executions
    ADD CONSTRAINT verifier_executions_input_shape CHECK (
        (window_start IS NOT NULL AND window_end IS NOT NULL AND window_start < window_end
         AND input_record_id IS NULL AND input_event_time IS NULL
         AND inputs IS NULL AND route_overrides IS NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NOT NULL AND input_event_time IS NOT NULL
            AND inputs IS NULL AND route_overrides IS NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND origin IN ('manual', 'group')
            AND jsonb_typeof(inputs) = 'object'
            AND jsonb_typeof(route_overrides) = 'object')
    ),
    ADD CONSTRAINT verifier_executions_run_shape CHECK (
        run_id IS NULL OR (origin = 'manual' AND inputs IS NOT NULL)
    ),
    ADD CONSTRAINT verifier_executions_group_shape CHECK (
        (origin = 'group') = (group_through IS NOT NULL)
        AND (origin = 'group') = (group_members IS NOT NULL)
        AND (origin = 'group') = (jsonb_typeof(group_sources) IS NOT DISTINCT FROM 'object')
        AND (origin <> 'group' OR group_id IS NOT NULL)
        AND (group_verdict IS NULL OR (origin = 'group' AND status = 'completed'))
    ),
    ADD CONSTRAINT verifier_executions_live_origin CHECK (
        origin IN ('schedule', 'manual', 'group')
    ) NOT VALID;

-- At most one group execution is outstanding per binding and group, so
-- arrivals during it coalesce into the next snapshot.
CREATE UNIQUE INDEX verifier_executions_group_outstanding
    ON wyrd.verifier_executions (data_tenant_id, binding_id, group_id)
    WHERE origin = 'group' AND status IN ('pending', 'running', 'retrying');

ALTER TABLE wyrd.operator_dispatches RENAME COLUMN run_id TO execution_id;

-- Live dispatches render their frozen context on a later delivery attempt,
-- so they take the new field names; settled dispatches keep theirs.
UPDATE wyrd.operator_dispatches
   SET failure_context = (failure_context - 'run_id' - 'verifier')
       || jsonb_build_object('execution_id', failure_context -> 'run_id')
       || CASE WHEN failure_context #>> '{verifier,implementation}' = 'drift'
               THEN jsonb_build_object('drift', jsonb_build_object(
                        'drifted_features', failure_context #> '{verifier,drifted_features}',
                        'total_features', failure_context #> '{verifier,total_features}'))
               ELSE '{}'::jsonb
          END,
       updated_at = statement_timestamp()
 WHERE status IN ('pending', 'running', 'retrying')
   AND failure_context ? 'run_id';

ALTER TABLE wyrd.verification_bindings
    ADD COLUMN inputs JSONB NOT NULL DEFAULT '{}'::jsonb
        CHECK (jsonb_typeof(inputs) = 'object'),
    ADD CONSTRAINT verification_bindings_live_activation CHECK (
        activation IN ('schedule', 'group_end')
    ) NOT VALID;

-- Fitted state is keyed by the drift check's requirement. Baselines fitted
-- for retired Verifier versions keep no requirement and stop being fitted.
ALTER TABLE wyrd.drift_baselines ADD COLUMN requirement_id TEXT;
UPDATE wyrd.drift_baselines
   SET state = 'failed',
       error = jsonb_build_object(
           'code', 'verifier_contract_retired',
           'message', 'registered under a retired Verifier contract; it is not fitted'),
       next_attempt_at = NULL, lease_token = NULL, lease_expires_at = NULL,
       updated_at = statement_timestamp()
 WHERE state IN ('pending', 'building')
    OR (state = 'failed' AND next_attempt_at IS NOT NULL);
ALTER TABLE wyrd.drift_baselines
    DROP CONSTRAINT drift_baselines_pkey,
    ADD CONSTRAINT drift_baselines_requirement_key
        UNIQUE (data_tenant_id, verifier_uid, requirement_id),
    ADD CONSTRAINT drift_baselines_requirement_present CHECK (
        requirement_id IS NOT NULL
    ) NOT VALID;

-- A Verifier is ready when it is an active building-block Verifier and every
-- PSI or SPC drift check, in a step or the whole Verifier, has a ready fitted
-- baseline for its requirement.
CREATE OR REPLACE FUNCTION wyrd.verifier_readiness(p_verifier_uid UUID) RETURNS TEXT
LANGUAGE sql STABLE AS $$
    SELECT CASE
        WHEN v.card_uid IS NULL
          OR v.kind <> 'Verifier'
          OR v.status NOT IN ('active', 'deprecated')
          OR v.spec ? 'implementation'
            THEN 'verifier_unavailable'
        WHEN EXISTS (
            SELECT 1
              FROM (SELECT r
                      FROM jsonb_array_elements(COALESCE(v.spec -> 'requirements', '[]'::jsonb)) AS r
                    UNION ALL
                    SELECT r
                      FROM jsonb_array_elements(COALESCE(v.spec -> 'steps', '[]'::jsonb)) AS s,
                           jsonb_array_elements(COALESCE(s -> 'requirements', '[]'::jsonb)) AS r
                   ) AS requirement(r)
             WHERE requirement.r #>> '{check,kind}' = 'drift'
               AND requirement.r #>> '{check,method}' IN ('Psi', 'Spc')
               AND NOT EXISTS (
                   SELECT 1 FROM wyrd.drift_baselines b
                    WHERE b.verifier_uid = v.card_uid
                      AND b.requirement_id = requirement.r ->> 'id'
                      AND b.state = 'ready'))
            THEN 'baseline_not_ready'
        ELSE 'ready'
    END
      FROM (SELECT 1) AS probe
      LEFT JOIN wyrd.cards v ON v.card_uid = p_verifier_uid
$$;
