# RBAC CRUD SQL Helpers

Wyrd auth mutation helpers live under `wyrd_sql::queries::auth` and take
`&mut TenantConn<'_>`. The tenant connection binds `app.current_tenant` for the
transaction, so row-level security remains the tenant boundary for users,
roles, service accounts, API keys, and role assignment joins.

User helpers insert active users, fetch by id or email, and soft-delete by
setting `status = 'deleted'`. Service-account helpers insert card-bound
Service or Agent principals, fetch active rows by card ref or id, soft-delete
by status, and resolve API-key status by prefix without exposing secret key
material.

Role helpers fetch by id or name, list tenant roles, upsert by
`(data_tenant_id, name)`, and delete by name. Role-assignment helpers grant,
revoke, and list roles for both users and service accounts using idempotent
`ON CONFLICT DO NOTHING` inserts.

API-key mutation is intentionally small: revocation marks `revoked_at` once and
returns whether the row changed. Rotation remains `revoke_api_key` followed by a
fresh key insert.
