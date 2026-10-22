-- Gateway snapshot invalidation.
--
-- Servers cache each tenant's gateway snapshot (deployments, credentials,
-- pricing, and policies) and drop it when this channel names the tenant. Any
-- write to a snapshot table notifies with payload '<data_tenant_id>'. NOTIFY
-- is transactional, so a listener hears it only after the write commits, and
-- identical payloads in one transaction are delivered once.

CREATE FUNCTION wyrd.notify_gateway_snapshot() RETURNS trigger
LANGUAGE plpgsql
SET search_path = ''
AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        PERFORM pg_catalog.pg_notify('wyrd_gateway_snapshot', OLD.data_tenant_id::text);
    ELSE
        PERFORM pg_catalog.pg_notify('wyrd_gateway_snapshot', NEW.data_tenant_id::text);
    END IF;
    RETURN NULL;
END
$$;

CREATE TRIGGER gateway_provider_deployments_notify_snapshot
    AFTER INSERT OR UPDATE OR DELETE ON wyrd.gateway_provider_deployments
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_gateway_snapshot();

CREATE TRIGGER gateway_provider_credentials_notify_snapshot
    AFTER INSERT OR UPDATE OR DELETE ON wyrd.gateway_provider_credentials
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_gateway_snapshot();

CREATE TRIGGER gateway_model_pricing_notify_snapshot
    AFTER INSERT OR UPDATE OR DELETE ON wyrd.gateway_model_pricing
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_gateway_snapshot();

CREATE TRIGGER gateway_policies_notify_snapshot
    AFTER INSERT OR UPDATE OR DELETE ON wyrd.gateway_policies
    FOR EACH ROW EXECUTE FUNCTION wyrd.notify_gateway_snapshot();
