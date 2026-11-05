-- Named publications accepted by queued Verifier invocation attempts.
--
-- One row is one immutable publication: the producer invocation attempt
-- (execution, step task, nullable zero-based fanout item and repeat
-- iteration, one-based attempt) and its logical name, unique within that
-- attempt. A file or directory publication holds its manifest of completed
-- verification Files, never their bytes; a JSON publication holds its bounded
-- value, JSON null included. The row commits under the execution's lease only
-- after every File it names completed, so a reader never sees partial
-- content. `call_id` and `arguments_sha256` identify the accepting call, so a
-- replay of that same call recovers its receipt while any other call reusing
-- the name conflicts. `sequence` orders acceptances within an attempt.
CREATE TABLE wyrd.verifier_execution_publications (
    data_tenant_id   UUID        NOT NULL,
    execution_id     UUID        NOT NULL,
    task_id          TEXT        NOT NULL,
    item_index       INTEGER     CHECK (item_index >= 0),
    iteration_index  INTEGER     CHECK (iteration_index >= 0),
    attempt          INTEGER     NOT NULL CHECK (attempt >= 1),
    name             TEXT        NOT NULL,
    kind             TEXT        NOT NULL CHECK (kind IN ('file', 'directory', 'json')),
    call_id          TEXT        NOT NULL,
    arguments_sha256 TEXT        NOT NULL,
    value            JSONB,
    manifest         JSONB,
    sequence         BIGINT      GENERATED ALWAYS AS IDENTITY,
    accepted_at      TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (data_tenant_id, execution_id)
        REFERENCES wyrd.verifier_executions(data_tenant_id, execution_id) ON DELETE CASCADE,
    CHECK ((kind = 'json') = (value IS NOT NULL)),
    CHECK ((kind = 'json') = (manifest IS NULL))
);

CREATE UNIQUE INDEX verifier_execution_publications_identity
    ON wyrd.verifier_execution_publications
       (data_tenant_id, execution_id, task_id, item_index, iteration_index, attempt, name)
    NULLS NOT DISTINCT;

ALTER TABLE wyrd.verifier_execution_publications ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.verifier_execution_publications FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.verifier_execution_publications
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.verifier_execution_publications TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
