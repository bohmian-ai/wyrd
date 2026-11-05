-- Replayable progress history of queued Verifier executions.
--
-- Every execution status or attempt change, invocation start, settlement, or
-- abandonment, and accepted publication inserts one event in the same
-- transaction as the transition it records, through the triggers below, so
-- no committed transition lacks its event and no rolled-back one has one.
-- `sequence` orders a tenant's events in commit-independent insertion order;
-- `event_id` is the server-assigned identity Bifrost history keeps.
--
-- Events are projected into `bifrost.verification.execution_events` through
-- the Scribe outbox. A projector assigns unbatched events a `batch_id` and
-- commits it before submitting, so a retry after an ambiguous outcome
-- resubmits the same rows under the same batch identity and Scribe's batch
-- dedup absorbs it. `acknowledged_at` is set once Scribe acknowledges the
-- batch; `projection_attempts` counts submissions that were not.
--
-- Bytes never appear here: a publication event names its total size, file
-- count, and a file's digest, and carries only a JSON publication's value,
-- which acceptance already bounded.
CREATE TABLE wyrd.verifier_execution_events (
    data_tenant_id      UUID        NOT NULL,
    sequence            BIGINT      GENERATED ALWAYS AS IDENTITY,
    event_id            UUID        NOT NULL DEFAULT gen_random_uuid(),
    execution_id        UUID        NOT NULL,
    event               TEXT        NOT NULL
        CHECK (event IN ('execution', 'invocation', 'invocation_abandoned', 'publication')),
    status              TEXT,
    task_id             TEXT,
    item_index          INTEGER,
    iteration_index     INTEGER,
    attempt             INTEGER,
    publication_name    TEXT,
    publication_kind    TEXT,
    size_bytes          BIGINT,
    file_count          INTEGER,
    sha256              TEXT,
    reason              TEXT,
    value               JSONB,
    occurred_at         TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    batch_id            UUID,
    projection_attempts INTEGER     NOT NULL DEFAULT 0,
    acknowledged_at     TIMESTAMPTZ,
    PRIMARY KEY (data_tenant_id, sequence),
    FOREIGN KEY (data_tenant_id, execution_id)
        REFERENCES wyrd.verifier_executions(data_tenant_id, execution_id) ON DELETE CASCADE,
    CHECK (acknowledged_at IS NULL OR batch_id IS NOT NULL)
);

-- One execution's history, in order, for inspection and projection position.
CREATE INDEX verifier_execution_events_execution
    ON wyrd.verifier_execution_events (data_tenant_id, execution_id, sequence);

-- Unprojected events only, so the projector's scan stays bounded by its lag
-- rather than the tenant's history.
CREATE INDEX verifier_execution_events_unacknowledged
    ON wyrd.verifier_execution_events (data_tenant_id, sequence)
    WHERE acknowledged_at IS NULL;

ALTER TABLE wyrd.verifier_execution_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.verifier_execution_events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.verifier_execution_events
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.verifier_execution_events TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- An execution's status or attempt change; `attempt` is the execution's
-- attempt count, null before its first claim, and `reason` its error code.
CREATE FUNCTION wyrd.record_verifier_execution_event() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    INSERT INTO wyrd.verifier_execution_events
        (data_tenant_id, execution_id, event, status, attempt, reason)
    VALUES (NEW.data_tenant_id, NEW.execution_id, 'execution', NEW.status,
            NULLIF(NEW.attempts, 0), NEW.error->>'code');
    RETURN NULL;
END
$$;

CREATE TRIGGER verifier_executions_record_insert_event
    AFTER INSERT ON wyrd.verifier_executions
    FOR EACH ROW EXECUTE FUNCTION wyrd.record_verifier_execution_event();

CREATE TRIGGER verifier_executions_record_update_event
    AFTER UPDATE OF status, attempts ON wyrd.verifier_executions
    FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status OR OLD.attempts IS DISTINCT FROM NEW.attempts)
    EXECUTE FUNCTION wyrd.record_verifier_execution_event();

