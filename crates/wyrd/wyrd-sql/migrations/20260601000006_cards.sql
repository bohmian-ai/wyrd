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
