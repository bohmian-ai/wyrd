-- Stage 3 transactional audit outbox (S3.C5).
-- vala.audit_chain_head: per-tenant hash-chain head + gapless seq counter.
-- vala.audit_outbox:     append-only, hash-chained audit rows awaiting relay.
--
-- append_audit writes one row in the audited operation's own transaction; a
-- background relay ships unshipped rows into the Iceberg vala.system.audit_log
-- table and marks them shipped. Rows are immutable except the one-way shipped
-- flip (enforced by the append-only trigger). The cross-tenant relay claim is a
-- SECURITY DEFINER routine owned by vala_audit_relay (BYPASSRLS).
-- NEVER creates roles here — only asserts existence and GRANTs to existing roles.

-- 1. Per-tenant chain head. last_seq is the gapless high-water mark; head_hash
--    is the entry_hash of row `last_seq` (32 zero bytes before the first row).
CREATE TABLE vala.audit_chain_head (
    data_tenant_id  uuid   NOT NULL PRIMARY KEY REFERENCES platform.tenants(data_tenant_id),
    last_seq        bigint NOT NULL DEFAULT 0,
    head_hash       bytea  NOT NULL
                        DEFAULT '\x0000000000000000000000000000000000000000000000000000000000000000'
                        CHECK (octet_length(head_hash) = 32),
    updated_at      timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE vala.audit_chain_head ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.audit_chain_head FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.audit_chain_head
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- 2. Append-only audit rows. entry_hash = SHA256(prev_hash ‖ canonical(seq, event)),
--    computed in Rust — vala-sql owns the canonical encoding. seq is gapless per
--    tenant. Content columns are immutable; only the shipped flip is permitted.
CREATE TABLE vala.audit_outbox (
    data_tenant_id  uuid   NOT NULL REFERENCES platform.tenants(data_tenant_id),
    seq             bigint NOT NULL,
    prev_hash       bytea  NOT NULL CHECK (octet_length(prev_hash)  = 32),
    entry_hash      bytea  NOT NULL CHECK (octet_length(entry_hash) = 32),
    request_id      text   NOT NULL,
    trace_id        text,
    operation       text   NOT NULL,
    resource        text   NOT NULL,
    card_ref        text,
    principal_id    uuid   NOT NULL,
    principal_kind  text   NOT NULL CHECK (principal_kind IN ('user', 'service', 'agent')),
    auth_method     text   NOT NULL CHECK (auth_method IN ('jwt', 'internal')),
    permission      text   NOT NULL,
    decision        text   NOT NULL CHECK (decision IN ('allow', 'deny')),
    result          text   NOT NULL CHECK (result IN ('success', 'failure')),
    payload_summary text   NOT NULL,
    detail          text,
    shipped         boolean NOT NULL DEFAULT false,
    ship_batch_id   bytea  CHECK (ship_batch_id IS NULL OR octet_length(ship_batch_id) = 16),
    created_at      timestamptz NOT NULL DEFAULT now(),
    shipped_at      timestamptz,
    PRIMARY KEY (data_tenant_id, seq)
);

ALTER TABLE vala.audit_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.audit_outbox FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.audit_outbox
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- Relay claim scans unshipped rows across tenants ordered by (tenant, seq).
CREATE INDEX audit_outbox_unshipped_idx
    ON vala.audit_outbox (data_tenant_id, seq)
    WHERE NOT shipped;

GRANT SELECT, INSERT, UPDATE ON vala.audit_chain_head TO wyrd_app;
GRANT SELECT, INSERT, UPDATE ON vala.audit_outbox      TO wyrd_app;

-- 3. Append-only enforcement. DELETE is always forbidden; UPDATE may only flip a
--    still-unshipped row to shipped (stamping ship_batch_id / shipped_at). Any
--    content mutation, or an update to an already-shipped row, aborts.
CREATE FUNCTION vala.audit_outbox_append_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'vala.audit_outbox is append-only: DELETE is forbidden'
            USING ERRCODE = 'P0001';
    END IF;

    IF OLD.shipped THEN
        RAISE EXCEPTION 'vala.audit_outbox row (tenant %, seq %) already shipped: immutable',
            OLD.data_tenant_id, OLD.seq
            USING ERRCODE = 'P0001';
    END IF;

    IF NEW.data_tenant_id  IS DISTINCT FROM OLD.data_tenant_id
       OR NEW.seq             IS DISTINCT FROM OLD.seq
       OR NEW.prev_hash       IS DISTINCT FROM OLD.prev_hash
       OR NEW.entry_hash      IS DISTINCT FROM OLD.entry_hash
       OR NEW.request_id      IS DISTINCT FROM OLD.request_id
       OR NEW.trace_id        IS DISTINCT FROM OLD.trace_id
       OR NEW.operation       IS DISTINCT FROM OLD.operation
       OR NEW.resource        IS DISTINCT FROM OLD.resource
       OR NEW.card_ref        IS DISTINCT FROM OLD.card_ref
       OR NEW.principal_id    IS DISTINCT FROM OLD.principal_id
       OR NEW.principal_kind  IS DISTINCT FROM OLD.principal_kind
       OR NEW.auth_method     IS DISTINCT FROM OLD.auth_method
       OR NEW.permission      IS DISTINCT FROM OLD.permission
       OR NEW.decision        IS DISTINCT FROM OLD.decision
       OR NEW.result          IS DISTINCT FROM OLD.result
       OR NEW.payload_summary IS DISTINCT FROM OLD.payload_summary
       OR NEW.detail          IS DISTINCT FROM OLD.detail
       OR NEW.created_at      IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'vala.audit_outbox content is immutable; only shipped may transition'
            USING ERRCODE = 'P0001';
    END IF;

    IF NOT NEW.shipped THEN
        RAISE EXCEPTION 'vala.audit_outbox UPDATE must set shipped = true'
            USING ERRCODE = 'P0001';
    END IF;

    RETURN NEW;
END;
$$;

CREATE TRIGGER audit_outbox_append_only
    BEFORE UPDATE OR DELETE ON vala.audit_outbox
    FOR EACH ROW EXECUTE FUNCTION vala.audit_outbox_append_only();

-- 4. Assert the relay role exists — roles.sql / db:setup-roles must provision it
--    before migrations run.
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'vala_audit_relay') THEN
    RAISE EXCEPTION 'vala_audit_relay missing — run bootstrap/roles.sql / db:setup-roles';
  END IF;
