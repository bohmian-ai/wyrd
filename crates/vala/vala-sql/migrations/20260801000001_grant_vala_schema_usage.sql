-- Allow the RLS-enforced request role to reach Vala tenant-scoped tables.
--
-- Table privileges were granted in the OLAP migrations, but PostgreSQL also
-- requires USAGE on the containing schema before a role can resolve those
-- tables. Do not mirror this for iceberg_catalog; that schema remains hidden
-- from wyrd_app because catalog metadata is tenant-free.

GRANT USAGE ON SCHEMA vala TO wyrd_app;
