-- Built-in `workload` role for existing tenants.
--
-- New tenants receive it from the server's built-in role seed
-- (`wyrd_runtime::builtin_roles::BUILTIN_ROLES`); every tenant that already
-- carries the built-in roles gets the same row here, with the permissions that
-- seed serializes. A Card-bound Service or Agent principal is granted it at
-- first projection. Existing principals are not granted it retroactively:
-- they keep exactly the roles an administrator gave them.
INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin)
SELECT gen_random_uuid(), data_tenant_id, 'workload',
       '[{"resource":"bifrost_record","action":"write","scope":"all"},
         {"resource":"bifrost_query","action":"read","scope":"all"}]'::jsonb,
       TRUE
  FROM wyrd.auth_roles
 WHERE builtin AND name = 'admin'
ON CONFLICT (data_tenant_id, name) DO NOTHING;
