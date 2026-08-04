-- Scribe publication validates and locks its exact current membership fence
-- through the audited operator transaction that also owns file-list and audit
-- publication. Membership mutation and direct operator reads remain forbidden.
CREATE FUNCTION vala.assert_scribe_publication_fence(
    p_data_tenant_id uuid,
    p_node_id uuid,
    p_fencing_token bigint
) RETURNS boolean
LANGUAGE sql
SECURITY DEFINER
SET search_path = pg_catalog, vala
AS $$
    SELECT EXISTS (
        SELECT 1
          FROM vala.cluster_nodes
         WHERE data_tenant_id = p_data_tenant_id
           AND node_id = p_node_id
           AND role = 'scribe'
           AND fencing_token = p_fencing_token
         FOR UPDATE
    )
$$;

REVOKE ALL ON FUNCTION vala.assert_scribe_publication_fence(uuid, uuid, bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.assert_scribe_publication_fence(uuid, uuid, bigint)
    TO wyrd_platform_admin;
