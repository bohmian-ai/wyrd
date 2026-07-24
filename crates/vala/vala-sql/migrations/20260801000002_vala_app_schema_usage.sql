-- Grant wyrd_app USAGE ON SCHEMA vala (S3.C2perm).
--
-- The foundational migrations grant table-level DML on Vala control tables but
-- require schema USAGE before the application role can reach them.
-- Without schema USAGE every one of those DML grants is dead: any write through
-- the request-path wyrd_app pool fails with Postgres 42501 (permission denied
-- for schema vala). This forward-only migration closes that gap.
--
-- USAGE only: the RLS tenant_isolation policies and the existing per-table DML
-- grants still gate every row. Mirrors wyrd-sql platform.sql
-- (GRANT USAGE ON SCHEMA platform, wyrd TO wyrd_app). iceberg_catalog is
-- intentionally left untouched -- wyrd_app is deliberately REVOKEd there.

GRANT USAGE ON SCHEMA vala TO wyrd_app;
