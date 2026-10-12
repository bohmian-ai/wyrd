-- Make a tenant's lifecycle state decide whether its credentials work.
--
-- Tenant statuses were introduced with the administrative principals change,
-- but nothing on the authentication path ever read them: credential exchange
-- checked the *principal's* status, and `TenantConn` does no directory lookup
-- at all. A suspended or failed tenant's credentials therefore kept
-- authenticating and authorizing exactly as before, which makes suspension a
-- label rather than a control.
--
-- The check belongs at the one place tenant entry converges — credential
-- exchange — rather than in each handler, where it would be a rule every new
-- route had to remember.
--
-- Tenant sessions cannot read `platform.tenants`, and admitting them would widen
-- the tenant plane's reach into the directory for one boolean. A SECURITY
-- DEFINER operator-session function, the pattern
-- `platform.resolve_tenant_by_slug` already establishes, answers the one
-- question without exposing the table.
CREATE FUNCTION platform.tenant_admits_credentials(p_tenant uuid)
RETURNS boolean
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, platform
AS $$
DECLARE
    prior text := coalesce(current_setting('app.operator', true), '');
    admits boolean;
BEGIN
    PERFORM set_config('app.operator', 'on', true);
    SELECT EXISTS (
        SELECT 1 FROM platform.tenants
        WHERE data_tenant_id = p_tenant
          AND status = 'active'
          AND deleted_at IS NULL
    ) INTO admits;
    PERFORM set_config('app.operator', prior, true);
    RETURN admits;
END;
$$;
