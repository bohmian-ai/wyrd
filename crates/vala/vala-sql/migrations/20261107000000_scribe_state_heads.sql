-- Durable head of one keyed state a state-fenced table records.
--
-- A state-fenced table holds one row per accepted revision of each keyed
-- state. The head names the only writer that may advance the state and the
-- revision it last accepted: a write advances revision r to r + 1 only from
-- the current writer, in the same tenant transaction that records its batch
-- fence, so acceptance and batch durability commit together or not at all. A
-- claim installs a new writer and returns the accepted position, so a later
-- write of the replaced writer is refused. The head holds no state payload.
CREATE TABLE vala.scribe_state_heads (
    data_tenant_id uuid NOT NULL REFERENCES platform.tenants(data_tenant_id),
    logical_table_fqn text NOT NULL,
    state_key uuid NOT NULL,
    -- The only writer whose write may advance the state.
    writer uuid NOT NULL,
    -- Claims order by generation: a claim older than the current one is
    -- refused, so a delayed claim cannot displace a newer writer.
    generation bigint NOT NULL CHECK (generation >= 0),
    -- The accepted revision; 0 means no revision was ever accepted.
    revision bigint NOT NULL DEFAULT 0 CHECK (revision >= 0),
    -- The batch whose fence accepted `revision`.
    batch_id uuid,
    PRIMARY KEY (data_tenant_id, logical_table_fqn, state_key),
    CHECK ((revision = 0) = (batch_id IS NULL))
);

ALTER TABLE vala.scribe_state_heads ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.scribe_state_heads FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.scribe_state_heads
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON vala.scribe_state_heads TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
