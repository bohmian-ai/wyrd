-- Tenant human OIDC login connections, and the move of human trust out of
-- wyrd.auth_trusted_issuers.
--
-- auth_human_connections is the only store the human login and callback read.
-- A tenant has at most one Active and at most one Candidate connection; both
-- are partial unique indexes, so the database refuses a second one no matter
-- which replica races. Removal never deletes a row: it wipes the sealed secret,
-- sets removed_at, and leaves the connection id as a non-login tombstone that
-- historical sessions and audit can still name.
--
-- client_secret_enc is a keyring envelope ("wsk1" || key_id || nonce ||
-- ciphertext) written under the deployment sealing keyring. jwks_uri is set
-- only by a successful candidate test, which is also the only writer of
-- tested_revision/tested_until; activation requires both to be current.
--
-- Upgrade preflight: every existing Human trusted issuer must map onto exactly
-- one tenant connection without loss or silent privilege. The migration aborts,
-- leaving the database unchanged, and names the tenant id and a repair step
-- (never an issuer secret) when a tenant has more than one Human issuer, a
-- Human issuer carries default roles, its audience differs from its client id,
-- it uses private_key_jwt, or a secret method stores no secret. Each surviving Human issuer becomes that tenant's
-- Active connection with its claim mapping, group role map, JWKS URI, and JWKS
-- TTL preserved. A Human issuer still referenced by a workload binding is kept
-- as a Workload issuer so the binding survives; every other Human issuer row is
-- removed. auth_trusted_issuers then holds workload trust only.

-- ---------------------------------------------------------------------------
-- Preflight
-- ---------------------------------------------------------------------------
DO $$
DECLARE
    offending RECORD;
BEGIN
    SELECT data_tenant_id, count(*) AS issuers
      INTO offending
      FROM wyrd.auth_trusted_issuers
     WHERE principal_kind = 'Human'
     GROUP BY data_tenant_id
    HAVING count(*) > 1
     ORDER BY data_tenant_id
     LIMIT 1;
    IF FOUND THEN
        RAISE EXCEPTION 'human connection preflight failed for tenant %: % Human trusted issuers exist and a tenant may have one human login connection. Repair: Delete all but the one Human trusted issuer this tenant should sign in through (wyrd auth trusted-issuer delete) with the previous release, then rerun the upgrade.', offending.data_tenant_id, offending.issuers;
    END IF;

    SELECT data_tenant_id
      INTO offending
      FROM wyrd.auth_trusted_issuers
     WHERE principal_kind = 'Human'
       AND default_roles <> '[]'::jsonb
     ORDER BY data_tenant_id
     LIMIT 1;
    IF FOUND THEN
        RAISE EXCEPTION 'human connection preflight failed for tenant %: the Human trusted issuer grants default roles, which tenant connections do not carry. Repair: Replace default_roles with explicit group_role_map entries (re-create the Human trusted issuer with empty default roles) with the previous release, then rerun the upgrade.', offending.data_tenant_id;
    END IF;

    SELECT data_tenant_id
      INTO offending
      FROM wyrd.auth_trusted_issuers
     WHERE principal_kind = 'Human'
       AND expected_audience <> client_id
     ORDER BY data_tenant_id
     LIMIT 1;
    IF FOUND THEN
        RAISE EXCEPTION 'human connection preflight failed for tenant %: the Human trusted issuer expects an audience different from its client id. Repair: Re-create the Human trusted issuer with expected_audience equal to client_id with the previous release, then rerun the upgrade.', offending.data_tenant_id;
    END IF;

    SELECT data_tenant_id
      INTO offending
      FROM wyrd.auth_trusted_issuers
     WHERE principal_kind = 'Human'
       AND client_auth NOT IN ('SecretBasic', 'SecretPost', 'Public')
     ORDER BY data_tenant_id
     LIMIT 1;
    IF FOUND THEN
        RAISE EXCEPTION 'human connection preflight failed for tenant %: the Human trusted issuer uses an unsupported client authentication method. Repair: Re-create the Human trusted issuer with SecretBasic, SecretPost, or Public client authentication with the previous release, then rerun the upgrade.', offending.data_tenant_id;
    END IF;

    SELECT data_tenant_id
      INTO offending
      FROM wyrd.auth_trusted_issuers
     WHERE principal_kind = 'Human'
       AND client_auth IN ('SecretBasic', 'SecretPost')
       AND client_secret_enc IS NULL
     ORDER BY data_tenant_id
     LIMIT 1;
    IF FOUND THEN
        RAISE EXCEPTION 'human connection preflight failed for tenant %: the Human trusted issuer uses a client secret method but stores no secret. Repair: Re-create the Human trusted issuer with its client secret (or Public client authentication) with the previous release, then rerun the upgrade.', offending.data_tenant_id;
    END IF;
END
$$;

