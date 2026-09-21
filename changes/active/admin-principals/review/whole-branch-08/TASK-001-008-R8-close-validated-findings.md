# TASK-001-008-R8 — Close the validated cumulative findings

## Route and authority

Implement this remediation with `$wyrd-implement`. The next
`$wyrd-task-review` must reassess the complete cumulative candidate.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  13, status `approved`, SHA-256
  `57f91317e68b06e7b4d34ea94b964e4a1dd99678275a2ee67d1d51f9b4b46332`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`.
- Parent remediation:
  `changes/active/admin-principals/review/whole-branch-07/TASK-001-008-R7-close-validated-findings.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed cumulative candidate:
  `eb9b2f69cb883fa508ed168f21cb868451e61b82`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-08/findings-validation.md`.
- Remediates: `FIND-admin-principals-R6-1`,
  `FIND-admin-principals-R7-5`, `FIND-admin-principals-R8-2`,
  `FIND-admin-principals-R8-3`, and `FIND-admin-principals-R8-5` through
  `FIND-admin-principals-R8-7`. The human authority rejected
  `FIND-admin-principals-R8-1` and `FIND-admin-principals-R8-4`; keep
  `67b4d0ba` and add no pre-release audit compatibility migration.
- Implements newly approved `REQ-012c`, `INV-013a`, and `AC-020`.

## Outcome

Keep the approved core authentication architecture: one
shared issuance owner mints five-minute tenant JWTs whose `permissions` claim
is the request authority; tenant requests verify those JWTs locally; platform
requests revalidate current state through Postgres. Close the remaining live
contract, security, upgrade, RLS, gRPC-proof, public-error, evidence, and scope
defects without introducing another auth mechanism or general framework.
Delegation must attenuate permissions to the caller/target overlap and durably
audit every authorization decision before responding.

## Findings and required corrections

### `FIND-admin-principals-R6-1` — deleted tenant-auth contracts remain live

The epoch/checker/cache runtime was removed, but live rustdoc, runtime comments,
MCP and CLI text, active documentation and dependent specifications still
describe immediate invalidation or request-time resolution. Protected tenant
OpenAPI operations advertise `WYRD_AUTH_401_CREDENTIAL_REVOKED` even though the
local verifier cannot emit it, and `wyrd-server` still directly depends on
unused `moka`. This violates the revision-12 deletion and publishes two
different auth contracts.

Update the existing live text in place: issuance reads current state once,
`permissions` is the tenant authority snapshot, local verification is
database-free, lifecycle changes stop new issuance immediately, and an issued
tenant JWT remains valid for at most five minutes. Reconcile active dependent
specifications without rewriting historical review packets, completed evidence,
or immutable migrations. Remove the credential-revoked response only from
protected tenant operations where it is unreachable, preserving issuance and
platform occurrences that can emit it. Remove only the unused direct
`wyrd-server` `moka` dependency; retain the live OIDC JWKS cache dependency.

### `FIND-admin-principals-R7-5` — exact focused evidence is absent

The R7 evidence names tests but records one command template and the placeholder
`N tests run: N passed`. That cannot prove the exact expressions selected a
nonzero test on the final tree.

For every specifically named closure test, record the literal final-candidate
command, positive selected count, pass result, and owning lane. Use the existing
runner and environment-owning wrapper; add no test harness and do not replace
focused proof with a family aggregate.

### `FIND-admin-principals-R8-2` — CLI secrets enter argv and debug output

The new platform and tenant administration endpoint arguments accept
`--credential` or `--token`, store the secret in `String`, and derive `Debug`.
The values can enter shell history, process argv, and diagnostics.

Keep the server URL argument, but delete the two secret-valued CLI options.
Reuse the existing ambient `ClientConfig` credential chain for tenant commands
and `WYRD_PLATFORM_CREDENTIAL` for platform commands. Carry resolved secrets as
`SecretString` and ensure containing debug output is redacted. Add no credential
source abstraction and no second client builder.

### `FIND-admin-principals-R8-3` — function-scoped import

The candidate imports `sha2::Digest` inside
`shipped_audit_staging_migration_is_immutable`, contrary to the module-scope
import rule. Move only that import into the existing `pg_tests` import block;
add no wrapper, alias, or new test.

### `FIND-admin-principals-R8-5` — tenant queries duplicate forced RLS

Four new statements in API-key, role-assignment, and service-account query
owners add tenant predicates even though every caller supplies `TenantConn`.
This creates a second tenant boundary beside forced RLS.

Remove only the redundant tenant `WHERE` expressions and unused binds from
those four statements. Preserve tenant keys on inserts, tenant-qualified
composite joins, principal and credential predicates, RLS policies, and caller
transaction ownership.

### `FIND-admin-principals-R8-6` — scoped Bifrost authority lacks real gRPC proof

The scoped allow/refuse matrix currently calls the HTTP `/v1/query` path; the
real gRPC expiry journey uses global authority. The production Oracle check is
present, but R7 explicitly required exact- or schema-scoped proof through real
gRPC serving.

Extend the existing bound-server Bifrost journey and reuse its scoped
principals, registered stable table IDs, bearer acquisition, and generated
gRPC client. A scoped bearer must drain a covered query and an uncovered query
must fail before a response stream opens. Add no production authorization path
and no new fixture abstraction.

### `FIND-admin-principals-R8-7` — malformed administrative IDs bypass the problem contract

New tenant-principal, platform-principal, platform-credential, platform-tenant,
and changed principal-revoke routes advertise path IDs as strings while Axum
extracts UUID/domain types before their handlers. A malformed value is valid
under the published schema but returns undocumented plain text.

Advertise the existing typed identifier for each changed path parameter and
map the existing Axum path rejection through the same canonical validation
problem mechanism used by the local-transfer correction. Add the reachable 400
problem and stable code to each affected operation. Add no middleware, second
parser, compatibility alias, or new error type.

### Revision 13 — delegation must not amplify authority

The current RFC 8693 exchange verifies that the caller has
`delegation:issue`, then the shared issuer signs the target principal's complete
current `PermissionSet`. A caller can therefore obtain a delegated token with
permissions the caller never held.

Keep `delegation:issue` as the entry permission, but mint the delegated token
with the semantic intersection of the verified caller token's permissions and
the target principal's current permissions. For every overlapping wildcard,
schema, or exact-object grant, retain the narrower scope; include no permission
unless both authorities cover it. Keep the existing target identity,
delegation chain, maximum depth, five-minute lifetime, no-refresh behavior, and
emit Card scope. Reuse the existing permission coverage semantics and shared
issuer; add no second RBAC engine or delegation policy layer.

### Revision 13 — delegation decisions must be durably audited

The current permission-denied branch returns before appending canonical audit,
and an allowed check followed by subject resolution or issuance failure leaves
no committed authorization decision. That violates the repository's one-row-
per-decision rule.

Every actual evaluation of `delegation:issue` must commit exactly one allowed
or denied decision to the canonical audit staging path before the response. A
successful exchange may use its existing token-exchange audit as the allowed
row, but must not add a duplicate. If permission is allowed and later subject
resolution or issuance refuses, commit the allowed no-effect decision. If the
audit append cannot commit, fail closed and issue no token. An invalid subject
token that never reaches permission evaluation creates no authorization
decision. Preserve the existing public denial and not-found errors and add no
second audit sink or best-effort fallback.

## Constraints and preserved behavior

- Preserve the five issuance entries, one concrete `TenantTokenIssuer`, the
  five-minute JWT shape, and `permissions: PermissionSet` as the sole tenant
  request authority.
- Preserve the concrete synchronous database-free `TokenVerifier`; add no
  checker, cache, epoch, introspection, factory, trait, or listener.
- Preserve admission-time Bifrost authentication and bounded completion; add
  no mid-stream token watcher or reauthentication.
- Preserve platform current-state authorization through `OperatorPool` and
  tenant data access through `TenantConn` with forced RLS.
- Preserve the canonical audit staging/publisher path and credential
  attribution. Keep `67b4d0ba`'s `FOR UPDATE NOWAIT` behavior and replay proof:
  one locked tenant must not stall publication for every tenant.
- Wyrd has not shipped: keep the new audit schema and hash encoding as the sole
  format and add no predecessor-table or predecessor-hash compatibility path.
- Preserve local-transfer problem mapping, principal-revoke no-effect audit,
  shared-client ownership, stable errors, and generated-contract ownership.
- Preserve delegation-chain representation, ordering, maximum depth, subject
  identity, no-refresh behavior, and emit Card scope; change only permission
  attenuation and canonical decision audit required by revision 13.
- Do not rewrite historical reviews, completed evidence, or immutable
  migrations merely to remove obsolete words.
- Do not add a general schema-migration system, credential-source layer, test
  harness, compatibility path, or broad verification task.

## Acceptance criteria

| Finding | Required observable result |
|---|---|
| `FIND-admin-principals-R6-1` | Every live tenant-auth contract describes the five-minute permission snapshot; protected tenant OpenAPI contains only reachable verifier errors; direct server metadata has no unused `moka`; every residual obsolete term is classified as history, unrelated platform usage, or defect |
| `FIND-admin-principals-R7-5` | Every named closure test maps one-to-one to a literal final-tree command, positive selected count, result, and owning lane, with no placeholder |
| `FIND-admin-principals-R8-2` | Tenant and platform CLI administration accept no secret-valued option, authenticate from their existing ambient/environment sources, fail when absent, and never render a supplied secret in debug output |
| `FIND-admin-principals-R8-3` | The migration test compiles and passes with no function-scoped `use` |
| `FIND-admin-principals-R8-5` | The four statements contain no redundant tenant filter while same-tenant behavior and cross-tenant invisibility remain correct through `TenantConn` |
| `FIND-admin-principals-R8-6` | One exact nonzero server-journey selector proves a scoped JWT allows the covered gRPC table and refuses an uncovered table before streaming |
| `FIND-admin-principals-R8-7` | Served OpenAPI and authenticated runtime requests agree for malformed IDs on a tenant principal, platform principal, and platform tenant path: typed schema, status, problem media, stable code, and operation-listed error |
| `REQ-012c` / `INV-013a` | Delegated JWT permissions equal the semantic caller/target intersection for wildcard, schema, exact-object, and disjoint grants; no delegated token contains authority not covered by both |
| `AC-020` | Allowed and denied `delegation:issue` evaluations each commit exactly one canonical audit row; allowed later failure retains a no-effect row; audit failure issues no token; successful issuance has no duplicate decision |

## Focused proof and broader verification

Use existing test owners and fixtures. Every specifically named test in the
implementation evidence must include its literal `mise exec -- cargo nextest
run --locked ... -E 'test(=...)'` command and positive selected count. Include
the repository-managed Postgres wrapper where required.

At minimum, prove:

- active-tree deleted-auth classification plus served OpenAPI and MCP discovery;
- CLI parsing/help, ambient tenant auth, environment platform auth, missing
  credentials, and redacted debug;
- the existing migration checksum test;
- same-tenant and cross-tenant behavior for the four corrected SQL statements;
- scoped allow/refuse through the real Bifrost gRPC query boundary; and
- malformed administrative identifiers through the assembled authenticated
  router and served OpenAPI;
- delegated permission attenuation for wildcard, schema, exact-object, and
  disjoint caller/target combinations; and
- successful, denied, allowed-then-refused, and audit-append-failed delegation
  decisions through real Postgres, proving exactly one durable row or no token.

Run the narrowest owning lanes, including:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run check:from-pools-allowlist`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:shared`
- `mise run test:sql`
- `mise run test:platform:journey` twice consecutively
- `mise run test:identity:journey`
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:journey:server`
- `mise run codegen:check`
- `mise run docs:check`
- strict rustdoc for every affected crate
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Do not substitute `mise run gate`, `test:rust`, another broad family aggregate,
or an ad hoc `--all-features` test lane. Append one evidence table mapping each
acceptance row to implementation commits, literal focused commands and selected
counts, owning lanes, and results.

