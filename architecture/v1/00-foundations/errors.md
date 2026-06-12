# Error Catalog Foundations

Wyrd-coded errors use stable `WYRD_<DOMAIN>_<STATUS>_<SLUG>` codes generated
from `#[wyrd_error(...)]` metadata. Public and cross-crate error families must
not hand-write parallel `code`, `status`, `title`, or `remediation` methods.

## SQL Error Family

`wyrd-sql` owns the shared `WYRD_SQL_*` catalog for server-tier SQL failures.
`vala-sql` re-exports `wyrd_sql::error::SqlError` so operators and API
consumers see one SQL family regardless of which storage crate issued the
query.

| Code | Status | Meaning |
|---|---:|---|
| `WYRD_SQL_500_CONNECT` | 500 | Database connection or migration bootstrap SQL failed. |
| `WYRD_SQL_500_MIGRATE` | 500 | Migration execution failed for a reason other than checksum drift. |
| `WYRD_SQL_500_MIGRATE_CHECKSUM` | 500 | A previously-applied migration was modified. |
| `WYRD_SQL_500_QUERY` | 500 | Generic database query failure. |
| `WYRD_SQL_404_NO_ROWS` | 404 | A query expected a row and none was returned. |
| `WYRD_SQL_409_UNIQUE_VIOLATION` | 409 | PostgreSQL SQLSTATE `23505`. |
| `WYRD_SQL_409_FK_VIOLATION` | 409 | PostgreSQL SQLSTATE `23503`. |
| `WYRD_SQL_409_CHECK_VIOLATION` | 409 | PostgreSQL SQLSTATE `23514`. |
| `WYRD_SQL_500_TX_FAILED` | 500 | Tenant-scoped transaction begin, binding, or commit failed after acquisition. |
| `WYRD_SQL_403_RLS_DENIED` | 403 | PostgreSQL SQLSTATE `42501` whose message identifies row-level security. |
| `WYRD_SQL_500_INVALID_TENANT_ID` | 500 | Stored tenant data violates Wyrd's `DataTenantId` contract. |

Constraint mappings use SQLSTATE codes instead of parsing database text. RLS
denials are the narrow exception: PostgreSQL reports both ordinary permission
denials and row-level security denials as SQLSTATE `42501`, so Wyrd only maps
that state to `RlsDenied` when the database message mentions row-level
security.
