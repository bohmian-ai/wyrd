-- Scribe publication validates and locks its exact current membership fence
-- through the audited operator transaction that also owns file-list and audit
-- publication. Membership mutation and direct operator reads remain forbidden.
CREATE FUNCTION vala.assert_scribe_publication_fence(
    p_data_tenant_id uuid,
    p_node_id uuid,
    p_fencing_token bigint
) RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, vala
AS $$
DECLARE
    prior text := coalesce(current_setting('app.operator', true), '');
    matched boolean;
BEGIN
    PERFORM set_config('app.operator', 'on', true);
    SELECT EXISTS (
        SELECT 1
          FROM vala.cluster_nodes
         WHERE data_tenant_id = p_data_tenant_id
           AND node_id = p_node_id
           AND role = 'scribe'
           AND fencing_token = p_fencing_token
         FOR UPDATE
    ) INTO matched;
    PERFORM set_config('app.operator', prior, true);
    RETURN matched;
END;
$$;
