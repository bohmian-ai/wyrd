-- Let the Vala application role assert a Forge lease fence while holding the
-- lease row lock in the same transaction as a tenant mutation.
CREATE OR REPLACE FUNCTION vala.assert_maintenance_lease_fence(
    p_lease_key text,
    p_owner uuid,
    p_fencing_token bigint
)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = vala, pg_catalog
AS $$
DECLARE
    matched boolean;
BEGIN
    SELECT EXISTS (
        SELECT 1
          FROM vala.maintenance_leases
         WHERE lease_key = p_lease_key
           AND owner = p_owner
           AND fencing_token = p_fencing_token
           AND expires_at > clock_timestamp()
         FOR UPDATE
    )
    INTO matched;

    RETURN matched;
END;
$$;

REVOKE ALL ON FUNCTION vala.assert_maintenance_lease_fence(text, uuid, bigint) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION vala.assert_maintenance_lease_fence(text, uuid, bigint) TO wyrd_app;
