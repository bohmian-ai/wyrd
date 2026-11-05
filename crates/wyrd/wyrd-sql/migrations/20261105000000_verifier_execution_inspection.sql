-- Public inspection of queued Verifier executions.
--
-- Execution status, invocation progress, and publications are read under the
-- new `verifier:read` permission rather than `cards:read`. Every built-in Role
-- carries its unscoped grant, so the built-in Roles a tenant was seeded with
-- before it existed gain it here; tenant-defined Roles are left to their
-- administrators. The migrator connects as operator, so the update spans
-- every tenant.
UPDATE wyrd.auth_roles
   SET permissions = permissions
       || '[{"resource": "verifier", "action": "read", "scope": "all"}]'::jsonb
 WHERE builtin
   AND name IN ('viewer', 'workload', 'editor')
   AND NOT permissions @> '[{"resource": "verifier", "action": "read", "scope": "all"}]'::jsonb;

-- Commit-independent insertion order of an execution's invocation attempts,
-- the stable cut and position an invocation page is read at, as
-- `verifier_execution_publications.sequence` is for publications.
ALTER TABLE wyrd.verifier_execution_invocations
    ADD COLUMN sequence BIGINT GENERATED ALWAYS AS IDENTITY;
