-- audit_claim_include_detail — restore `detail` column in claim_unshipped_audit.
--
-- Migration 20260802000000 added the `detail text` column to vala.audit_outbox
-- (and to the RETURNS TABLE of vala.claim_unshipped_audit).
-- Migration 20260802000001 later did DROP+CREATE on claim_unshipped_audit to
-- stamp ship_batch_id in the claim transaction, and inadvertently reverted the
-- signature — the redefined function omits `detail` from RETURNS TABLE and from
-- the SELECT list.
--
-- The Rust row type `AuditOutboxRow` (sqlx::FromRow) requires `detail`, so any
-- caller of `claim_unshipped_audit` fails at row decode with
--   Sql(Query(ColumnNotFound("detail"))).
--
-- Fix: DROP+CREATE the function again, this time including `detail` in both
-- the RETURNS TABLE and the SELECT projection, preserving all other columns
-- and the `ship_batch_id` behaviour introduced by 20260802000001.

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
    detail          text,
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
           o.decision, o.result, o.payload_summary, o.detail, o.created_at,
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