## Implementation evidence

Final candidate: `20e5becad` on `claude/admin-principals-spec-qfsmjc`.
Implementation commits: `fa9a24b22` (R8-3), `dc7e6cc31` (R8-5), `71a68703d` +
`aaf753607` (R8-2), `3074983c5` (R8-7), `c2d262b3d` (R8-6), `7b9fbaad1`
(spec revision 13), `5d7353346` + `8ea8fca46` + `c62c77dba` (REQ-012c,
INV-013a, AC-020), `c39f1b6fd` + `f67024f7c` (R6-1), `a25864af2` + `20e5becad`
(strict rustdoc and `git diff --check` cleanup).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-admin-principals-R6-1` | `c39f1b6fd`: `WYRD_AUTH_401_CREDENTIAL_REVOKED` removed from protected tenant 401 descriptions (`auth/revoke.rs`, `bifrost/routes.rs`, `components/{admin,authz/check,cards,principals,storage}`, `query/routes.rs`); direct `moka` dependency removed from `wyrd-server` (OIDC JWKS cache in `wyrd-auth-oidc` retained); snapshot prose in `principal_kind.rs`, `principal.rs`, `mcp/principals.rs`, `wyrd-cli/src/principal/{mod,revoke}.rs`, `wyrd-testing/src/server.rs`, docs `cli.svx`, `authorization.svx`, `running-the-server.svx`, `object-scoped-rbac/spec.md` INV-005; schemas regenerated. `f67024f7c`: served-document and MCP proofs | `protected_tenant_operations_document_no_revocation_refusal` 1/1; `principals::pg_tests::a_write_tool_is_scoped_at_dispatch_not_merely_hidden` 1/1; `mise run codegen:check` rc=0; `mise run test:principals:integration` rc=0; `mise run test:bifrost:journey:mcp` 9/9 | PASS |
| `FIND-admin-principals-R7-5` | This table and the focused command list below: literal command, selected count and result for every named test | 16 focused commands, each `1 test run: 1 passed` on the final tree | PASS |
| `FIND-admin-principals-R8-2` | `71a68703d`, `aaf753607`: `--credential`/`--token` removed from platform and tenant administration; ambient `ClientConfig` chain for tenant commands, `WYRD_PLATFORM_CREDENTIAL` for platform commands, carried as `SecretString` | `the_platform_credential_is_not_an_argument` 1/1; `the_tenant_credential_is_not_an_argument` 1/1; `operator_journey::operator_administers_a_deployment_through_the_cli` 1/1 (missing-credential refusals); `mise run test:cli:journey` 24 passed, 5 ignored (pre-existing, gated to other lanes) | PASS |
| `FIND-admin-principals-R8-3` | `fa9a24b22`: `sha2::Digest` moved into the module-scope `pg_tests` import block | `pg_tests::shipped_audit_staging_migration_is_immutable` 1/1 | PASS |
| `FIND-admin-principals-R8-5` | `dc7e6cc31`: redundant tenant `WHERE` expressions and binds removed from the four statements in `queries/auth/{api_keys,role_assignments,service_accounts}.rs`; inserts, composite joins and RLS policies untouched | `pg_tests::tenant_principal_queries_are_confined_by_row_level_security` 1/1 (same-tenant visible, cross-tenant invisible through `TenantConn`); `mise run test:sql` 247/247; `mise run check:tenant-isolation` rc=0 | PASS |
| `FIND-admin-principals-R8-6` | `c2d262b3d`: `prove_scoped_bearer_over_grpc` extends the existing bound-server matrix with the generated gRPC client | `query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables` 1/1; `mise run test:bifrost:journey:server` 14/14 | PASS |
| `FIND-admin-principals-R8-7` | `3074983c5`: typed path parameters published; Axum `PathRejection` mapped through `http::error::path_rejection` to the canonical validation problem; 400 listed on each affected operation | `a_malformed_administrative_identifier_answers_with_a_documented_problem` 1/1 (tenant principal, platform principal, platform tenant) | PASS |
| `REQ-012c` / `INV-013a` | `5d7353346`: `PermissionSet::intersection` (narrower scope for overlaps; `AnyOf` flattened; disjoint yields nothing); `TenantGrant::Delegation { caller, ceiling }` narrows the target's resolved set to the verified caller set; `c62c77dba` boxes the caller | `permission::tests::intersection_keeps_only_the_narrower_shared_authority` 1/1 (wildcard, schema/table, disjoint table, `AnyOf`, disjoint); `exchange_api_key::pg_tests::a_delegated_token_carries_only_the_caller_and_target_intersection` 1/1; `journey_delegation_cannot_amplify_the_caller` 1/1 | PASS |
| `AC-020` | `5d7353346`: `DelegateToken::execute` owns the `TenantConn`; a denied decision appends one denied row and commits; allowed-then-refused appends one allowed no-effect row and commits; success reuses the token-exchange audit row; a store failure returns without commit and without a token | `a_denied_delegation_commits_one_denied_decision` 1/1; `an_allowed_delegation_that_refuses_later_commits_one_allowed_decision` 1/1; `an_unverifiable_subject_token_records_no_decision` 1/1; `a_refused_delegation_audit_issues_no_token` 1/1 | PASS |

### Focused commands (final tree)

| Command | Selected | Result |
|---|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(=permission::tests::intersection_keeps_only_the_narrower_shared_authority)'` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::a_denied_delegation_commits_one_denied_decision)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::an_allowed_delegation_that_refuses_later_commits_one_allowed_decision)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::an_unverifiable_subject_token_records_no_decision)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::a_delegated_token_carries_only_the_caller_and_target_intersection)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::a_refused_delegation_audit_issues_no_token)'"` | 1 | pass |
| `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test auth_e2e -E 'test(=journey_delegation_cannot_amplify_the_caller)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_openapi_contract -E 'test(=protected_tenant_operations_document_no_revocation_refusal)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test pg_openapi_contract -E 'test(=a_malformed_administrative_identifier_answers_with_a_documented_problem)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=principals::pg_tests::a_write_tool_is_scoped_at_dispatch_not_merely_hidden)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p vala-sql --test pg_migration -E 'test(=pg_tests::shipped_audit_staging_migration_is_immutable)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test pg_admin_principals -E 'test(=pg_tests::tenant_principal_queries_are_confined_by_row_level_security)'"` | 1 | pass |
| `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=query::tenant_scoped_roles_reach_only_their_granted_bifrost_tables)'"` | 1 | pass |
| `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=platform::credential::tests::the_platform_credential_is_not_an_argument)'` | 1 | pass |
| `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=principal::credential::tests::the_tenant_credential_is_not_an_argument)'` | 1 | pass |
| `WYRD_CLI_E2E=1 scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=operator_journey::operator_administers_a_deployment_through_the_cli)'"` | 1 | pass |

