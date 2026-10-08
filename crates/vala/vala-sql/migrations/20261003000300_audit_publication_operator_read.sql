-- The audit publisher lists the tenants that owe publication in one
-- cross-tenant read on the operator pool, comparing each chain head with its
-- publication watermark. The operator role already reads the chain head; this
-- grants it the matching read of the progress row.
GRANT SELECT ON vala.audit_publication TO wyrd_platform_admin;
