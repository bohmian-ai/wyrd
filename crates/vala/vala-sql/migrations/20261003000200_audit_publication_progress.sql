-- Separate audit publication progress from the append lock.
--
-- Every append for a tenant locks its `vala.audit_chain_head` row. The
-- publisher kept its watermark and frozen bound on that same row, so freezing
-- a range needed the lock appenders hold and failed under sustained appends.
-- Progress now lives in its own per-tenant row that appenders never touch:
-- publishers serialize on it among themselves and never wait on an append.
--
-- published_seq is the watermark every staged row at or below has been durably
-- published into vala.system.audit_log and may be deleted through;
-- publishing_seq_hi is the single frozen upper bound of the one batch currently
-- owed to retained history, so every concurrent or restarted publisher derives
-- the same batch identity until that batch settles.
CREATE TABLE vala.audit_publication (
    data_tenant_id    uuid   NOT NULL PRIMARY KEY REFERENCES platform.tenants(data_tenant_id),
    published_seq     bigint NOT NULL DEFAULT 0,
    publishing_seq_hi bigint,
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CHECK (published_seq >= 0),
    CHECK (publishing_seq_hi IS NULL OR publishing_seq_hi > published_seq)
);

ALTER TABLE vala.audit_publication ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.audit_publication FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.audit_publication
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON vala.audit_publication TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

INSERT INTO vala.audit_publication (data_tenant_id, published_seq, publishing_seq_hi)
SELECT data_tenant_id, published_seq, publishing_seq_hi
  FROM vala.audit_chain_head;

ALTER TABLE vala.audit_chain_head
    DROP COLUMN publishing_seq_hi,
    DROP COLUMN published_seq;
