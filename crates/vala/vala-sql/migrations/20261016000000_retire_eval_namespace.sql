-- Retire the vala.eval namespace from Forge scheduling identities.
--
-- The Eval tables no longer exist, so their maintenance tasks have no table to
-- act on. Snapshot-expiration claims reference their task without a cascade,
-- so they are removed first; remaining namespaces are untouched.

DELETE FROM vala.forge_snapshot_expiration_claims
WHERE task_id IN (SELECT task_id FROM vala.forge_tasks WHERE namespace_name = 'vala.eval');

DELETE FROM vala.forge_tasks WHERE namespace_name = 'vala.eval';

ALTER TABLE vala.forge_tasks
    DROP CONSTRAINT forge_tasks_namespace_name_check,
    ADD CONSTRAINT forge_tasks_namespace_name_check
        CHECK (namespace_name IN ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.drift','vala.verification','vala.dev','vala.datasets','vala.gateway'));
