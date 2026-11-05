-- Cumulative publication accounting, completed File custody, and indexed
-- page cuts of queued Verifier executions.
--
-- `publication_count` and `publication_bytes` total every publication the
-- execution accepted across all attempts: a JSON value's serialized length,
-- or a manifest's file sizes. They live on the execution row, which every
-- acceptance locks, so concurrent acceptances never lose a charge and a
-- reclaim or restart never replenishes them.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN publication_count INTEGER NOT NULL DEFAULT 0 CHECK (publication_count >= 0),
    ADD COLUMN publication_bytes BIGINT  NOT NULL DEFAULT 0 CHECK (publication_bytes >= 0);

-- The completed File descriptor shared completion returned for each manifest
-- file, in manifest order, so readers locate the exact accepted bytes
-- without rediscovering them. Private server metadata, never part of a
-- public manifest. A file or directory publication without them is malformed;
-- readers refuse it rather than guess a location.
ALTER TABLE wyrd.verifier_execution_publications
    ADD COLUMN objects JSONB,
    ADD CONSTRAINT verifier_execution_publications_objects
        CHECK (kind <> 'json' OR objects IS NULL);

-- Page cuts and continuations read one execution's rows by sequence.
CREATE INDEX verifier_execution_invocations_sequence
    ON wyrd.verifier_execution_invocations (data_tenant_id, execution_id, sequence);
CREATE INDEX verifier_execution_publications_sequence
    ON wyrd.verifier_execution_publications (data_tenant_id, execution_id, sequence);
