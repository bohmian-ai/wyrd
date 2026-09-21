-- Tenant access tokens are self-contained five-minute JWTs; revocation and
-- suspension refuse the next issuance and never compare a token's `iat`
-- against a per-principal epoch. Drop the epoch columns that design used.

ALTER TABLE wyrd.auth_users
    DROP COLUMN tokens_not_before;

ALTER TABLE wyrd.auth_service_accounts
    DROP COLUMN tokens_not_before;
