-- C2: Persist ship_batch_id at claim time so crash recovery is idempotent.
--
-- Problem: claim_unshipped_audit returns rows without stamping ship_batch_id.
-- After a crash between ship() and mark_shipped(), any new row appended for the
-- same tenant shifts the seq range, mints a different batch_id, and re-ships
-- already-committed rows → duplicate rows in vala.system.audit_log.
--
-- Fix: the relay stamps ship_batch_id inside the claim transaction (via
-- stamp_audit_ship_batch_id). On recovery the existing ship_batch_id is reused.
-- The append-only trigger is relaxed to allow this stamp.

-- 1. Relax the append-only trigger to permit ship_batch_id stamp (NULL → value)
--    while shipped stays false. Content columns and already-shipped rows remain
--    fully immutable.
CREATE OR REPLACE FUNCTION vala.audit_outbox_append_only() RETURNS trigger
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
       OR NEW.created_at      IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'vala.audit_outbox content is immutable; only shipped/ship_batch_id may transition'
            USING ERRCODE = 'P0001';
    END IF;

    IF NOT NEW.shipped THEN
        -- The only permitted non-shipped UPDATE is stamping ship_batch_id (NULL → value).
        IF NEW.ship_batch_id IS NULL THEN
            RAISE EXCEPTION 'vala.audit_outbox UPDATE must set shipped=true or stamp ship_batch_id'
                USING ERRCODE = 'P0001';
        END IF;
        IF OLD.ship_batch_id IS NOT NULL THEN
            RAISE EXCEPTION 'vala.audit_outbox ship_batch_id is immutable once stamped (tenant %, seq %)',
                OLD.data_tenant_id, OLD.seq
                USING ERRCODE = 'P0001';
        END IF;
    END IF;

    RETURN NEW;
END;
$$;

-- 2. Recreate claim_unshipped_audit to also return ship_batch_id so the relay
--    can detect rows that are already stamped (crash-recovery path) and reuse
--    their batch_id without re-computing it.
GRANT CREATE ON SCHEMA vala TO vala_audit_relay;

DROP FUNCTION vala.claim_unshipped_audit(int);

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
    created_at      timestamptz,
    ship_batch_id   bytea
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
           o.decision, o.result, o.payload_summary, o.created_at,
           o.ship_batch_id
      FROM vala.audit_outbox o
     WHERE NOT o.shipped
     ORDER BY o.data_tenant_id, o.seq
     LIMIT p_limit
    FOR UPDATE SKIP LOCKED;
END;
$$;
GRANT EXECUTE ON FUNCTION vala.claim_unshipped_audit(int) TO wyrd_app;
ALTER FUNCTION vala.claim_unshipped_audit(int) OWNER TO vala_audit_relay;

-- 3. New SECURITY DEFINER helper to stamp ship_batch_id on a claimed seq range
--    within the claim transaction. WHERE ship_batch_id IS NULL makes it a no-op
--    for already-stamped rows (idempotent on re-entry / partial re-apply).
CREATE OR REPLACE FUNCTION vala.stamp_audit_ship_batch_id(
    p_tenant_id  uuid,
    p_seq_lo     bigint,
    p_seq_hi     bigint,
    p_batch_id   bytea
) RETURNS bigint
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala, platform
AS $$
DECLARE
    updated bigint;
BEGIN
    UPDATE vala.audit_outbox
       SET ship_batch_id = p_batch_id
     WHERE data_tenant_id = p_tenant_id
       AND seq BETWEEN p_seq_lo AND p_seq_hi
       AND NOT shipped
       AND ship_batch_id IS NULL;
    GET DIAGNOSTICS updated = ROW_COUNT;
    RETURN updated;
END;
$$;
GRANT EXECUTE ON FUNCTION vala.stamp_audit_ship_batch_id(uuid, bigint, bigint, bytea)
    TO wyrd_app;
ALTER FUNCTION vala.stamp_audit_ship_batch_id(uuid, bigint, bigint, bytea)
    OWNER TO vala_audit_relay;

REVOKE CREATE ON SCHEMA vala FROM vala_audit_relay;
