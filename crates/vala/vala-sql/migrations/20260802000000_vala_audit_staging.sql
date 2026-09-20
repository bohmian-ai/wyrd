-- Transactional audit staging.
-- vala.audit_chain_head: per-tenant hash-chain head, gapless seq counter, and
--                        the publication watermark staging is drained through.
-- vala.audit_staging:    append-only, hash-chained authorization decisions
--                        awaiting publication into vala.system.audit_log.
--
-- append_audit writes one immutable row in the transaction that made the
-- authorization decision, so the decision and its record share a fate.

-- 1. Per-tenant chain head. last_seq is the gapless high-water mark; head_hash
--    is the entry_hash of row `last_seq` (32 zero bytes before the first row);
--    published_seq is the watermark every staged row at or below has been
--    durably published into vala.system.audit_log and may be deleted through;
--    publishing_seq_hi is the single frozen upper bound of the one batch
--    currently owed to retained history, so every concurrent or restarted
--    publisher derives the same batch identity for `published_seq + 1
--    .. publishing_seq_hi` until that batch settles. Rows appended above the
--    frozen bound wait for the next batch.
CREATE TABLE vala.audit_chain_head (
    data_tenant_id    uuid   NOT NULL PRIMARY KEY REFERENCES platform.tenants(data_tenant_id),
    last_seq          bigint NOT NULL DEFAULT 0,
    published_seq     bigint NOT NULL DEFAULT 0,
    publishing_seq_hi bigint,
    head_hash         bytea  NOT NULL
                        DEFAULT '\x0000000000000000000000000000000000000000000000000000000000000000'
                        CHECK (octet_length(head_hash) = 32),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CHECK (published_seq >= 0 AND published_seq <= last_seq),
    CHECK (publishing_seq_hi IS NULL
           OR (publishing_seq_hi > published_seq AND publishing_seq_hi <= last_seq))
);

ALTER TABLE vala.audit_chain_head ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.audit_chain_head FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.audit_chain_head
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- 2. Append-only staged decisions. entry_hash = SHA256(prev_hash ‖ canonical(seq, event)),
--    computed in Rust — vala-sql owns the canonical encoding. seq is gapless per
--    tenant. Every column here is retained, so the retained row and its
--    predecessor reproduce the hash without reading staging.
CREATE TABLE vala.audit_staging (
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
    permission      text   NOT NULL,
    outcome         text   NOT NULL CHECK (outcome IN ('allowed', 'denied')),
    detail          text,
    -- The credential the decision was authenticated by. A principal holds
    -- several at once so rotation can overlap, and "which key did this" is the
    -- question an operator has after a leak. Nullable because not every
    -- decision has one behind it: a federated human presents an identity and
    -- the server's own internal decisions present nothing.
    credential_id   uuid,
    created_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (data_tenant_id, seq)
);

ALTER TABLE vala.audit_staging ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.audit_staging FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.audit_staging
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

GRANT SELECT, INSERT, UPDATE ON vala.audit_chain_head TO wyrd_app;
-- DELETE is the publisher draining rows already durable in
-- vala.system.audit_log. Rows are never mutated in place.
GRANT SELECT, INSERT, DELETE ON vala.audit_staging TO wyrd_app;

-- 3. Immutability enforcement: a staged row is never rewritten. Draining
--    removes a published row; it never edits one.
CREATE FUNCTION vala.audit_staging_immutable() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'vala.audit_staging rows are immutable: % is forbidden', TG_OP
        USING ERRCODE = 'P0001';
END;
$$;

CREATE TRIGGER audit_staging_immutable
    BEFORE UPDATE ON vala.audit_staging
    FOR EACH ROW EXECUTE FUNCTION vala.audit_staging_immutable();