### Lanes

| Lane | Result |
|---|---|
| `mise run fmt:check` | rc=0 |
| `mise run lints` | rc=0 (after `c62c77dba` boxed `TenantGrant::Delegation::caller` for `clippy::large_enum_variant`) |
| `mise run check:client-tier` | rc=0 |
| `mise run check:unwrap-audit` | rc=0 |
| `mise run check:clippy-allow-audit` | rc=0 |
| `mise run check:tenant-isolation` | rc=0 |
| `mise run check:from-pools-allowlist` | rc=0 |
| `mise run test:principals:unit` | rc=0 (2, 11, 18, 4, 4 passed) |
| `mise run test:principals:integration` | rc=0 |
| `mise run test:shared` | rc=0 (664 passed) |
| `mise run test:sql` | rc=0 (123 + 4 + 118 + 2 passed) |
| `mise run test:platform:journey` ×2 consecutive | rc=0, 38/38 both runs |
| `mise run test:identity:journey` | rc=0 (20/20) |
| `mise run test:cli:journey` | rc=0 (24 passed, 5 ignored pre-existing) |
| `mise run test:bifrost:journey:mcp` | rc=0 (9/9) |
| `mise run test:bifrost:integration:server` | rc=0 (67/67) |
| `mise run test:bifrost:journey:server` | rc=0 (14/14) |
| `mise run codegen:check` | rc=0 |
| `mise run docs:check` | rc=0 |
| Strict rustdoc (`RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc --locked --no-deps --all-features -p wyrd-runtime -p wyrd-auth -p wyrd-server -p wyrd-cli -p wyrd-spec -p wyrd-sql -p wyrd-testing -p wyrd-mcp`) | rc=0 after `a25864af2` |
| `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f 20e5becad` | rc=0 after `20e5becad` |

