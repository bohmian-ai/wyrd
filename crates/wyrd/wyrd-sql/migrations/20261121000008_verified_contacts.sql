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
