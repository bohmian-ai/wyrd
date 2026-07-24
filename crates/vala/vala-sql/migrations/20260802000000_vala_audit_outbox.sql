-- Transactional audit outbox.
-- vala.audit_chain_head: per-tenant hash-chain head + gapless seq counter.
-- vala.audit_outbox:     append-only, hash-chained audit rows.
--
-- append_audit writes one immutable row in the audited operation's transaction.

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
--    tenant.
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
    created_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, seq)
);

ALTER TABLE vala.audit_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.audit_outbox FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.audit_outbox
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

GRANT SELECT, INSERT, UPDATE ON vala.audit_chain_head TO wyrd_app;
GRANT SELECT, INSERT ON vala.audit_outbox TO wyrd_app;

-- 3. Append-only enforcement.
CREATE FUNCTION vala.audit_outbox_append_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'vala.audit_outbox is append-only: % is forbidden', TG_OP
        USING ERRCODE = 'P0001';
END;
$$;

CREATE TRIGGER audit_outbox_append_only
    BEFORE UPDATE OR DELETE ON vala.audit_outbox
    FOR EACH ROW EXECUTE FUNCTION vala.audit_outbox_append_only();
