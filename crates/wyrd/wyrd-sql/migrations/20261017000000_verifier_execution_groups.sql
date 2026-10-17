-- Record the caller's group on queued Verifier executions.
--
-- A submitted execution keeps the optional caller-chosen `group_id` it was
-- accepted with, so its result row carries it into Bifrost where grouped
-- Verifier bindings discover executions sharing it. The value is opaque
-- caller data scoped to the tenant and owner Service; it grants nothing.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN group_id TEXT
        CHECK (group_id IS NULL OR octet_length(group_id) BETWEEN 1 AND 256);
