-- Per-principal revocation epoch. The token verifier rejects any access JWT
-- whose `iat` predates this timestamp, giving instant revocation without a
-- per-jti deny-list. NULL means the principal has never been revoked.
--
-- Lookup is always by primary key (data_tenant_id, id); no additional index
-- is needed. Existing RLS policies on both tables cover the new column.

ALTER TABLE wyrd.auth_users
    ADD COLUMN tokens_not_before TIMESTAMPTZ;

ALTER TABLE wyrd.auth_service_accounts
    ADD COLUMN tokens_not_before TIMESTAMPTZ;
