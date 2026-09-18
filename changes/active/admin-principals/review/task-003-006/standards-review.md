# Repository-standards audit — TASK-003 … TASK-006

**Subject**: branch `claude/admin-principals-spec-qfsmjc`, base
`40a73817d415e9a1626e6ec7a91edda083e344d3`, candidate `9bc53a6`.

**Independence limitation (recorded, not waived)**: `$wyrd-task-review` requires
this audit to be produced by a specialist independent of both implementation and
the acceptance reviewer. This session had no delegation tool available and was
directed to review directly, so the standards audit was performed by the
acceptance reviewer. The audit is complete against the authorities listed below;
the independence property is not established. Treat this as a known gap in the
review, not as an argument for or against the verdict.

## Authority coverage

| Changed surface | Governing authority | Result |
|---|---|---|
| `wyrd-server/src/boot/init.rs`, `main.rs` | AGENTS.md §4, §5, §6, §16; spec REQ-020…024 | **FAIL** (§ below) |
| `wyrd-server/src/components/platform/{routes,provisioning,recovery}.rs` | AGENTS.md §5, §6, §9, §16; agent-rules connection abstractions + transactional authorization audit | **PARTIAL PASS** |
| `wyrd-server/src/components/principals/routes.rs` | AGENTS.md §5, §9, §16; agent-rules transactional authorization audit; spec REQ-037 | **FAIL** |
| `wyrd-sql/src/queries/platform/*`, `queries/auth/*` | AGENTS.md §15 (`wyrd-sql` is the durable Postgres layer), §16 | **PASS** |
| `wyrd-sql/migrations/…20,…21,…22` | agent-rules RLS/tenant boundary; forward-only migration precedent | **PASS** |
| `wyrd-spec/src/auth/{tenant_admin,tenant_principals}.rs` | AGENTS.md §9 typed wire bodies; §15 `wyrd-spec` scope | **PARTIAL PASS** |
| `wyrd-server/tests/platform_admin_e2e.rs` | AGENTS.md §11 test taxonomy | **PARTIAL PASS** |
| `mise.toml` | AGENTS.md §11 verification lanes | **PASS** |
| `docs/`, `crates/wyrd/wyrd-cli/`, `openapi.yaml` | AGENTS.md §9, §12; spec REQ-036, REQ-040 | **FAIL** (untouched) |

## Rule-by-rule result

### AGENTS.md §4 Rust Core Rules — PASS with one note
- Domain newtypes (`DataTenantId`, `PrincipalId`, `TenantSlug`, `RequestId`) are
  used for durable identifiers throughout the new code. PASS.
- `thiserror` for library errors (`InitError`, `ProvisionError`,
  `PlatformAuthzError`), `WyrdError` for the public catalog. PASS.
- `secrecy::SecretString` carries the root credential out of
  `initialize_platform_root`; `SecretBearer` carries it on the wire. PASS.
- No `unwrap()` on environment/IO/parse paths in non-test code. One `expect` at
  `boot/init.rs:94` (`"permission set serializes to JSON"`) names a genuine
  serialization invariant over an owned in-memory value. PASS.
- `#[tracing::instrument]` with `skip(...)` on every handler and service method;
  no secret-bearing argument is recorded. PASS.

### AGENTS.md §5 Struct-centered style — PASS
`TenantProvisioning`, `TenantRecovery`, `PlatformAuthorization`,
`PlatformCredentials`, `PlatformSessions` are dependency-owning concrete structs
with inherent methods, matching the `wyrd-registry::Cards` precedent. The free
functions that remain (`slug_or_store`, `metadata`, `internal`,
`platform_root_grant`, `require_principal_admin`, `tenant_conn`,
`provision_error`, `not_configured`) are stateless conversions or per-request
helpers with no natural owner. No zero-sized utility struct was introduced. PASS.

### AGENTS.md §6 Async rules — PASS
`async` appears only at genuine IO boundaries. Argon2 hashing is moved off the
reactor with `tokio::task::spawn_blocking` in all three issuance sites
(`provisioning.rs:223`, `recovery.rs:105`, `principals/routes.rs:103`). No ad
hoc runtime is constructed. PASS.

