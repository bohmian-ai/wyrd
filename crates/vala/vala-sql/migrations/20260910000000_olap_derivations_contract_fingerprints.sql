-- olap_derivations_contract_fingerprints — immutable derivation contract guard.
--
-- Adds three 32-byte SHA-256 fingerprint columns to vala.olap_derivations:
--
--   source_schema_fingerprint  SHA-256 of the source table's declared user Arrow
--                              fields (name || data_type, field-by-field). Computed
--                              by DomainTable::schema_fingerprint() at boot.
--
--   transform_fingerprint      SHA-256 of the derivation's CONTRACT_VERSION string.
--                              Must change whenever the transform logic changes
--                              semantics (new mapping, new output schema, etc.).
--
--   target_set_fingerprint     SHA-256 of the sorted target UIDs concatenated
--                              (all 16-byte UIDs in lexicographic sort order).
--                              Detects target-table routing changes.
--
-- Columns are NULLABLE so pre-existing rows (created by earlier migrations or
-- in-progress test fixtures) survive without back-filling. New writes always
-- supply all three values. The register_derivation_with_contract query enforces
-- that a re-registration with different fingerprints is DriftRejected — the
-- derivation worker skips that tenant to prevent stale watermark reuse under
-- changed semantics.

ALTER TABLE vala.olap_derivations
    ADD COLUMN source_schema_fingerprint BYTEA
        CHECK (source_schema_fingerprint IS NULL OR octet_length(source_schema_fingerprint) = 32),
    ADD COLUMN transform_fingerprint BYTEA
        CHECK (transform_fingerprint IS NULL OR octet_length(transform_fingerprint) = 32),
    ADD COLUMN target_set_fingerprint BYTEA
        CHECK (target_set_fingerprint IS NULL OR octet_length(target_set_fingerprint) = 32);

COMMENT ON COLUMN vala.olap_derivations.source_schema_fingerprint IS
    'SHA-256 of the source table declared user Arrow fields (name+data_type per field). NULL for rows inserted before this migration.';
COMMENT ON COLUMN vala.olap_derivations.transform_fingerprint IS
    'SHA-256 of the derivation CONTRACT_VERSION string. Change this constant when derivation semantics change. NULL for rows inserted before this migration.';
COMMENT ON COLUMN vala.olap_derivations.target_set_fingerprint IS
    'SHA-256 of all target table UIDs concatenated in lexicographic order. NULL for rows inserted before this migration.';
