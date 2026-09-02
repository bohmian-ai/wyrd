-- Durable Oracle reader authority: table serialization, epoch leases, and the
-- per-table protection frontier each fenced Oracle epoch publishes.
--
-- Forge maintenance runs in a different process from every reader, so an
-- admitted query has to leave a durable trace or destructive maintenance would
-- be deciding from age and liveness alone. Four owners carry that trace:
--
--   * bifrost_table_maintenance_authority is the single SQL serialization
--     boundary for one tenant-qualified table. Reader-protection expansion,
--     narrowing, release, and snapshot-expiration preparation all take this one
--     row FOR UPDATE, so the admission-versus-destruction race has exactly one
--     durable winner per table without a global physical lock.
--   * oracle_reader_epochs holds one bounded renewable lease per fenced Oracle
--     epoch. Postgres statement_timestamp() is the only lease clock, so lease
--     expiry is a database fact rather than a comparison of two local wall
--     clocks.
--   * oracle_table_protections and oracle_table_protection_members hold one
--     epoch's conservative frontier for one table: a set of retained-head ->
--     protected-snapshot chains with the exact Iceberg ancestry path proving
--     each chain, so maintenance never infers ancestry from snapshot-ID order.
--
-- A protection header is Forge protection regardless of heartbeat freshness or
-- epoch state. Only the invalidation-and-release sequence removes one.

-- ---------------------------------------------------------------------------
-- Per-table serialization boundary
-- ---------------------------------------------------------------------------

CREATE TABLE vala.bifrost_table_maintenance_authority (
    data_tenant_id   uuid  NOT NULL REFERENCES platform.tenants(data_tenant_id),
    catalog_name     text  NOT NULL CHECK (catalog_name = 'wyrd-redux'),
    namespace_name   text  NOT NULL CHECK (btrim(namespace_name) <> ''),
    table_name       text  NOT NULL CHECK (btrim(table_name) <> ''),
    table_uid        bytea NOT NULL CHECK (octet_length(table_uid) = 16),
    PRIMARY KEY (data_tenant_id, catalog_name, namespace_name, table_name),
    UNIQUE (data_tenant_id, table_uid),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_tables(data_tenant_id, table_uid)
);

-- Exactly one authority row per already-registered Bifrost table. The fqn is
-- `<namespace>.<table>` with the namespace itself dotted, so the table name is
-- the final segment and the namespace is everything before it. Table
-- registration inserts its own row from now on, which is why no caller ever
-- needs an advisory-lock fallback for a missing row.
INSERT INTO vala.bifrost_table_maintenance_authority
    (data_tenant_id, catalog_name, namespace_name, table_name, table_uid)
SELECT data_tenant_id,
       'wyrd-redux',
       left(fqn, length(fqn) - strpos(reverse(fqn), '.')),
       right(fqn, strpos(reverse(fqn), '.') - 1),
       table_uid
  FROM vala.bifrost_tables
 WHERE strpos(reverse(fqn), '.') > 1;

ALTER TABLE vala.bifrost_table_maintenance_authority ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.bifrost_table_maintenance_authority FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.bifrost_table_maintenance_authority
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.bifrost_table_maintenance_authority FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.bifrost_table_maintenance_authority TO wyrd_app;

-- ---------------------------------------------------------------------------
-- Oracle epoch leases
-- ---------------------------------------------------------------------------

-- The epoch owner is the system tenant, not the tenant whose data is being
-- read: one epoch serves every tenant this Oracle process admits, and its lease
-- is a process fact. Tenant-qualified protection lives in the two tables below.
CREATE TABLE vala.oracle_reader_epochs (
    epoch_owner_tenant_id uuid NOT NULL
        REFERENCES platform.tenants(data_tenant_id)
        CHECK (epoch_owner_tenant_id = '00000000-0000-0000-0000-000000000000'::uuid),
    node_id           uuid   NOT NULL,
    fencing_token     bigint NOT NULL CHECK (fencing_token > 0),
    state             text   NOT NULL
        CHECK (state IN ('acquired', 'active', 'draining', 'invalidated')),
    state_revision    bigint NOT NULL CHECK (state_revision >= 1),
    acquired_at       timestamptz NOT NULL,
    activated_at      timestamptz,
    renewed_at        timestamptz NOT NULL,
    lease_expires_at  timestamptz NOT NULL,
    invalidated_at    timestamptz,
    CHECK (lease_expires_at > acquired_at),
    -- An epoch is 'acquired' until activation succeeds, so it has no
    -- activation instant; an activation failure invalidates without ever
    -- gaining one, which is why only 'active' and 'draining' require it.
    CHECK (state <> 'acquired' OR activated_at IS NULL),
    CHECK (state NOT IN ('active', 'draining') OR activated_at IS NOT NULL),
    CHECK ((state = 'invalidated') = (invalidated_at IS NOT NULL)),
    PRIMARY KEY (epoch_owner_tenant_id, node_id, fencing_token)
);

CREATE INDEX oracle_reader_epochs_expiry
    ON vala.oracle_reader_epochs (lease_expires_at);

