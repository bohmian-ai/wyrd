-- A verification's target in one round trip.
--
-- Before any Verifier runs, direct or queued, the server needs the Verifier
-- Card, its wyrd.verifier_readiness, the subject Card, and the tenant's
-- SYSTEM principal. This function returns all of them from one statement, so
-- a direct verify needs no transaction around it, as
-- vala.oracle_read_table_cut does for Oracle cuts, and submission acceptance
-- and the queued runner read the same answer inside their own transactions.
--
-- It binds p_data_tenant_id as the transaction-local RLS tenant itself, so a
-- caller runs it with no BEGIN or COMMIT and the binding ends with the
-- statement's implicit transaction. Inside a transaction that already bound a
-- different tenant it refuses rather than rebinding. It runs with the
-- caller's privileges, so RLS bounds every row, and it writes nothing.
--
-- Result: one row per non-deleted Card among the two UIDs (none, one, or two;
-- one when both UIDs name the same Card), each carrying the Verifier's
-- readiness and the tenant's active SYSTEM principal, or NULL when the tenant
-- has none.

CREATE FUNCTION wyrd.verification_target(
    p_data_tenant_id uuid,
    p_verifier_card_uid uuid,
    p_subject_card_uid uuid
) RETURNS TABLE (
    card_uid uuid,
    data_tenant_id uuid,
    kind text,
    space text,
    name text,
    version text,
    spec jsonb,
    spec_hash text,
    artifact_hash text,
    labels jsonb,
    annotations jsonb,
    status text,
    sub_principal_id uuid,
    act_principal_id uuid,
    created_at timestamptz,
    updated_at timestamptz,
    card_blob_uri text,
    readiness text,
    system_principal_id uuid
)
LANGUAGE plpgsql
STABLE
SECURITY INVOKER
SET search_path = ''
AS $$
BEGIN
    IF pg_catalog.current_setting('app.current_tenant', true)
           NOT IN ('', p_data_tenant_id::text) THEN
        RAISE EXCEPTION 'verification target tenant differs from the bound tenant'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    PERFORM pg_catalog.set_config('app.current_tenant', p_data_tenant_id::text, true);

    RETURN QUERY
    SELECT c.card_uid, c.data_tenant_id, c.kind, c.space, c.name, c.version,
           c.spec, c.spec_hash, c.artifact_hash, c.labels, c.annotations,
           c.status, c.sub_principal_id, c.act_principal_id, c.created_at,
           c.updated_at, c.card_blob_uri,
           wyrd.verifier_readiness(p_verifier_card_uid),
           (SELECT a.id
              FROM wyrd.auth_service_accounts a
             WHERE a.data_tenant_id = p_data_tenant_id
               AND a.principal_kind = 'system'
               AND a.status = 'active'
             LIMIT 1)
      FROM wyrd.cards c
     WHERE c.data_tenant_id = p_data_tenant_id
       AND c.card_uid IN (p_verifier_card_uid, p_subject_card_uid)
       AND c.status != 'deleted';
END;
$$;

-- Verification target invalidation.
--
-- Servers cache resolved targets and drop them when this channel names what
-- changed. A Card's spec, version, and name never change under its UID, so
-- only a status, version, or spec hash change or a delete notifies, with
-- payload '<data_tenant_id>|<card_uid>'; a drift baseline's state change
-- notifies the same way for its Verifier, since readiness reads it. Any write
-- to a SYSTEM principal notifies with payload '<data_tenant_id>'. NOTIFY is transactional, so a
-- listener hears it only after the write commits, and identical payloads in
-- one transaction are delivered once.

CREATE FUNCTION wyrd.notify_verification_target_card() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    PERFORM pg_catalog.pg_notify(
        'wyrd_verification_target',
        OLD.data_tenant_id::text || '|' || OLD.card_uid::text);
    RETURN NULL;
END
$$;

CREATE TRIGGER cards_notify_verification_target_update
    AFTER UPDATE ON wyrd.cards
    FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status
          OR OLD.version IS DISTINCT FROM NEW.version
          OR OLD.spec_hash IS DISTINCT FROM NEW.spec_hash)
    EXECUTE FUNCTION wyrd.notify_verification_target_card();

CREATE TRIGGER cards_notify_verification_target_delete
    AFTER DELETE ON wyrd.cards
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_verification_target_card();

CREATE FUNCTION wyrd.notify_verification_target_principal() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    IF TG_OP <> 'INSERT' AND OLD.principal_kind = 'system' THEN
        PERFORM pg_catalog.pg_notify('wyrd_verification_target', OLD.data_tenant_id::text);
    END IF;
    IF TG_OP <> 'DELETE' AND NEW.principal_kind = 'system' THEN
        PERFORM pg_catalog.pg_notify('wyrd_verification_target', NEW.data_tenant_id::text);
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER auth_service_accounts_notify_verification_target
    AFTER INSERT OR UPDATE OR DELETE ON wyrd.auth_service_accounts
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_verification_target_principal();

CREATE FUNCTION wyrd.notify_verification_target_baseline() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM pg_catalog.pg_notify(
            'wyrd_verification_target',
            OLD.data_tenant_id::text || '|' || OLD.verifier_uid::text);
    ELSE
        PERFORM pg_catalog.pg_notify(
            'wyrd_verification_target',
            NEW.data_tenant_id::text || '|' || NEW.verifier_uid::text);
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER drift_baselines_notify_verification_target
    AFTER INSERT OR DELETE OR UPDATE OF state ON wyrd.drift_baselines
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_verification_target_baseline();
