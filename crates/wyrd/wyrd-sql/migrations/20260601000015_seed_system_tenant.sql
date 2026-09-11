-- Restore the established platform/system tenant used for audit events that
-- cannot trust request-carried tenant data.
--
-- The migration is intentionally strict: only a fresh row or the exact
-- compatible sentinel is accepted. Ambiguous ownership fails transactionally
-- without overwriting platform state.

DO $$
DECLARE
    sentinel platform.tenants%ROWTYPE;
    slug_owner uuid;
BEGIN
    SELECT *
      INTO sentinel
      FROM platform.tenants
     WHERE data_tenant_id = '00000000-0000-0000-0000-000000000000'::uuid
     FOR UPDATE;

    SELECT data_tenant_id
      INTO slug_owner
      FROM platform.tenants
     WHERE slug = 'wyrd-system'
     FOR UPDATE;

    IF FOUND AND slug_owner <> '00000000-0000-0000-0000-000000000000'::uuid THEN
        RAISE EXCEPTION
            'canonical system tenant slug is owned by another tenant';
    END IF;

    IF sentinel.data_tenant_id IS NULL THEN
        INSERT INTO platform.tenants
            (data_tenant_id, slug, display_name, status, deleted_at)
        VALUES
            ('00000000-0000-0000-0000-000000000000'::uuid,
             'wyrd-system',
             'Wyrd System',
             'active',
             NULL);
    ELSIF sentinel.slug <> 'wyrd-system'
       OR sentinel.display_name <> 'Wyrd System'
       OR sentinel.status <> 'active'
       OR sentinel.deleted_at IS NOT NULL THEN
        RAISE EXCEPTION
            'existing system tenant sentinel has incompatible attributes';
    END IF;
END $$;
