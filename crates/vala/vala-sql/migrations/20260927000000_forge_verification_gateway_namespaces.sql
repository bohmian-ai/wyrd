-- Restore vala.verification, which the gateway migration dropped, so Forge
-- scheduling identities admit every built-in namespace.

ALTER TABLE vala.forge_tasks
    DROP CONSTRAINT forge_tasks_namespace_name_check,
    ADD CONSTRAINT forge_tasks_namespace_name_check
        CHECK (namespace_name IN ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.eval','vala.drift','vala.verification','vala.dev','vala.datasets','vala.gateway'));

ALTER TABLE vala.forge_planning_demands
    DROP CONSTRAINT forge_planning_demands_namespace_name_check,
    ADD CONSTRAINT forge_planning_demands_namespace_name_check
        CHECK (namespace_name IN ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.eval','vala.drift','vala.verification','vala.dev','vala.datasets','vala.gateway'));
