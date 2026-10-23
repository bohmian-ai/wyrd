-- Configured deployment environment of accepted verification work.
--
-- A submitted execution freezes the effective deployment environment it was
-- accepted under: the invocation's configured value, the matching binding's
-- selector, or NULL for unscoped work. Workers and status reads use this
-- frozen value, never their own configuration.
ALTER TABLE wyrd.verifier_executions
    ADD COLUMN deployment_environment TEXT
        CHECK (deployment_environment IS NULL OR octet_length(deployment_environment) > 0);

-- The target read also returns the caller's owner Service Card, whose
-- bindings select the effective deployment environment of an explicit
-- verification. `p_owner_card_uid` is NULL for a rootless caller and for
-- reads that only need the Verifier and subject.
DROP FUNCTION wyrd.verification_target(uuid, uuid, uuid);

CREATE FUNCTION wyrd.verification_target(
    p_data_tenant_id uuid,
    p_verifier_card_uid uuid,
    p_subject_card_uid uuid,
    p_owner_card_uid uuid
) RETURNS TABLE (
    card_uid uuid,
    data_tenant_id uuid,
    kind text,
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
    SELECT c.card_uid, c.data_tenant_id, c.kind, c.name, c.version,
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
       AND c.card_uid IN (p_verifier_card_uid, p_subject_card_uid, p_owner_card_uid)
       AND c.status != 'deleted';
END;
$$;
