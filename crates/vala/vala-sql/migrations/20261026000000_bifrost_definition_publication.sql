-- Tenant-bound publication of one table definition edit.
--
-- An explicit table edit publishes its Iceberg pointer and its new version's
-- retained reference in the same tenant transaction that refreshes the
-- control row and announces the change, so the edit commits or rolls back as
-- one. Tenant sessions have no access to iceberg_catalog; this definer is
-- their only write to it. It derives the physical namespace from
-- wyrd.current_tenant() alone, requires the table to be registered for that
-- tenant and its version references to name p_table_uuid, keeps the new
-- metadata beside the current one, and swaps only that table's pointer from
-- the caller's expected location. No tenant, catalog, or physical namespace
-- is accepted from the caller.
--
-- Returns false, changing nothing, when the pointer has moved or the table is
-- not at p_table_version - 1; the caller reports that as a version conflict.
-- iceberg_tables and iceberg_table_versions are created at runtime by
-- iceberg-catalog-sql, which PL/pgSQL resolves at first execution.
CREATE FUNCTION vala.bifrost_publish_table_definition(
    p_namespace_name text,
    p_table_name text,
    p_table_uuid text,
    p_table_version bigint,
    p_expected_location text,
    p_published_location text
) RETURNS boolean
LANGUAGE plpgsql
VOLATILE
SECURITY DEFINER
SET search_path = ''
AS $$
DECLARE
    v_tenant uuid := wyrd.current_tenant();
    v_namespace text;
    v_current bigint;
BEGIN
    IF p_namespace_name IS NULL
       OR p_namespace_name !~ '^vala\.[^.]+$'
       OR p_table_name IS NULL
       OR p_table_name = ''
       OR p_table_uuid IS NULL
       OR p_table_version IS NULL
       OR p_table_version < 2
       OR p_expected_location IS NULL
       OR p_published_location IS NULL
       OR pg_catalog.regexp_replace(p_published_location, '/[^/]*$', '')
          IS DISTINCT FROM pg_catalog.regexp_replace(p_expected_location, '/[^/]*$', '') THEN
        RAISE EXCEPTION 'invalid Bifrost table definition publication'
            USING ERRCODE = 'invalid_parameter_value';
    END IF;
    IF NOT EXISTS (
        SELECT 1
          FROM vala.bifrost_table_maintenance_authority
         WHERE data_tenant_id = v_tenant
           AND catalog_name = 'wyrd-redux'
           AND namespace_name = p_namespace_name
           AND table_name = p_table_name
    ) THEN
        RAISE EXCEPTION 'Bifrost table is not registered for this tenant'
            USING ERRCODE = 'no_data_found';
    END IF;
    v_namespace := 'vala.tenants.' || v_tenant::text || '.' || pg_catalog.substr(p_namespace_name, 6);

    SELECT pg_catalog.max(table_version)
      INTO v_current
      FROM iceberg_catalog.iceberg_table_versions
     WHERE catalog_name = 'wyrd-redux'
       AND table_namespace = v_namespace
       AND table_name = p_table_name
       AND table_uuid = p_table_uuid;
    IF v_current IS NULL THEN
        RAISE EXCEPTION 'Bifrost table identity does not match'
            USING ERRCODE = 'no_data_found';
    END IF;
    IF v_current <> p_table_version - 1 THEN
        RETURN false;
    END IF;

    UPDATE iceberg_catalog.iceberg_tables
       SET metadata_location = p_published_location,
           previous_metadata_location = p_expected_location
     WHERE catalog_name = 'wyrd-redux'
       AND table_namespace = v_namespace
       AND table_name = p_table_name
       AND (iceberg_type = 'TABLE' OR iceberg_type IS NULL)
       AND metadata_location = p_expected_location;
    IF NOT FOUND THEN
        RETURN false;
    END IF;
    INSERT INTO iceberg_catalog.iceberg_table_versions
        (catalog_name, table_namespace, table_name, table_uuid, table_version, metadata_location)
    VALUES ('wyrd-redux', v_namespace, p_table_name, p_table_uuid, p_table_version,
            p_published_location);
    RETURN true;
END
$$;
