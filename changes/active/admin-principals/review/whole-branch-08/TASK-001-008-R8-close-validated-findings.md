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
