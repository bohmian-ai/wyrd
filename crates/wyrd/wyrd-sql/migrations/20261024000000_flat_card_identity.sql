-- Flat Card identity: (data_tenant_id, kind, name, version).
--
-- Card space is removed from the registry, its indexes, the principal Card
-- projection, and the verification target. This is a fresh-baseline change:
-- there is no collision mapping, so a database that holds one kind/name/version
-- in two spaces fails the new unique index and must be recreated.

DROP INDEX IF EXISTS wyrd.cards_identity_unique;
DROP INDEX IF EXISTS wyrd.idx_cards_version_latest;
DROP INDEX IF EXISTS wyrd.idx_cards_spec_hash;
DROP INDEX IF EXISTS wyrd.idx_cards_space;

ALTER TABLE wyrd.cards DROP COLUMN space;

CREATE UNIQUE INDEX cards_identity_unique
    ON wyrd.cards (data_tenant_id, kind, name, version)
    WHERE status NOT IN ('deleted', 'failed', 'expired');

CREATE INDEX idx_cards_version_latest
    ON wyrd.cards (
        data_tenant_id, kind, name,
        version_major DESC, version_minor DESC, version_patch DESC
    )
    WHERE status = 'active' AND NOT version_is_prerelease;

CREATE INDEX idx_cards_spec_hash
    ON wyrd.cards (data_tenant_id, kind, name, spec_hash)
    WHERE status = 'active';

ALTER TABLE wyrd.auth_service_accounts
    DROP CONSTRAINT auth_service_accounts_card_binding_check;

ALTER TABLE wyrd.auth_service_accounts DROP COLUMN space;

ALTER TABLE wyrd.auth_service_accounts
    ADD CONSTRAINT auth_service_accounts_card_binding_check
    CHECK (
        (card_kind IS NULL AND card_uid IS NULL AND card_ref IS NULL
            AND version IS NULL)
        OR (card_kind IS NOT NULL AND card_uid IS NOT NULL AND card_ref IS NOT NULL
            AND version IS NOT NULL)
    );

-- The result shape loses `space`, so the function is replaced rather than
-- redefined in place.
DROP FUNCTION wyrd.verification_target(uuid, uuid, uuid);

CREATE FUNCTION wyrd.verification_target(
    p_data_tenant_id uuid,
    p_verifier_card_uid uuid,
    p_subject_card_uid uuid
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
       AND c.card_uid IN (p_verifier_card_uid, p_subject_card_uid)
       AND c.status != 'deleted';
END;
$$;
