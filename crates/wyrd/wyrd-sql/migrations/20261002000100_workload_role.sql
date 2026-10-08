-- Built-in `workload` and `wyrd_default` roles for existing tenants.
--
-- New tenants receive both from the server's built-in role seed
-- (`wyrd_runtime::builtin_roles::BUILTIN_ROLES`); every tenant that already
-- carries the built-in roles gets the same rows here, with the permissions
-- that seed serializes. A Card-bound Service or Agent principal is granted
-- `wyrd_default` at first projection, so the principals that already exist
-- receive it here too. `workload`, which adds tenant-wide Bifrost query reads,
-- is only ever granted explicitly.
INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin)
SELECT gen_random_uuid(), data_tenant_id, 'workload',
       '[{"resource":"bifrost_table","action":"read","scope":"all"},
         {"resource":"bifrost_record","action":"write","scope":"all"},
         {"resource":"bifrost_query","action":"read","scope":"all"}]'::jsonb,
       TRUE
  FROM wyrd.auth_roles
 WHERE builtin AND name = 'admin'
ON CONFLICT (data_tenant_id, name) DO NOTHING;

INSERT INTO wyrd.auth_roles (id, data_tenant_id, name, permissions, builtin)
SELECT gen_random_uuid(), data_tenant_id, 'wyrd_default',
       '[{"resource":"bifrost_table","action":"read","scope":"all"},
         {"resource":"bifrost_record","action":"write","scope":"all"},
         {"resource":"evals","action":"run","scope":"all"}]'::jsonb,
       TRUE
  FROM wyrd.auth_roles
 WHERE builtin AND name = 'admin'
ON CONFLICT (data_tenant_id, name) DO NOTHING;

INSERT INTO wyrd.auth_service_account_roles (data_tenant_id, service_account_id, role_id)
SELECT sa.data_tenant_id, sa.id, r.id
  FROM wyrd.auth_service_accounts sa
  JOIN wyrd.auth_roles r
    ON r.data_tenant_id = sa.data_tenant_id
   AND r.builtin
   AND r.name = 'wyrd_default'
 WHERE sa.principal_kind IN ('service', 'agent')
   AND sa.card_ref IS NOT NULL
ON CONFLICT (data_tenant_id, service_account_id, role_id) DO NOTHING;
