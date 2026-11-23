-- Tenant-local verified recovery contacts (authorization-model spec revision
-- 6, "Persistence and authority changes": tenant-local verified recovery
-- contact). One verified address per User, kept apart from the unverified
-- profile and invitation email; duplicate addresses are allowed within and
-- across tenants, so nothing here is unique on the address. `revision`
-- advances with every replacement.
CREATE TABLE wyrd.auth_user_contacts (
    data_tenant_id  UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    user_id         UUID        NOT NULL,
    address         TEXT        NOT NULL CHECK (address <> ''),
    verified_at     TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp(),
    revision        BIGINT      NOT NULL DEFAULT 1 CHECK (revision > 0),
    PRIMARY KEY (data_tenant_id, user_id),
    FOREIGN KEY (data_tenant_id, user_id)
        REFERENCES wyrd.auth_users(data_tenant_id, id) ON DELETE CASCADE
);

ALTER TABLE wyrd.auth_user_contacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_user_contacts FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_user_contacts
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_user_contacts TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- Workspace discovery (revision 6, "Workspace discovery directory"): the one
-- cross-tenant lookup, a minimal projection from the HMAC of a canonical
-- verified address to the tenant and User whose contact it is, at the
-- contact revision it was projected from. Disclosure revalidates every entry
-- through the tenant's own owners, so a stale entry reveals nothing. No
-- address, password, grant or session is held here.
CREATE TABLE platform.workspace_directory (
    data_tenant_id    UUID   NOT NULL REFERENCES platform.tenants(data_tenant_id) ON DELETE CASCADE,
    user_id           UUID   NOT NULL,
    address_key       BYTEA  NOT NULL CHECK (length(address_key) = 32),
    contact_revision  BIGINT NOT NULL CHECK (contact_revision > 0),
    PRIMARY KEY (data_tenant_id, user_id)
);

CREATE INDEX platform_workspace_directory_address
    ON platform.workspace_directory (address_key, data_tenant_id);

-- Spent discovery proofs ("Proven email signup/discovery/contact proof and
-- one-use redemption"): the proof digest, the browser binding that alone may
-- repeat it after a lost response, the deadline of the read it granted, and
-- the proof's own expiry, until which the digest is retained. A discovery
-- visitor is pre-tenant, so the table carries no tenant key, as "Genuine
-- pre-tenant platform records may omit `data_tenant_id`" permits; it forces
-- RLS and admits only the platform owner.
CREATE TABLE platform.discovery_proofs (
    proof_hash            BYTEA       PRIMARY KEY CHECK (length(proof_hash) = 32),
    browser_binding_hash  BYTEA       NOT NULL CHECK (length(browser_binding_hash) = 32),
    read_until            TIMESTAMPTZ NOT NULL,
    expires_at            TIMESTAMPTZ NOT NULL
);

CREATE INDEX platform_discovery_proofs_expires_at
    ON platform.discovery_proofs (expires_at);

ALTER TABLE platform.workspace_directory ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.workspace_directory FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.workspace_directory TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

ALTER TABLE platform.discovery_proofs ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform.discovery_proofs FORCE ROW LEVEL SECURITY;
CREATE POLICY operator_access ON platform.discovery_proofs TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());

-- Every committed contact change notifies the directory reconciliation with
-- payload '<data_tenant_id>:<user_id>'; NOTIFY is transactional, so it is
-- heard only after the change commits.
CREATE FUNCTION wyrd.notify_contact_change() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM pg_catalog.pg_notify('wyrd_contact', OLD.data_tenant_id::text || ':' || OLD.user_id::text);
    ELSE
        PERFORM pg_catalog.pg_notify('wyrd_contact', NEW.data_tenant_id::text || ':' || NEW.user_id::text);
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER auth_user_contacts_notify
    AFTER INSERT OR UPDATE OR DELETE ON wyrd.auth_user_contacts
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_contact_change();