END $$;

-- 5. Grant schema + table access to vala_audit_relay so the SECURITY DEFINER
--    claim routine executes under that role's privileges. USAGE on the schema is
--    required in addition to BYPASSRLS (which skips RLS, not privilege checks);
--    SELECT+UPDATE is required because SELECT ... FOR UPDATE takes row locks.
--    CREATE ON SCHEMA vala is pre-committed by vala_sql::migrate() so PostgreSQL 17
--    permits ALTER FUNCTION OWNER TO vala_audit_relay; it is revoked at the end.
GRANT USAGE ON SCHEMA vala TO vala_audit_relay;
GRANT SELECT, UPDATE ON vala.audit_outbox TO vala_audit_relay;

-- 6. Cross-tenant relay claim. Owned by vala_audit_relay (BYPASSRLS) so it reads
--    unshipped rows across every tenant regardless of the caller's tenant bind.
--    FOR UPDATE SKIP LOCKED keeps concurrent relay passes from grabbing the same
--    rows. The caller ships one RecordBatch per tenant, then marks the range
--    shipped under that tenant's RLS bind via mark_audit_shipped.
CREATE OR REPLACE FUNCTION vala.claim_unshipped_audit(p_limit int)
RETURNS TABLE(
    data_tenant_id  uuid,
    seq             bigint,
    entry_hash      bytea,
    prev_hash       bytea,
    request_id      text,
    trace_id        text,
    operation       text,
    resource        text,
    card_ref        text,
    principal_id    uuid,
    principal_kind  text,
    auth_method     text,
    permission      text,
    decision        text,
    result          text,
    payload_summary text,
    detail          text,
    created_at      timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala, platform
AS $$
BEGIN
    RETURN QUERY
    SELECT o.data_tenant_id, o.seq, o.entry_hash, o.prev_hash,
           o.request_id, o.trace_id, o.operation, o.resource, o.card_ref,
           o.principal_id, o.principal_kind, o.auth_method, o.permission,
           o.decision, o.result, o.payload_summary, o.detail, o.created_at
      FROM vala.audit_outbox o
     WHERE NOT o.shipped
     ORDER BY o.data_tenant_id, o.seq
     LIMIT p_limit
    FOR UPDATE SKIP LOCKED;
END;
$$;
-- Grant EXECUTE while wyrd_migrator still owns the function (before owner transfer).
GRANT EXECUTE ON FUNCTION vala.claim_unshipped_audit(int) TO wyrd_app;
ALTER FUNCTION vala.claim_unshipped_audit(int) OWNER TO vala_audit_relay;

-- vala_audit_relay only needs CREATE ON SCHEMA vala for the ALTER FUNCTION OWNER
-- transfer above. Revoke it now that the transfer is done.
REVOKE CREATE ON SCHEMA vala FROM vala_audit_relay;
