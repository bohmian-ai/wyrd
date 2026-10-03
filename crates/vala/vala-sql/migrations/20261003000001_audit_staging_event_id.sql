-- Make a retried audit write idempotent.
--
-- The audit outbox retries a tenant batch whose commit failed, including one
-- whose commit outcome is unknown because the connection broke after the
-- server committed. Each staged decision now carries the event id the outbox
-- assigned when it was staged, unique per tenant, so the append skips events
-- already staged instead of chaining them a second time.
--
-- Rows staged before this migration receive a random id once, then the
-- default is dropped so every new row must carry the id its writer assigned.
-- The chain hash does not cover the id, so existing rows are unaffected.
ALTER TABLE vala.audit_staging ADD COLUMN event_id uuid NOT NULL DEFAULT gen_random_uuid();
ALTER TABLE vala.audit_staging ALTER COLUMN event_id DROP DEFAULT;
ALTER TABLE vala.audit_staging
    ADD CONSTRAINT audit_staging_event_id_key UNIQUE (data_tenant_id, event_id);