-- An invocation attempt's start, settlement, or abandonment. A settlement's
-- status is its step status or requirement outcome; its reason is bounded.
-- A deletion is recorded only while its execution exists, so removing an
-- execution with its checkpoints records nothing.
CREATE FUNCTION wyrd.record_verifier_invocation_event() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        INSERT INTO wyrd.verifier_execution_events
            (data_tenant_id, execution_id, event, status, task_id, item_index,
             iteration_index, attempt)
        SELECT OLD.data_tenant_id, OLD.execution_id, 'invocation_abandoned', NULL,
               OLD.task_id, OLD.item_index, OLD.iteration_index, OLD.attempt
         WHERE EXISTS (SELECT 1 FROM wyrd.verifier_executions e
                        WHERE e.data_tenant_id = OLD.data_tenant_id
                          AND e.execution_id = OLD.execution_id);
        RETURN NULL;
    END IF;
    INSERT INTO wyrd.verifier_execution_events
        (data_tenant_id, execution_id, event, status, task_id, item_index,
         iteration_index, attempt, reason)
    VALUES (NEW.data_tenant_id, NEW.execution_id, 'invocation',
            COALESCE(NEW.settlement->'result'->>'status',
                     NEW.settlement->'result'->>'outcome', 'started'),
            NEW.task_id, NEW.item_index, NEW.iteration_index, NEW.attempt,
            left(NEW.settlement->'result'->>'reason', 1024));
    RETURN NULL;
END
$$;

CREATE TRIGGER verifier_execution_invocations_record_insert_event
    AFTER INSERT ON wyrd.verifier_execution_invocations
    FOR EACH ROW EXECUTE FUNCTION wyrd.record_verifier_invocation_event();

CREATE TRIGGER verifier_execution_invocations_record_settle_event
    AFTER UPDATE OF settlement ON wyrd.verifier_execution_invocations
    FOR EACH ROW WHEN (OLD.settlement IS NULL AND NEW.settlement IS NOT NULL)
    EXECUTE FUNCTION wyrd.record_verifier_invocation_event();

CREATE TRIGGER verifier_execution_invocations_record_delete_event
    AFTER DELETE ON wyrd.verifier_execution_invocations
    FOR EACH ROW WHEN (OLD.settlement IS NULL)
    EXECUTE FUNCTION wyrd.record_verifier_invocation_event();

-- An accepted publication: its name, kind, total size, file count, a file's
-- digest, and a JSON publication's value.
CREATE FUNCTION wyrd.record_verifier_publication_event() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    INSERT INTO wyrd.verifier_execution_events
        (data_tenant_id, execution_id, event, task_id, item_index, iteration_index,
         attempt, publication_name, publication_kind, size_bytes, file_count, sha256, value)
    VALUES (NEW.data_tenant_id, NEW.execution_id, 'publication', NEW.task_id,
            NEW.item_index, NEW.iteration_index, NEW.attempt, NEW.name, NEW.kind,
            CASE WHEN NEW.kind = 'json' THEN pg_catalog.octet_length(NEW.value::text)
                 ELSE (SELECT COALESCE(sum((f->>'size_bytes')::bigint), 0)
                         FROM pg_catalog.jsonb_array_elements(NEW.manifest->'files') AS f)
            END,
            CASE WHEN NEW.kind = 'json' THEN NULL
                 ELSE pg_catalog.jsonb_array_length(NEW.manifest->'files') END,
            CASE WHEN NEW.kind = 'file' THEN NEW.manifest->'files'->0->>'sha256' END,
            NEW.value);
    RETURN NULL;
END
$$;

CREATE TRIGGER verifier_execution_publications_record_event
    AFTER INSERT ON wyrd.verifier_execution_publications
    FOR EACH ROW EXECUTE FUNCTION wyrd.record_verifier_publication_event();