-- ---------------------------------------------------------------------------
-- Tenant human connections
-- ---------------------------------------------------------------------------
CREATE TABLE wyrd.auth_human_connections (
    connection_id       UUID        PRIMARY KEY,
    data_tenant_id      UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    -- Monotonic per tenant across every connection it ever had, so a revision
    -- names exactly one staged configuration.
    revision            BIGINT      NOT NULL CHECK (revision >= 1),
    state               TEXT        NOT NULL
        CHECK (state IN ('Candidate', 'Active', 'Inactive')),
    issuer_url          TEXT        NOT NULL,
    -- The ID-token audience is always the client id; it is not stored twice.
    client_id           TEXT        NOT NULL,
    client_auth         TEXT        NOT NULL
        CHECK (client_auth IN ('SecretBasic', 'SecretPost', 'Public')),
    client_secret_enc   BYTEA,
    claim_mapping       JSONB       NOT NULL,
    group_role_map      JSONB       NOT NULL,
    jwks_ttl_secs       BIGINT      NOT NULL CHECK (jwks_ttl_secs > 0),
    jwks_uri            TEXT,
    tested_revision     BIGINT,
    tested_until        TIMESTAMPTZ,
    removed_at          TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- A live secret method always carries a secret and Public never does; a
    -- removed tombstone carries none.
    CHECK (
        (removed_at IS NULL AND (client_auth = 'Public') = (client_secret_enc IS NULL))
        OR (removed_at IS NOT NULL AND client_secret_enc IS NULL)
    ),
    CHECK (removed_at IS NULL OR state = 'Inactive'),
    CHECK (state <> 'Active' OR jwks_uri IS NOT NULL),
    CHECK ((tested_revision IS NULL) = (tested_until IS NULL))
);

CREATE UNIQUE INDEX auth_human_connections_one_active
    ON wyrd.auth_human_connections (data_tenant_id) WHERE state = 'Active';
CREATE UNIQUE INDEX auth_human_connections_one_candidate
    ON wyrd.auth_human_connections (data_tenant_id) WHERE state = 'Candidate';
CREATE INDEX auth_human_connections_tenant
    ON wyrd.auth_human_connections (data_tenant_id, revision);

ALTER TABLE wyrd.auth_human_connections ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_human_connections FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_human_connections
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- ---------------------------------------------------------------------------
-- Move human trust
-- ---------------------------------------------------------------------------
INSERT INTO wyrd.auth_human_connections (
    connection_id, data_tenant_id, revision, state, issuer_url, client_id,
    client_auth, client_secret_enc, claim_mapping, group_role_map,
    jwks_ttl_secs, jwks_uri, created_at, updated_at
)
SELECT gen_random_uuid(), data_tenant_id, 1, 'Active', issuer_url, client_id,
       client_auth,
       CASE WHEN client_auth = 'Public' THEN NULL ELSE client_secret_enc END,
       claim_mapping, group_role_map,
       GREATEST(jwks_ttl_secs, 1), jwks_uri, created_at, now()
  FROM wyrd.auth_trusted_issuers
 WHERE principal_kind = 'Human';

UPDATE wyrd.auth_trusted_issuers AS issuer
   SET principal_kind = 'Workload',
       updated_at = now()
 WHERE issuer.principal_kind = 'Human'
   AND EXISTS (
       SELECT 1
         FROM wyrd.auth_workload_bindings AS binding
        WHERE binding.data_tenant_id = issuer.data_tenant_id
          AND binding.issuer_url = issuer.issuer_url
   );

DELETE FROM wyrd.auth_trusted_issuers
 WHERE principal_kind = 'Human';

ALTER TABLE wyrd.auth_trusted_issuers
    ADD CONSTRAINT auth_trusted_issuers_workload_only
    CHECK (principal_kind = 'Workload');

-- ---------------------------------------------------------------------------
-- Session provenance
-- ---------------------------------------------------------------------------
-- A human session belongs to the exact connection revision its login began
-- through. Login state records it; the first refresh row copies it and every
-- rotation copies it forward. Issuing a session or a refresh successor takes
-- the tenant connection slot lock and requires that exact revision to still
-- be Active, so replacement, deactivation, and removal end old sessions on
-- every replica. The foreign key keeps a removed connection's tombstone for
-- as long as a session names it.
--
-- A human refresh row also names the OAuth client it was issued to (RFC 6749
-- §6): `wyrd-cli` rotates its token on every refresh, while `wyrd-ui`
-- presents the same token until its absolute expiry.
--
-- In-flight login state is transient (five minutes) and names no connection,
-- so it is discarded. Legacy refresh rows record no issuer provenance, so
-- every pre-existing human refresh family stays unbound and can no longer
-- renew; its user signs in again under the tenant's current connection.
DELETE FROM wyrd.auth_login_state;

ALTER TABLE wyrd.auth_login_state
    ADD COLUMN connection_id UUID NOT NULL,
    ADD COLUMN connection_revision BIGINT NOT NULL;

ALTER TABLE wyrd.auth_refresh_tokens
    ADD COLUMN human_connection_id UUID
        REFERENCES wyrd.auth_human_connections(connection_id),
    ADD COLUMN human_connection_revision BIGINT,
    ADD COLUMN client_id TEXT CHECK (client_id IN ('wyrd-ui', 'wyrd-cli')),
    ADD CONSTRAINT auth_refresh_tokens_human_connection_pair
        CHECK ((human_connection_id IS NULL) = (human_connection_revision IS NULL)),
    ADD CONSTRAINT auth_refresh_tokens_human_client
        CHECK ((human_connection_id IS NULL) = (client_id IS NULL)),
    ADD CONSTRAINT auth_refresh_tokens_human_connection_user
        CHECK (human_connection_id IS NULL OR principal_kind = 'user');
