-- Admit the built-in bifrost.storage namespace to Forge scheduling identities.

ALTER TABLE vala.forge_tasks
    DROP CONSTRAINT forge_tasks_namespace_name_check,
    ADD CONSTRAINT forge_tasks_namespace_name_check
        CHECK (namespace_name IN ('bifrost.system','bifrost.bifrost','bifrost.traces','bifrost.metrics','bifrost.logs','bifrost.drift','bifrost.verification','bifrost.dev','bifrost.datasets','bifrost.gateway','bifrost.storage'))
        NOT VALID;