ALTER TABLE vala.oracle_reader_epochs ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.oracle_reader_epochs FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.oracle_reader_epochs
    USING      (epoch_owner_tenant_id = wyrd.current_tenant())
    WITH CHECK (epoch_owner_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.oracle_reader_epochs FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.oracle_reader_epochs TO wyrd_app;

-- ---------------------------------------------------------------------------
-- Per-epoch, per-table protection frontier
-- ---------------------------------------------------------------------------

-- table_uid, not the reconstructed namespace string, is durable table identity.
-- The checked catalog/namespace/table payload exists so drift between the
-- registry and a protection header is visible rather than silent.
CREATE TABLE vala.oracle_table_protections (
    data_tenant_id            uuid   NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid                 bytea  NOT NULL CHECK (octet_length(table_uid) = 16),
    node_id                   uuid   NOT NULL,
    fencing_token             bigint NOT NULL CHECK (fencing_token > 0),
    catalog_name              text   NOT NULL CHECK (catalog_name = 'wyrd-redux'),
    namespace_name            text   NOT NULL CHECK (btrim(namespace_name) <> ''),
    table_name                text   NOT NULL CHECK (btrim(table_name) <> ''),
    revision                  bigint NOT NULL CHECK (revision >= 1),
    frontier_encoding_version integer NOT NULL CHECK (frontier_encoding_version = 1),
    frontier_digest           bytea  NOT NULL CHECK (octet_length(frontier_digest) = 32),
    updated_at                timestamptz NOT NULL,
    PRIMARY KEY (data_tenant_id, table_uid, node_id, fencing_token),
    FOREIGN KEY (data_tenant_id, table_uid)
        REFERENCES vala.bifrost_table_maintenance_authority(data_tenant_id, table_uid)
);

CREATE INDEX oracle_table_protections_epoch
    ON vala.oracle_table_protections (node_id, fencing_token);

ALTER TABLE vala.oracle_table_protections ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.oracle_table_protections FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.oracle_table_protections
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.oracle_table_protections FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE ON vala.oracle_table_protections TO wyrd_app;

-- One member is one comparable Iceberg chain: the newest active local cut is
-- the retained head, the oldest is the protected snapshot, and ancestry_path is
-- the inclusive newest-to-oldest parent walk that proves they are on the same
-- chain. Incomparable active cuts get separate members; a singleton chain has
-- the same head and protected id and a one-element path.
CREATE TABLE vala.oracle_table_protection_members (
    data_tenant_id                 uuid   NOT NULL REFERENCES platform.tenants(data_tenant_id),
    table_uid                      bytea  NOT NULL CHECK (octet_length(table_uid) = 16),
    node_id                        uuid   NOT NULL,
    fencing_token                  bigint NOT NULL CHECK (fencing_token > 0),
    protected_snapshot_id          bigint NOT NULL,
    protected_snapshot_timestamp_ms bigint NOT NULL CHECK (protected_snapshot_timestamp_ms >= 0),
    retained_head_snapshot_id      bigint NOT NULL,
    retained_head_timestamp_ms     bigint NOT NULL CHECK (retained_head_timestamp_ms >= 0),
    ancestry_path                  bigint[] NOT NULL CHECK (array_length(ancestry_path, 1) >= 1),
    ancestry_digest_version        integer NOT NULL CHECK (ancestry_digest_version = 1),
    ancestry_digest                bytea  NOT NULL CHECK (octet_length(ancestry_digest) = 32),
    CHECK (retained_head_timestamp_ms >= protected_snapshot_timestamp_ms),
    CHECK (ancestry_path[1] = retained_head_snapshot_id),
    CHECK (ancestry_path[array_length(ancestry_path, 1)] = protected_snapshot_id),
    PRIMARY KEY (data_tenant_id, table_uid, node_id, fencing_token, protected_snapshot_id),
    FOREIGN KEY (data_tenant_id, table_uid, node_id, fencing_token)
        REFERENCES vala.oracle_table_protections
                   (data_tenant_id, table_uid, node_id, fencing_token)
        ON DELETE CASCADE
);

ALTER TABLE vala.oracle_table_protection_members ENABLE ROW LEVEL SECURITY;
ALTER TABLE vala.oracle_table_protection_members FORCE  ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON vala.oracle_table_protection_members
    USING      (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

REVOKE ALL ON vala.oracle_table_protection_members FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE, DELETE
    ON vala.oracle_table_protection_members TO wyrd_app;

-- Read-only cross-tenant enumeration for crash/takeover recovery. Recovery may
-- learn which (tenant, table, node, fence) keys a dead epoch still protects; it
-- may not mutate tenant state or append tenant audit through this grant.
GRANT SELECT ON vala.oracle_table_protections TO wyrd_platform_admin;
GRANT SELECT ON vala.oracle_reader_epochs TO wyrd_platform_admin;

-- ---------------------------------------------------------------------------
-- Retirement proof
-- ---------------------------------------------------------------------------

-- Retiring an epoch requires proof that no protection header remains for it,
-- but that proof spans every tenant while the retiring transaction is bound to
-- the system owner under forced RLS — so a plain count inside that transaction
-- would always see zero and retire an epoch that is still protecting tables.
-- This execute-only function is exactly that proof: read-only, cross-tenant,
-- returning one number and no row payload, and taking no tenant input.
CREATE FUNCTION vala.oracle_epoch_protection_count(
    p_node_id uuid,
    p_fencing_token bigint
) RETURNS bigint
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = ''
AS $$
    SELECT pg_catalog.count(*)
      FROM vala.oracle_table_protections
     WHERE node_id = p_node_id AND fencing_token = p_fencing_token
$$;

ALTER FUNCTION vala.oracle_epoch_protection_count(uuid, bigint) OWNER TO wyrd_migrator;
REVOKE ALL ON FUNCTION vala.oracle_epoch_protection_count(uuid, bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.oracle_epoch_protection_count(uuid, bigint) TO wyrd_app;
GRANT EXECUTE ON FUNCTION vala.oracle_epoch_protection_count(uuid, bigint) TO wyrd_platform_admin;
