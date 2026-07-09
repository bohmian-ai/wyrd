-- Append-only audit ledger for every wyrd.cards write.
-- Mirrors the F-12 transactional-audit pattern from
-- 20260601000001_auth.sql (audit_credential_issuance / audit_token_exchange).
-- Defense-in-depth: RLS denies UPDATE/DELETE by default, AND a
-- BEFORE UPDATE/DELETE trigger raises 42501 unconditionally.

CREATE TABLE wyrd.audit_card_registration (
    audit_id              UUID        PRIMARY KEY,
    data_tenant_id        UUID        NOT NULL,
    card_uid              UUID        NOT NULL,
    kind                  TEXT        NOT NULL,
    operation             TEXT        NOT NULL,
    outcome               TEXT,
    actor_principal_id    UUID        NOT NULL,
    actor_kind            TEXT        NOT NULL,
    before_spec_hash      TEXT,
    after_spec_hash       TEXT,
    request_id            TEXT,
    occurred_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT audit_card_registration_operation_check CHECK (
        operation IN ('register','update','delete')
    ),
    CONSTRAINT audit_card_registration_outcome_check CHECK (
        outcome IS NULL OR outcome IN ('created','idempotent_noop','deduplicated')
    ),
    CONSTRAINT audit_card_registration_outcome_operation_check CHECK (
        (operation = 'register' AND outcome IS NOT NULL)
        OR (operation <> 'register' AND outcome IS NULL)
    ),
    CONSTRAINT audit_card_registration_actor_kind_check CHECK (
        actor_kind IN ('user','service','agent')
    ),
    CONSTRAINT audit_card_registration_kind_check CHECK (
        kind IN (
            'Data','Model','Experiment','Prompt','Agent','Workflow','Eval','Drift',
            'Service','Policy','Mcp','Audit','Artifact','Trigger','Operator','Source','External'
        )
    ),
    CONSTRAINT audit_card_registration_op_hash_consistency CHECK (
        (operation = 'register' AND before_spec_hash IS NULL     AND after_spec_hash IS NOT NULL)
        OR (operation = 'update'   AND before_spec_hash IS NOT NULL AND after_spec_hash IS NOT NULL)
        OR (operation = 'delete'   AND before_spec_hash IS NOT NULL AND after_spec_hash IS NULL)
    )
);

CREATE INDEX idx_audit_card_registration_card_uid
    ON wyrd.audit_card_registration (data_tenant_id, card_uid, occurred_at DESC);

CREATE INDEX idx_audit_card_registration_actor
    ON wyrd.audit_card_registration (data_tenant_id, actor_principal_id, occurred_at DESC);

CREATE INDEX idx_audit_card_registration_kind_time
    ON wyrd.audit_card_registration (data_tenant_id, kind, occurred_at DESC);

CREATE INDEX idx_audit_card_registration_request_id
    ON wyrd.audit_card_registration (data_tenant_id, request_id)
    WHERE request_id IS NOT NULL;

ALTER TABLE wyrd.audit_card_registration ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.audit_card_registration FORCE  ROW LEVEL SECURITY;

-- Unified tenant_isolation policy governs SELECT and INSERT; UPDATE/DELETE
-- are blocked unconditionally by triggers below.
CREATE POLICY tenant_isolation ON wyrd.audit_card_registration
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE POLICY admin_cross_tenant ON wyrd.audit_card_registration
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);

CREATE OR REPLACE FUNCTION wyrd.audit_card_registration_block_mutations()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION USING
        MESSAGE    = 'wyrd.audit_card_registration is append-only',
        ERRCODE    = '42501',
        CONSTRAINT = 'audit_card_registration_append_only';
END;
$$;

CREATE TRIGGER audit_card_registration_block_update
    BEFORE UPDATE ON wyrd.audit_card_registration
    FOR EACH ROW
    EXECUTE FUNCTION wyrd.audit_card_registration_block_mutations();

CREATE TRIGGER audit_card_registration_block_delete
    BEFORE DELETE ON wyrd.audit_card_registration
    FOR EACH ROW
    EXECUTE FUNCTION wyrd.audit_card_registration_block_mutations();

REVOKE ALL ON TABLE wyrd.audit_card_registration FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.audit_card_registration FROM wyrd_platform_admin;
GRANT  SELECT, INSERT ON wyrd.audit_card_registration TO wyrd_app;
GRANT  SELECT          ON wyrd.audit_card_registration TO wyrd_platform_admin;
