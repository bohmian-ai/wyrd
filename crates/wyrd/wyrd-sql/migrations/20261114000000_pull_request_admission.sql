-- Pull request admission: one ordinary queued execution per frozen
-- pull_request binding of a Repository's PR source generation.
--
-- `pull_request` holds the accepted PR pins: provider host, repository id,
-- PR number, base ref/sha, head ref/sha and head repository id, the
-- provider delivery that admitted them, and the generation every execution
-- of one admission shares, with its `pr_observed_at`: the PostgreSQL time before
-- the provider PR read that confirmed the source, so a delayed observation
-- older than the current generation is obsolete. `pr_deactivated_at` marks
-- a generation whose PR was closed, turned into a draft or lost repository
-- access; history is kept. A binding refused at admission is one terminal
-- errored execution without accepted authority.
--
-- A generation is reserved before its input records are acknowledged:
-- `pending` and due at `infinity`, which no claim reaches, so its frozen
-- policy and identities survive a crash before the handoff completes. The
-- acknowledged handoff makes it due at PostgreSQL's statement time.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN pull_request JSONB,
    ADD COLUMN pr_observed_at TIMESTAMPTZ,
    ADD COLUMN pr_deactivated_at TIMESTAMPTZ;

ALTER TABLE wyrd.verifier_executions
    DROP CONSTRAINT verifier_runs_origin_check,
    ADD CONSTRAINT verifier_executions_origin_check
        CHECK (origin IN ('schedule', 'manual', 'observation', 'group', 'pull_request')),
    DROP CONSTRAINT verifier_executions_live_origin,
    ADD CONSTRAINT verifier_executions_live_origin CHECK (
        origin IN ('schedule', 'manual', 'group', 'pull_request')
    ) NOT VALID,
    DROP CONSTRAINT verifier_executions_input_shape,
    ADD CONSTRAINT verifier_executions_input_shape CHECK (
        (window_start IS NOT NULL AND window_end IS NOT NULL AND window_start < window_end
         AND input_record_id IS NULL AND input_event_time IS NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NOT NULL AND input_event_time IS NOT NULL)
        OR (window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND origin IN ('manual', 'group', 'pull_request'))
    ),
    ADD CONSTRAINT verifier_executions_pull_request_shape CHECK (
        (origin = 'pull_request') = (jsonb_typeof(pull_request) IS NOT DISTINCT FROM 'object')
        AND (origin = 'pull_request') = (pr_observed_at IS NOT NULL)
        AND (pr_deactivated_at IS NULL OR origin = 'pull_request')
    ),
    DROP CONSTRAINT verifier_executions_refused_shape,
    ADD CONSTRAINT verifier_executions_refused_shape CHECK (
        authority IS NOT NULL
        OR (origin IN ('group', 'pull_request') AND status = 'errored' AND attempts = 0
            AND (origin <> 'group' OR group_id IS NOT NULL) AND result_id IS NULL
            AND lease_token IS NULL AND settled_at IS NOT NULL
            AND window_start IS NULL AND window_end IS NULL
            AND input_record_id IS NULL AND input_event_time IS NULL
            AND group_snapshot_sha256 IS NULL)
    );

-- One PR's generations, newest observation last.
CREATE INDEX verifier_executions_pull_request
    ON wyrd.verifier_executions (
        data_tenant_id, (pull_request->>'host'), (pull_request->>'repository_id'),
        ((pull_request->>'number')::bigint), pr_observed_at
    )
    WHERE origin = 'pull_request';

-- Execution status and publications of queued Verifier executions are read
-- under the `verifier:read` permission rather than `cards:read`. Every
-- built-in Role carries its unscoped grant, so the built-in Roles a tenant
-- was seeded with before it existed gain it here; tenant-defined Roles are
-- left to their administrators. The migrator connects as operator, so the
-- update spans every tenant.
UPDATE wyrd.auth_roles
   SET permissions = permissions
       || '[{"resource": "verifier", "action": "read", "scope": "all"}]'::jsonb
 WHERE builtin
   AND name IN ('viewer', 'workload', 'editor')
   AND NOT permissions @> '[{"resource": "verifier", "action": "read", "scope": "all"}]'::jsonb;
