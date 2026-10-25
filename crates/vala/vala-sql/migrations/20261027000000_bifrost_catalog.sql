-- The public Bifrost catalog is `bifrost`.
--
-- Logical namespaces are `bifrost.<segment>` and physical tenant namespaces
-- `bifrost.tenants.<tenant>.<segment>`. The two tenant-bound definers resolve
-- only that spelling, and Forge schedules only those namespaces. Rows recorded
-- under the retired spelling are not converted and resolve nothing.

ALTER TABLE vala.forge_tasks
    DROP CONSTRAINT forge_tasks_namespace_name_check,
    ADD CONSTRAINT forge_tasks_namespace_name_check
        CHECK (namespace_name IN ('bifrost.system','bifrost.bifrost','bifrost.traces','bifrost.metrics','bifrost.logs','bifrost.drift','bifrost.verification','bifrost.dev','bifrost.datasets','bifrost.gateway'))
        NOT VALID;

CREATE OR REPLACE FUNCTION vala.oracle_catalog_metadata_location(
    p_namespace_name text,
    p_table_name text
) RETURNS text
LANGUAGE plpgsql
STABLE
SECURITY DEFINER
SET search_path = ''
AS $$
DECLARE
    v_tenant uuid := wyrd.current_tenant();
    v_segment text;
    v_location text;
    v_matches bigint;
BEGIN
    IF v_tenant IS NULL
       OR p_namespace_name IS NULL
       OR p_table_name IS NULL
       OR p_table_name = ''
       OR p_namespace_name !~ '^bifrost\.[^.]+$' THEN
        RETURN NULL;
    END IF;
    v_segment := pg_catalog.substr(p_namespace_name, 9);
    SELECT pg_catalog.max(metadata_location), pg_catalog.count(*)
      INTO v_location, v_matches
      FROM iceberg_catalog.iceberg_tables
     WHERE catalog_name = 'wyrd-redux'
       AND table_namespace = 'bifrost.tenants.' || v_tenant::text || '.' || v_segment
       AND table_name = p_table_name
       AND (iceberg_type = 'TABLE' OR iceberg_type IS NULL);
    IF v_matches <> 1 THEN
        RETURN NULL;
    END IF;
    RETURN v_location;
EXCEPTION
    WHEN undefined_table THEN
        RETURN NULL;
END
$$;

CREATE OR REPLACE FUNCTION vala.bifrost_publish_table_definition(
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
       OR p_namespace_name !~ '^bifrost\.[^.]+$'
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
    v_namespace := 'bifrost.tenants.' || v_tenant::text || '.' || pg_catalog.substr(p_namespace_name, 9);

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
