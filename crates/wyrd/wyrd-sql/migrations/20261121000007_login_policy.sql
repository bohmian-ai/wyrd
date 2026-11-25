-- Tenant login policy (authorization-model spec revision 3, "Persistence and
-- authority changes": new tenant login-policy metadata, the
-- require-company-SSO state and its revision).
--
-- A tenant without a row requires nothing. While `require_company_sso` holds,
-- company SSO is the tenant's only human sign-in method, whether or not an
-- Active connection exists at the moment: there is no password or social
-- fallback. `revision` advances with every policy change.
CREATE TABLE wyrd.auth_login_policies (
    data_tenant_id       UUID        PRIMARY KEY REFERENCES platform.tenants(data_tenant_id),
    require_company_sso  BOOLEAN     NOT NULL DEFAULT FALSE,
    revision             BIGINT      NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT statement_timestamp()
);

ALTER TABLE wyrd.auth_login_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.auth_login_policies FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON wyrd.auth_login_policies
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());
CREATE POLICY operator_access ON wyrd.auth_login_policies TO CURRENT_USER
    USING (wyrd.operator_session()) WITH CHECK (wyrd.operator_session());
