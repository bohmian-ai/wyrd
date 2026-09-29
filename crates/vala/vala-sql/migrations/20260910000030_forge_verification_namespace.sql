-- Let Forge maintain the verification result tables.
--
-- `vala.verification.results` is a Bifrost table like any other, but Forge's
-- namespace allowlist predates it, so every planning hint for it was refused
-- and the table was never compacted. Widening the accepted set is the whole
-- fix; existing rows are unaffected.
ALTER TABLE vala.forge_tasks DROP CONSTRAINT forge_tasks_namespace_name_check;
ALTER TABLE vala.forge_tasks ADD CONSTRAINT forge_tasks_namespace_name_check
    CHECK (namespace_name IN ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.eval','vala.drift','vala.verification','vala.dev','vala.datasets'));
ALTER TABLE vala.forge_planning_demands DROP CONSTRAINT forge_planning_demands_namespace_name_check;
ALTER TABLE vala.forge_planning_demands ADD CONSTRAINT forge_planning_demands_namespace_name_check
    CHECK (namespace_name IN ('vala.system','vala.bifrost','vala.traces','vala.metrics','vala.logs','vala.eval','vala.drift','vala.verification','vala.dev','vala.datasets'));
