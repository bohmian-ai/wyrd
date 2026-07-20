-- Forge restart reconciliation reads one tenant/table resource at a time.
CREATE INDEX audit_outbox_resource_seq_idx
    ON vala.audit_outbox (data_tenant_id, resource, seq);
