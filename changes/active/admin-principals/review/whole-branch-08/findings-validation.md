# Whole-branch 08 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate and reviewed HEAD:
  `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Approved specification: `changes/active/admin-principals/spec.md`, revision
  12
- Original delivery: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior verdict, ledger, and remediation:
  `changes/active/admin-principals/review/whole-branch-07/`

I read the complete cumulative diff, every Wave 1 report, the final source, and
the applicable authority. CodeGraph was used before direct source inspection to
trace tenant issuance, delegation, Bifrost admission and authorization, and
audit publication. Proposed corrections below reuse existing owners and add no
new policy engine, auth layer, parser, cache, or test harness.

## Wave 1 proposal disposition

| Wave 1 proposal | Disposition | Reason |
|---|---|---|
| `TREV-WB08-1` — obsolete epoch/NOTIFY prose | **REVISED / CONSOLIDATED** | The reported source rustdoc is stale and reachable, and the same incomplete deletion appears in live docs, MCP/CLI text, route error inventories, active dependent specifications, and the unused direct server cache dependency. Retained as stable `FIND-admin-principals-R6-1` with `CONTRACT-08-01` and `AUTH-R8-3`. Historical review packets, completed task evidence, and immutable migrations remain history rather than correction targets. |
| `TREV-WB08-2` — exact focused evidence absent | **CONFIRMED** | The R7 packet still records one template and the placeholder `N tests run: N passed`, not each literal command and selected count required by `VER-002` and the prior finding. Retained as stable `FIND-admin-principals-R7-5`. |
| `TREV-WB08-3` — audit-publisher lock-policy drift | **CONFIRMED** | Commit `67b4d0ba` changes audit publication concurrency and rewrites its replay journey; the packet itself labels it out of task, and `VER-005` excludes that repair. Retained as `FIND-admin-principals-R8-1`. |
| `REPO-R8-1` — CLI secrets in argv/plain `String` | **CONFIRMED** | Both endpoint structs are new, accept secrets through process arguments, and derive secret-revealing `Debug`, contrary to existing security authority. Retained as `FIND-admin-principals-R8-2`. The TASK-008 implementation suggestion to pass `--token` cannot override that pre-existing repository rule. |
| `REPO-R8-2` — function-scoped import | **CONFIRMED** | The candidate added `use sha2::Digest` inside a test despite the explicit module-scope import rule. Retained as `FIND-admin-principals-R8-3`. |
| `AUTH-R8-1` — delegation can widen authority | **REJECTED** | The exact behavior predates the base: the base implementation checked only `delegation:issue` and minted the target's complete roles. The approved specification expressly excludes changing delegation chains, while R7 only routes the existing grant through the shared issuer. This is real pre-existing security debt, but not a regression or unmet approved task outcome; track it as a separate security change. |
| `AUTH-R8-2` — denied delegation is not durably audited | **REJECTED** | The immediate unaudited denial is byte-for-byte equivalent to the base path and is outside `REQ-037`'s newly built administrative operations. The global audit rule makes it separate pre-existing debt, but this acceptance audit may not pull an unrelated token-exchange redesign into R7. |
| `AUTH-R8-3` — unused server `moka` dependency | **REVISED / CONSOLIDATED** | The direct dependency has no server caller and R7 expressly required the removed verifier-cache dependencies to go. Consolidated into stable `FIND-admin-principals-R6-1`; the live OIDC cache dependency remains. |
| `DATA-WB08-01` — retained audit history cannot upgrade | **CONFIRMED** | The candidate changes both the built-in Iceberg schema fingerprint and the hash preimage without an evolution path. Existing retained tables fail `ensure_builtin`, and old credential-free hashes cannot be reproduced by the new unconditional encoding. Retained as `FIND-admin-principals-R8-4`. |
| `DATA-WB08-02` — tenant SQL duplicates RLS | **CONFIRMED** | The four new statements run only through `TenantConn`; their extra tenant `WHERE` expressions are expressly prohibited parallel boundaries, not required inserts or composite joins. Retained as `FIND-admin-principals-R8-5`. |
| `BIFROST-R8-1` — scoped authority not proved over gRPC | **CONFIRMED** | R7 explicitly requires the exact/schema table matrix through real Bifrost gRPC serving. The scoped matrix uses the SDK's HTTP `/v1/query` path; the only direct gRPC test carries global authority. Retained as `FIND-admin-principals-R8-6`. |
| `CONTRACT-08-01` — live contracts describe deleted auth | **REVISED / CONSOLIDATED** | Confirmed and consolidated with `TREV-WB08-1` and `AUTH-R8-3` as stable `FIND-admin-principals-R6-1`. |
| `CONTRACT-08-02` — malformed admin UUID paths bypass the public problem contract | **CONFIRMED** | The new routes advertise `String`, extract UUID/domain types before the handler, and therefore return Axum plain text for malformed values. Retained as `FIND-admin-principals-R8-7`. |

The rejected delegation proposals are not softened into optional work in this
task. They are omitted from the final ledger.

## Final deduplicated finding ledger

### `FIND-admin-principals-R6-1` — REVISED — VIOLATION — deletion of the rejected tenant-auth design is incomplete

- **Wave 1 sources:** `TREV-WB08-1`, `CONTRACT-08-01`, `AUTH-R8-3`.
- **Violated obligation:** `REQ-012`, `REQ-012a`, `INV-013`, `AC-010`,
  `AC-013`, `R7-AUTH-7`, and stable `FIND-admin-principals-R6-1` require one
  local five-minute permission-snapshot model and deletion of its epoch,
  request-time resolver, immediate-revocation, and verifier-cache contracts.
- **Exact evidence:**
  `crates/wyrd-spec/src/auth/principal_kind.rs:7-8,20-21` still names the
  deleted NOTIFY/epoch mechanisms;
  `crates/shared/wyrd-runtime/src/principal.rs:27` says permissions resolve at
  verify time; `components/auth/token_extract.rs:51` describes choosing a
  tenant verifier; `wyrd-server/src/mcp/principals.rs:129-136`,
  `wyrd-cli/src/principal/{mod.rs,revoke.rs}`, and active docs still promise
  immediate invalidation; active dependent specifications still prescribe
  epochs; tenant protected-route annotations advertise
  `WYRD_AUTH_401_CREDENTIAL_REVOKED` although the local verifier cannot emit
  it; and `crates/wyrd/wyrd-server/Cargo.toml:54` retains an unused direct
  `moka` dependency.
- **Reachability and consequence:** rustdoc, CLI help, MCP discovery, published
  docs, `/openapi.json`, active design inputs, and the server dependency graph
  are live surfaces. They tell operators and future implementers that existing
  tenant JWTs are immediately revoked or database-resolved even though they
  intentionally remain valid until expiry, and expose an unreachable route
  error.
- **Decision-complete minimum correction:** update the existing live prose in
  place to say that issuance reads current state once, `permissions` is the
  authority, local verification is database-free, lifecycle changes prevent
  new issuance immediately, and already-issued tenant JWTs lapse within five
  minutes. Reconcile the stale active dependent specifications, but leave
  historical reviews, completed task evidence, and immutable migrations
  unchanged and classified as history. Remove
  `WYRD_AUTH_401_CREDENTIAL_REVOKED` only from protected tenant operations
  whose verifier cannot produce it, preserving the error on issuance or
  platform paths where it remains reachable. Remove only the unused direct
  `wyrd-server` `moka` dependency; retain the live OIDC JWKS cache dependency.
- **Focused closure proof:** a cumulative active-tree audit classifies every
  residual obsolete term as live defect, unrelated platform concept, or
  historical record; served OpenAPI tests prove protected tenant routes list
  only concrete verifier 401 outcomes; MCP discovery pins the five-minute
  description; `cargo metadata` shows no direct server `moka`; run the existing
  docs, codegen, lint, and auth verification lanes.

### `FIND-admin-principals-R7-5` — CONFIRMED — MISSING — exact focused-test evidence is still absent

- **Wave 1 source:** `TREV-WB08-2`.
- **Violated obligation:** `VER-002`, stable
  `FIND-admin-principals-R7-5`, and the R7 task require every specifically
  named Rust test to record its exact nonzero selector and owning result.
- **Exact evidence:**
  `whole-branch-07/TASK-001-008-R7-close-validated-findings.md:299-318`
  contains one parameterized command template and the literal placeholder
  `N tests run: N passed`; it does not preserve a literal command or selected
  count per named test.
- **Observable consequence:** the packet cannot establish that each final-tree
  command selected exactly the named nonzero test instead of zero or a broader
  expression.
- **Decision-complete minimum correction:** run no new test harness and add no
  test. Append, for every named closure test, the literal final-candidate
  `mise exec -- cargo nextest run --locked ... -E 'test(=...)'` command, its
  nonzero selected count, pass result, and owning lane; use the existing
  environment-owning wrapper where required.
- **Focused closure proof:** the evidence table itself contains no placeholder
  and maps every named test one-to-one to a literal command, positive selected
  count, result, and lane.

### `FIND-admin-principals-R8-1` — CONFIRMED — DRIFT — the candidate includes an unrelated audit-publisher concurrency change

- **Wave 1 source:** `TREV-WB08-3`.
- **Violated obligation:** `VER-001`, `VER-005`, R7's bounded remediation
  outcome, and the review requirement that unrelated work not enter the
  candidate.
- **Exact evidence:** commit `67b4d0ba` changes
  `freeze_publication_range` from blocking `FOR UPDATE` to `FOR UPDATE NOWAIT`
  at `vala-sql/src/queries/audit_staging.rs:199-249` and rewrites the owning
  replay journey. The implementation packet calls it an out-of-task fix.
- **Observable consequence:** accepting this auth task would also approve a
  separate audit-publication concurrency policy and proof rewrite that was not
  part of the approved change.
- **Decision-complete minimum correction:** remove only commit `67b4d0ba`'s
  production and test changes from this candidate, restoring the pre-existing
  publisher lock semantics and journey. Do not redesign or repair the publisher
  here; carry any recurring failure into its own change.
- **Focused closure proof:** the cumulative diff contains no `NOWAIT` or replay
  rewrite attributable to `67b4d0ba`; the auth task's required evidence remains
  unchanged.

### `FIND-admin-principals-R8-2` — CONFIRMED — VIOLATION — new CLI administration commands accept and expose secrets as ordinary arguments

- **Wave 1 source:** `REPO-R8-1`.
- **Violated obligation:** AGENTS.md secret handling and
  `architecture/wyrd-security-posture.md` require `SecretString` with redacted
  debug and prohibit credentials in command arguments.
- **Exact evidence:** `wyrd-cli/src/platform/credential.rs:30-39` and
  `wyrd-cli/src/principal/credential.rs:29-38` derive `Debug`, hold credentials
  as `String`, and expose `--credential` or `--token`; `wyrd-cli/src/client.rs`
  then threads the plaintext as `&str` before wrapping it.
- **Observable consequence:** credentials enter shell history/process argv and
  may appear in derived debug output.
- **Decision-complete minimum correction:** retain `--server`, but delete the
  two new secret-valued command options. Resolve tenant administration through
  the existing ambient `ClientConfig` credential chain and platform
  administration from `WYRD_PLATFORM_CREDENTIAL`; carry the resolved values as
  `SecretString` and ensure any containing debug output is redacted. Do not add
  a credential-source abstraction or a second client builder.
- **Focused closure proof:** CLI journey tests invoke both command families with
  their documented environment/config credentials, prove missing credentials
  fail, and parser/help tests prove neither secret-valued option exists; a
  debug assertion proves no supplied secret is rendered.

### `FIND-admin-principals-R8-3` — CONFIRMED — VIOLATION — a candidate-added import is hidden inside a test function

- **Wave 1 source:** `REPO-R8-2`.
- **Violated obligation:** `architecture/agent-rules.md:10` requires imports at
  module scope; neither narrow exception applies.
- **Exact evidence:**
  `crates/vala/vala-sql/tests/pg_migration.rs:134` imports `sha2::Digest`
  inside `shipped_audit_staging_migration_is_immutable`.
- **Observable consequence:** the module dependency is hidden from its import
  block and establishes the exact local-import pattern the rule bans.
- **Decision-complete minimum correction:** move only `sha2::Digest` to the
  existing `pg_tests` module import block; add no alias, wrapper, or new test.
- **Focused closure proof:** the existing migration checksum test compiles and
  passes, and the file has no function-scoped `use`.

### `FIND-admin-principals-R8-4` — CONFIRMED — REGRESSION — credential attribution strands existing retained audit history

- **Wave 1 source:** `DATA-WB08-01`.
- **Violated obligation:** `REQ-037`, the canonical durable audit-history
  contract, and upgrade/recovery guarantees require existing history to remain
  publishable and hash-verifiable.
- **Exact evidence:** `tables/audit/audit_log.rs:37-63` appends a fourteenth
  content column; `BifrostCatalog::ensure_builtin` rejects an existing
  predecessor fingerprint at `catalog/bifrost_catalog.rs:977-980`; and
  `vala-sql/src/queries/audit_staging.rs:361-384` unconditionally appends an
  optional credential segment even when absent. The SQL migration test upgrades
  only Postgres staging, not an existing retained Iceberg table or old hashes.
- **Reachability and consequence:** any tenant with a pre-candidate
  `vala.system.audit_log` reaches `ensure_builtin` on its next publication,
  receives `FingerprintMismatch`, retains the owed frozen range, and repeats.
  Old credential-free rows also used a shorter hash preimage than the new
  verifier can infer from their stored null column.
- **Decision-complete minimum correction:** specialize the existing built-in
  owner for exactly one recognized predecessor audit fingerprint: use the
  installed Iceberg schema-evolution mechanism to append nullable
  `credential_id`, then atomically advance the catalog fingerprint. Reject every
  other mismatch as today; do not add a generic migration framework or accept
  arbitrary drift. Preserve the predecessor hash encoding whenever
  `credential_id` is absent and append the new segment only when it is present,
  so all old and new null rows share one reproducible preimage.
- **Focused closure proof:** create the predecessor retained table and old
  hash-chained rows, upgrade, publish both an old credential-free row and a new
  credential-attributed decision, query both from one history, and reproduce
  every stored hash without a fingerprint conflict. Run the owning migration,
  SQL, and Bifrost audit-publication lanes.

### `FIND-admin-principals-R8-5` — CONFIRMED — VIOLATION — four new tenant queries duplicate the RLS boundary

- **Wave 1 source:** `DATA-WB08-02`.
- **Violated obligation:** `REQ-031`, `AC-010`, AGENTS.md, and
  `architecture/agent-rules.md` require `TenantConn` plus forced RLS to be the
  sole tenant filter and prohibit parallel hand-written predicates.
- **Exact evidence:** candidate-added predicates appear in
  `wyrd-sql/src/queries/auth/api_keys.rs:64-79,93-108`,
  `role_assignments.rs:34-50`, and
  `service_accounts.rs:231-247`; their served credential, OIDC role, provisioning,
  and recovery callers all pass `TenantConn`.
- **Observable consequence:** tenant isolation on these paths has two
  independently maintained expressions even though RLS already supplies the
  authoritative one.
- **Decision-complete minimum correction:** remove only the redundant tenant
  `WHERE` predicates and now-unused tenant binds from those four statements.
  Preserve inserted tenant keys, tenant-qualified composite joins, principal
  and credential predicates, and all transaction ownership.
- **Focused closure proof:** focused SQL tests prove same-tenant behavior and
  cross-tenant invisibility through `TenantConn`; run
  `check:tenant-isolation`, `test:sql`, and the owning principal and identity
  journeys.

### `FIND-admin-principals-R8-6` — CONFIRMED — MISSING — the required scoped-permission proof does not use the real gRPC query boundary

- **Wave 1 source:** `BIFROST-R8-1`.
- **Violated obligation:** R7 focused proof explicitly requires an exact-table
  or schema-scoped allow/refuse matrix through real Bifrost gRPC serving.
- **Exact evidence:**
  `wyrd-testing/tests/bifrost/server/query.rs:928-1145` builds scoped tokens but
  calls `Bifrost::query_only`, which uses HTTP `/v1/query`; the direct gRPC test
  at `:1195-1270` proves expiry with `PermissionScope::All`, not scoped table
  authority.
- **Observable consequence:** the HTTP path proves the production Oracle
  decision, but the mandated gRPC adapter-to-Oracle propagation of scoped JWT
  permissions remains unproved.
- **Decision-complete minimum correction:** extend the existing bound-server
  Bifrost journey, reusing its scoped principals, registered table UIDs, raw
  bearer acquisition, and generated `BifrostQueryServiceClient`. Present one
  schema- or exact-table bearer in gRPC metadata, drain its covered query, and
  require an uncovered query to fail before a response stream opens. Add no
  production authorization path or fixture abstraction.
- **Focused closure proof:** one exact nonzero server-journey selector exercises
  both the covered and uncovered gRPC requests and records the exact command,
  selected count, result, and owning `test:bifrost:journey:server` lane.

### `FIND-admin-principals-R8-7` — CONFIRMED — INCORRECT — malformed administrative path identifiers bypass the published problem contract

- **Wave 1 source:** `CONTRACT-08-02`.
- **Violated obligation:** `REQ-036`, `REQ-049`, `AC-014`, `AC-019`, and
  AGENTS.md require typed OpenAPI parameters and reachable public failures
  through the canonical Wyrd problem mapper.
- **Exact evidence:** the new principal, platform-principal, platform-credential,
  and platform-tenant operations in
  `components/principals/routes.rs:348-504`,
  `components/platform/{routes.rs:288-343,credentials.rs:64-204,identity.rs:570-592}`
  advertise path IDs as `String` while handlers directly extract `Uuid` or
  `DataTenantId`. The changed principal revoke route has the same mismatch at
  `auth/revoke.rs:50-77`.
- **Reachability and consequence:** an authenticated request containing
  `not-a-uuid` is accepted by the advertised schema, rejected by Axum before
  the handler, and returned as undocumented plain text rather than a stable
  `application/problem+json` response.
- **Decision-complete minimum correction:** advertise the existing typed
  identifier for each changed path parameter and receive Axum's existing path
  rejection at those route boundaries, mapping it through the same canonical
  validation-problem mechanism already used by the local-transfer correction.
  Add the reachable 400 problem/code to each affected operation. Add no
  middleware, second parser, alias, or error type.
- **Focused closure proof:** served-router contract tests cover one tenant
  principal path, one platform principal path, and one platform tenant path,
  asserting the typed OpenAPI parameter and malformed runtime response status,
  problem media, stable code, and operation-listed error.

## Validation result

Nine material findings survive: stable
`FIND-admin-principals-R6-1`, stable `FIND-admin-principals-R7-5`, and new
`FIND-admin-principals-R8-1` through `FIND-admin-principals-R8-7`.

Each retained correction is bounded by existing approved behavior. None needs a
new product surface, auth architecture, policy engine, compatibility route, or
general migration framework. The retained-audit correction fixes one concrete
predecessor schema and hash encoding rather than introducing generic schema
evolution.

HEAD remained `eb9b2f69cb883fa508ed168f21cb868451e61b82` after validation.
