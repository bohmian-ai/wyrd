-- Server-derived outbound CardRef edges for the registry.
--
-- The source card and target card are both tenant-qualified. This keeps a
-- relationship row from pointing across tenants even if a caller bypasses
-- the Rust resolver.

CREATE TABLE wyrd.card_relationships (
    card_uid       UUID NOT NULL,
    data_tenant_id UUID NOT NULL REFERENCES platform.tenants(data_tenant_id),
    relation_kind  TEXT NOT NULL DEFAULT 'outbound',
    target_kind    TEXT NOT NULL,
    target_space   TEXT NOT NULL,
    target_name    TEXT NOT NULL,
    target_version TEXT NOT NULL,
    target_uid     UUID NOT NULL,

    CONSTRAINT card_relationships_kind_check
        CHECK (relation_kind = 'outbound'),
    CONSTRAINT card_relationships_source_fk
        FOREIGN KEY (data_tenant_id, card_uid)
        REFERENCES wyrd.cards(data_tenant_id, card_uid)
        ON DELETE CASCADE,
    CONSTRAINT card_relationships_target_fk
        FOREIGN KEY (data_tenant_id, target_uid)
        REFERENCES wyrd.cards(data_tenant_id, card_uid),
    CONSTRAINT card_relationships_identity_unique
        UNIQUE (
            data_tenant_id,
            card_uid,
            relation_kind,
            target_kind,
            target_space,
            target_name,
            target_version,
            target_uid
        )
);

ALTER TABLE wyrd.card_relationships ENABLE ROW LEVEL SECURITY;
ALTER TABLE wyrd.card_relationships FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON wyrd.card_relationships
    USING (data_tenant_id = wyrd.current_tenant())
    WITH CHECK (data_tenant_id = wyrd.current_tenant());

CREATE POLICY admin_cross_tenant ON wyrd.card_relationships
    TO wyrd_platform_admin
    USING (true)
    WITH CHECK (true);

CREATE INDEX card_relationships_target_idx
    ON wyrd.card_relationships (data_tenant_id, target_uid);

REVOKE ALL ON TABLE wyrd.card_relationships FROM wyrd_app;
REVOKE ALL ON TABLE wyrd.card_relationships FROM wyrd_platform_admin;
GRANT SELECT, INSERT, DELETE ON wyrd.card_relationships TO wyrd_app;
GRANT SELECT, DELETE ON wyrd.card_relationships TO wyrd_platform_admin;
