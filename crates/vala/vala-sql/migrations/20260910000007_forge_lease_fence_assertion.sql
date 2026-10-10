-- Let a Vala tenant connection assert a Forge lease fence while holding the
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
    prior text := coalesce(current_setting('app.operator', true), '');
    matched boolean;
BEGIN
    PERFORM set_config('app.operator', 'on', true);
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

    PERFORM set_config('app.operator', prior, true);
    RETURN matched;
END;
$$;
