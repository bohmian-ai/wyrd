-- Forge bounded operation state: one row per (tenant, resource, family, operation_id).
--
-- Forge transitions append immutable evidence to vala.audit_outbox and
-- transactionally maintain this current-state projection. Bifrost has not been
-- deployed, so this first projection migration intentionally starts empty; new
-- Forge writers populate it from their first Prepared transition. Runtime code
-- never reduces audit history to reconstruct projection state.

CREATE TABLE vala.forge_operation_state (
    data_tenant_id    uuid        NOT NULL,
    resource          text        NOT NULL,
    family            text        NOT NULL CHECK (family IN (
        'staging_fold', 'iceberg_rewrite', 'snapshot_expire', 'orphan_gc'
    )),
    operation_id      uuid        NOT NULL,
    phase             text        NOT NULL CHECK (phase IN (
        'prepared', 'committed', 'recovered', 'reset'
    )),
    prepared_detail   jsonb       NOT NULL,
    current_detail    jsonb       NOT NULL,
    prepared_audit_seq bigint     NOT NULL,
    terminal_audit_seq bigint,
    prepared_at       timestamptz NOT NULL,
    updated_at        timestamptz NOT NULL,
    PRIMARY KEY (data_tenant_id, resource, family, operation_id),
    CHECK (
        (phase = 'prepared' AND terminal_audit_seq IS NULL)
        OR (phase <> 'prepared' AND terminal_audit_seq IS NOT NULL)
    )
);

ALTER TABLE vala.forge_operation_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.forge_operation_state FORCE  ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation
    ON vala.forge_operation_state
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE INDEX forge_operation_state_open
    ON vala.forge_operation_state (
        data_tenant_id, resource, family, prepared_at, operation_id
    )
    WHERE phase = 'prepared';

DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'wyrd_platform_admin') THEN
    RAISE EXCEPTION 'wyrd_platform_admin missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

GRANT SELECT, INSERT, UPDATE ON vala.forge_operation_state TO wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON vala.forge_operation_state TO wyrd_app;