### AGENTS.md §9 Server and contract rules — FAIL
- Typed request/response structs with `deny_unknown_fields` on inputs. PASS.
- Handlers return structured `WyrdError` values. PASS, with one defect: a
  duplicate principal name is a `UNIQUE (data_tenant_id, name)` violation that
  `principals/routes.rs:159` maps through `internal()` to
  `WyrdError::Internal` (HTTP 500) instead of a caller-correctable conflict.
  The equivalent case on the platform plane is handled correctly by
  `slug_or_store` (`provisioning.rs:256`), so the repository already owns the
  pattern this surface did not reuse.
- **Durable write operations carry the required audit context — FAIL.**
  `components/principals/routes.rs` performs four authorization decisions
  (`require_principal_admin`, lines 137, 199, 217, 241) and writes durable
  credential and principal rows, and appends no audit row for any of them,
  allowed or denied. `components/admin/routes.rs:15-17` still records the
  deliberate no-audit stance that TASK-005 was to correct. This violates
  AGENTS.md §2 ("Audit records authorization decisions … Every decision that
  evaluates a principal's permission is transactionally audited in the
  transaction that made it") and `architecture/agent-rules.md`. The platform
  plane does satisfy the rule — `PlatformAuthorization::authorize` appends the
  row in the deciding transaction, rolls back and refuses when the append fails,
  and commits denials durably.
- **Generated artifacts — FAIL.** None of the six new routes carry
  `#[utoipa::path]` and none are registered in
  `wyrd-server/src/http/openapi.rs`, so `openapi.yaml` contains no `/platform/*`
  or `/v1/principals*` path. `codegen:check` passes only because nothing
  regenerates; the absence is silent.

### AGENTS.md §11 Test taxonomy — PARTIAL PASS
Five real-server journeys exist and drive client → server → client over the
shipped HTTP surface with repository-managed Postgres. That is the right tier.
The gap is coverage, not tier: the negative and edge flows §11 explicitly
requires (injected staged failure, concurrency, backpressure-equivalent retry
convergence, audit-append failure) are absent for these surfaces, and three
acceptance obligations have no test at any tier. See the acceptance matrix in
`verdict.md`. AC-002's "through the shipped SDK and CLI" is not met: the journey
calls `initialize_platform_root` as a Rust function, not the `wyrd-server init`
subcommand, and no CLI exists for tenant creation or principal administration.

Note on evidence quality: `queries/auth/service_accounts.rs:506` asserts
`contains()` against a SQL string literal re-declared inside the test rather than
against the production query. It was updated by this change but not introduced
by it; it proves nothing about `api_key_by_prefix` and should not be counted as
evidence for the nullable-expiry behavior.

### AGENTS.md §16 Rustdoc — PASS
Every new module, struct, field, enum, variant, function, method, and test
function carries rustdoc that states intent and its role in the surrounding
workflow. Every fallible function carries `# Errors` naming its conditions.
Three doc comments assert behavior the code does not implement, which is a
correctness finding rather than a documentation-coverage one:
- `queries/platform/provisioning.rs:75` — "a retry knows it is resuming rather
  than starting fresh"; no resume path exists.
- `queries/platform/principals.rs:44-47` — correctly discloses that
  initialization is three separate statements, which is itself the §4/spec
  violation recorded in `verdict.md`.
- `components/platform/provisioning.rs:11-13` — "the order is chosen so that
  every failure leaves a tenant that is visibly incomplete"; the order is right,
  but nothing consumes the incompleteness (see FIND-004-1).

### architecture/agent-rules.md — connection abstractions — PASS
Exactly two boundaries are used. Platform-scope work takes `&OperatorPool`
(`principals.rs`, `credentials.rs`, `principal_grants.rs`, `audit_authz.rs`,
`provisioning.rs`) or a transaction begun from it; tenant-scope work takes
`&mut TenantConn<'_>` and relies on RLS. `TenantProvisioning` and
`TenantRecovery` hold both and use each for its own rows. No third abstraction,
no hand-written tenant filter added to a tenant-scope query, no widened query.
PASS.

### architecture/agent-rules.md — tenant isolation — PASS
`wyrd.auth_api_keys` retains its composite FK `(data_tenant_id, principal_id) →
wyrd.auth_service_accounts` plus `FORCE ROW LEVEL SECURITY`, so the
caller-supplied `principal_id` on `/v1/principals/{id}/credentials` cannot name a
principal outside the authenticated tenant: the insert fails the FK under the
tenant binding. `tenant_conn` derives the tenant from `caller.data_tenant_id`
only. PASS.

### Migration safety — PASS
- `20260601000022` drops the auto-named `platform.tenants` status check by
  discovering `contype = 'c' AND pg_get_constraintdef(oid) LIKE '%status%'`. The
  only other check on that table constrains `slug` and its definition contains
  no `status` token, so the predicate selects exactly one constraint. The
  replacement is a superset of the original domain, so every existing row
  validates without `NOT VALID`.
- `tenants_failure_reason_consistency` is added together with a nullable
  `provisioning_failed_reason`; existing rows are `status <> 'failed'` with a
  NULL reason and satisfy it.
- The same name-agnostic pattern on `wyrd.auth_refresh_tokens` and
  `wyrd.auth_service_accounts` filters on `%principal_kind%`; each table has
  exactly one such check, and both replacements are supersets. The pattern
  matches the `20260601000011` precedent.
- `ALTER TABLE wyrd.auth_api_keys RENAME COLUMN sa_id TO principal_id` carries
  the FK, the partial index and the RLS policy automatically; the dependent
  index rename uses `ALTER INDEX IF EXISTS`.
- `expires_at DROP NOT NULL` is paired with the `api_key_by_prefix` predicate
  change to `(k.expires_at IS NULL OR k.expires_at > now())`, so no previously
  valid key becomes invalid and no previously invalid key becomes valid.
- Ordering is sound: `…20` widens `auth_service_accounts` kinds before `…22`
  widens `auth_refresh_tokens` kinds, and no data backfill is required.
PASS. Verified by `pg_migration` and `pg_admin_principals` (9 tests, green).

### AGENTS.md §12 Completion standard — FAIL
Format and lints pass (`cargo clippy -p wyrd-server -p wyrd-sql -p wyrd-auth
--all-targets`, clean). No gate was weakened, disabled, ignored or deleted; no
`#[allow]` was added; the one pre-existing clippy allow on the card-free path was
removed rather than widened. But "public contracts regenerate cleanly when
touched" is not met (the OpenAPI document was not extended), and the targeted
tests for the touched surface do not cover three acceptance obligations.

## Material standards findings

1. **Tenant-plane authorization decisions are unaudited.**
   `crates/wyrd/wyrd-server/src/components/principals/routes.rs:57-69, 137, 199,
   217, 241`. Violates AGENTS.md §2 and `architecture/agent-rules.md`
   transactional authorization audit. Correction: route each decision through a
   deciding transaction that appends its row before the operation, refuse when
   the append fails, and record the denial durably — the shape
   `wyrd-auth/src/platform_authz.rs` already owns for the platform plane — and
   retire the no-audit note in `components/admin/routes.rs:15-17`.

2. **New HTTP routes are absent from the generated contract.**
   `crates/wyrd/wyrd-server/src/http/openapi.rs:14-41`. Violates AGENTS.md §9
   and spec REQ-036. Correction: annotate the six handlers and register their
   paths and schemas, then regenerate.

3. **A caller-correctable conflict is reported as an internal error.**
   `crates/wyrd/wyrd-server/src/components/principals/routes.rs:159` via
   `internal()` at line 271. Violates AGENTS.md §9 stable-error contract.
   Correction: classify the unique violation the way `slug_or_store` already
   does on the platform plane.

4. **Initialization is not one transaction.**
   `crates/wyrd/wyrd-server/src/boot/init.rs:76-101`. Violates the spec's
   REQ-021/REQ-023 and is recorded here because it is also an AGENTS.md §15
   root-cause question: the durable multi-write belongs in one `wyrd-sql`
   transaction rather than three pool statements composed in the server tier.
   Full diagnosis in `verdict.md` (FIND-003-1).
