-- Wyrd card registry — single universal table for all CardKind variants.
--
-- Durable identity: (data_tenant_id, kind, space, name, version).
-- card_uid is a stable handle, minted in Rust as UUIDv7 via Uuid::now_v7()
-- and passed as a bound parameter at every INSERT site. No DEFAULT
-- gen_random_uuid() is declared because gen_random_uuid() returns v4,
-- and wyrd_spec::ids::CardUid::from_uuid7 (added by this PR) validates
-- the version nibble at the constructor boundary. A v4 DEFAULT would
-- silently leak past the type system on any code path that omits the
-- column.

CREATE TABLE wyrd.cards (
    card_uid              UUID        PRIMARY KEY,
    data_tenant_id        UUID        NOT NULL REFERENCES platform.tenants(data_tenant_id),
    kind                  TEXT        NOT NULL,
    space                 TEXT        NOT NULL,
    name                  TEXT        NOT NULL,
    version               TEXT        NOT NULL,
    -- Sortable, DB-generated semver components. GENERATED ALWAYS ... STORED so
    -- they can never drift from `version`; there is no write path that sets them.
    -- The numeric triple always precedes any '-'/'+' in valid semver, so the
    -- extraction is exact. `version` is validated semver TEXT (wyrd-spec).
    version_major         BIGINT      GENERATED ALWAYS AS
        (split_part(split_part(split_part(version, '+', 1), '-', 1), '.', 1)::bigint) STORED,
    version_minor         BIGINT      GENERATED ALWAYS AS
        (split_part(split_part(split_part(version, '+', 1), '-', 1), '.', 2)::bigint) STORED,
    version_patch         BIGINT      GENERATED ALWAYS AS
        (split_part(split_part(split_part(version, '+', 1), '-', 1), '.', 3)::bigint) STORED,
    -- Stable-release filter. TRUE iff a '-pre' region exists once build metadata
    -- (which may itself contain '-') is stripped. Exact for hyphenated pre-releases.
    version_is_prerelease BOOLEAN     GENERATED ALWAYS AS
        (position('-' in split_part(version, '+', 1)) > 0) STORED,
    spec                  JSONB       NOT NULL,
    spec_hash             TEXT        NOT NULL,
    artifact_hash         TEXT,
    labels                JSONB       NOT NULL DEFAULT '{}'::jsonb,
    annotations           JSONB       NOT NULL DEFAULT '{}'::jsonb,
    status                TEXT        NOT NULL DEFAULT 'active',
    created_by            UUID,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- Defense-in-depth against a Rust bug that admits a malformed kind.
    -- Adding a new kind is a one-line ALTER TABLE in a future migration.
    CONSTRAINT cards_kind_check CHECK (
        kind IN (
            'Data','Model','Experiment','Prompt','Agent','Workflow','Eval','Drift',
            'Service','Policy','Mcp','Audit','Artifact','Trigger','Operator','Source','External'
        )
    ),

    -- Default 'active'. Soft-delete sets 'deleted'. 'deprecated' is reserved
    -- for the deprecation lifecycle and does not hide the row from list_*.
    CONSTRAINT cards_status_check CHECK (
        status IN ('active','deprecated','deleted')
    ),

    -- Durable identity. card_uid is a handle; this 5-tuple is the key.
    -- register_card's ON CONFLICT clause targets this constraint by name.
    CONSTRAINT cards_identity_unique UNIQUE (data_tenant_id, kind, space, name, version)
);

-- Indexes ---------------------------------------------------------------
-- The UNIQUE constraint above already provides the
-- (data_tenant_id, kind, space, name, version) btree; no second index.

CREATE INDEX idx_cards_kind
    ON wyrd.cards (data_tenant_id, kind, status);

CREATE INDEX idx_cards_space
    ON wyrd.cards (data_tenant_id, space, kind, status);

CREATE INDEX idx_cards_labels_gin
    ON wyrd.cards USING GIN (labels);

CREATE INDEX idx_cards_annotations_gin
    ON wyrd.cards USING GIN (annotations);

CREATE INDEX idx_cards_created_by
    ON wyrd.cards (data_tenant_id, created_by);

-- "Latest stable in line" pushdown (B3 resolve, B4 range/latest, C1 query).
-- Partial (stable-only) matches the default query shape and stays small.
-- DESC lets "ORDER BY version_major DESC, minor DESC, patch DESC LIMIT 1" read
-- the first row. Reads must repeat the WHERE predicates to use it.
CREATE INDEX idx_cards_version_latest
    ON wyrd.cards (
        data_tenant_id, kind, space, name,
        version_major DESC, version_minor DESC, version_patch DESC
    )
    WHERE status <> 'deleted' AND NOT version_is_prerelease;

-- Hash dedup probe (B3 latest-in-line hash compare, C1 find_card_by_spec_hash).
CREATE INDEX idx_cards_spec_hash
    ON wyrd.cards (data_tenant_id, kind, space, name, spec_hash)
    WHERE status <> 'deleted';

-- Row-level security ----------------------------------------------------
-- Mirrors the storage-table pattern at
-- crates/wyrd/wyrd-sql/migrations/20260601000002_storage.sql:53-65.

ALTER TABLE wyrd.cards ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.cards FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.cards
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

-- wyrd_platform_admin requires cross-tenant access for sweepers and
-- operator restore. Named permissive policy avoids cluster-level
-- BYPASSRLS on this role at runtime. Role name confirmed at
-- crates/wyrd/wyrd-sql/migrations/20260601000000_platform.sql:27-33.
CREATE POLICY admin_cross_tenant ON wyrd.cards
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);

-- BEFORE UPDATE trigger -------------------------------------------------
-- Enforces spec_hash immutability and bumps updated_at on every update.
-- spec_hash drift on the same identity is a versioning violation: the
-- caller must mint a new version, not mutate the existing row.

CREATE OR REPLACE FUNCTION wyrd.cards_before_update()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.spec_hash IS DISTINCT FROM NEW.spec_hash THEN
        RAISE EXCEPTION USING
            MESSAGE = 'cards.spec_hash is immutable; create a new version',
            ERRCODE = 'P0001',
            CONSTRAINT = 'cards_spec_hash_immutable';
    END IF;
    IF OLD.version IS DISTINCT FROM NEW.version THEN
        RAISE EXCEPTION USING
            MESSAGE = 'cards.version is immutable; create a new version',
            ERRCODE = 'P0001',
            CONSTRAINT = 'cards_version_immutable';
    END IF;
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER cards_before_update
    BEFORE UPDATE ON wyrd.cards
    FOR EACH ROW
    EXECUTE FUNCTION wyrd.cards_before_update();

-- Grants ---------------------------------------------------------------
-- Matches the storage-table grant pattern at
-- crates/wyrd/wyrd-sql/migrations/20260601000002_storage.sql:135-138.
-- wyrd_app: read + register + soft-delete (UPDATE status).
-- wyrd_platform_admin: read + status repair only; never INSERT here
-- because tenant-scoped writes must flow through wyrd_app for audit.
REVOKE ALL ON TABLE wyrd.cards FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.cards FROM wyrd_platform_admin;
GRANT SELECT, INSERT, UPDATE ON wyrd.cards TO wyrd_app;
GRANT SELECT, UPDATE ON wyrd.cards TO wyrd_platform_admin;