Environment note: the local mise task shell resolves no `python` executable, so
the four Python-script tasks (`check:unwrap-audit`, `check:clippy-allow-audit`,
`check:tenant-isolation`, `docs:check`) were run with `python` resolving to
`python3` on `PATH`. The tasks themselves are unchanged.

### Residual obsolete-term classification (R6-1)

- History, retained: `changes/active/admin-principals/spec.md` deletion
  prohibitions; TASK-002/TASK-008 evidence; object-scoped-rbac task and review
  evidence.
- Correct live usage: `identity_e2e.rs` comments state tokens are "not
  introspected"; "ends renewal immediately" in `authentication.svx` and
  `identity-and-auth.svx` describe issuance, not request verification.
- Unrelated: `wyrd-sql` column introspection; `reader_pins.rs` "stop
  authorizing source IO"; `architecture/wyrd-design.md` and verified-change
  NOTIFY channels; the `wyrd-client` facade "introspection door".
- Defects remaining: none.

### Non-goals confirmed

- `FIND-admin-principals-R8-1` rejected: the `67b4d0ba` `FOR UPDATE NOWAIT`
  behavior and its replay proof are unchanged.
- `FIND-admin-principals-R8-4` rejected: no audit compatibility migration or
  predecessor-hash path was added.
- No checker, cache, epoch, introspection, credential-source layer, second
  client builder, test harness, or second audit sink was added.

### Follow-up (out of scope)

The pre-existing `wyrd auth trusted-issuer`, `wyrd auth workload-binding`, and
`wyrd auth refresh` commands still accept bearer or refresh tokens as argv
options (`--token`, `--refresh-token`, both with environment fallbacks). R8-2
covered only the new administration commands.
