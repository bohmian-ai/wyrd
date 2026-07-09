-- Stage 4 control tables for domain table index declarations and entity time bounds.
-- vala.olap_indexes:      declared indexes (bloom/zone_map/lookup_set) per domain table.
-- vala.entity_time_bounds: per-entity min/max wyrd_event_time for bounded id-lookup (M-06).
-- Both tenant-scoped with landed RLS pattern: ENABLE+FORCE RLS, USING+WITH CHECK.

CREATE TABLE vala.olap_indexes (
    data_tenant_id  UUID    NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid       BYTEA   NOT NULL CHECK (octet_length(table_uid) = 16),
    column_name     TEXT    NOT NULL,
    index_kind      TEXT    NOT NULL CHECK (index_kind IN ('bloom', 'zone_map', 'lookup_set', 'none')),
    index_state     TEXT    NOT NULL CHECK (index_state IN ('building', 'ready', 'failed', 'deprecated')),
    params          JSONB,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, table_uid, column_name, index_kind),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
        ON DELETE CASCADE
);

-- Valid state transitions enforced by trigger below:
--   building → ready | failed
--   ready    → deprecated
-- 'failed' and 'deprecated' are terminal.
CREATE OR REPLACE FUNCTION vala.check_index_state_transition() RETURNS trigger
    LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.index_state = NEW.index_state THEN
        RETURN NEW;
    END IF;
    IF OLD.index_state = 'building' AND NEW.index_state IN ('ready', 'failed') THEN
        RETURN NEW;
    END IF;
    IF OLD.index_state = 'ready' AND NEW.index_state = 'deprecated' THEN
        RETURN NEW;
    END IF;
    RAISE EXCEPTION 'invalid index_state transition: % → %', OLD.index_state, NEW.index_state;
END;
$$;

CREATE TRIGGER olap_indexes_state_transition
    BEFORE UPDATE OF index_state ON vala.olap_indexes
    FOR EACH ROW EXECUTE FUNCTION vala.check_index_state_transition();

ALTER TABLE vala.olap_indexes ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.olap_indexes FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.olap_indexes
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE INDEX olap_indexes_table_state_idx
    ON vala.olap_indexes (data_tenant_id, table_uid, index_state)
    WHERE index_state IN ('building', 'ready');

-- entity_time_bounds: best-effort acceleration for by-id lookups (F-05 / M-06).
-- Producer: the generic Bifrost commit path for tables with entity_bounds_mapping().
-- Consumer: ValaQueryService id-lookups (task 16) to bound the time-window scan.
-- Contract: bounds are an over-approximation (LEAST/GREATEST) — never gate a read on them.
-- entity_kind ∈ {trace, agent_run, session}.
CREATE TABLE vala.entity_time_bounds (
    data_tenant_id  UUID    NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid       BYTEA   NOT NULL CHECK (octet_length(table_uid) = 16),
    entity_kind     TEXT    NOT NULL CHECK (entity_kind IN ('trace', 'agent_run', 'session')),
    entity_id       TEXT    NOT NULL,
    min_event_time  TIMESTAMPTZ NOT NULL,
    max_event_time  TIMESTAMPTZ NOT NULL,
    CONSTRAINT entity_time_bounds_ordering
        CHECK (min_event_time <= max_event_time),
    PRIMARY KEY (data_tenant_id, table_uid, entity_kind, entity_id),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
        ON DELETE CASCADE
);

ALTER TABLE vala.entity_time_bounds ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.entity_time_bounds FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.entity_time_bounds
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE INDEX entity_time_bounds_kind_entity_idx
    ON vala.entity_time_bounds (data_tenant_id, table_uid, entity_kind, entity_id);

GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.olap_indexes, vala.entity_time_bounds
    TO wyrd_app;
