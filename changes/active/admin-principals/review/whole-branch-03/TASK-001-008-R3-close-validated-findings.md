# TASK-001-008-R3 — Close validated admin-principals findings

## Route

Implement this packet with `$wyrd-implement`. Make the smallest cohesive
correction that closes every finding below. A later `$wyrd-task-review` must
review the complete cumulative candidate, not only this remediation diff.

## Authority and reviewed candidate

- Approved specification: `changes/active/admin-principals/spec.md`, revision 10.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-03/findings-validation.md`.

All of `FIND-TASK-001-10` is waived. Do not rewrite history, remove AI trailers,
or change author/committer identities. The verified-change-contract commits are
approved cumulative content; preserve them and do not classify them as drift.

## Resume handoff

Resume from the existing dirty worktree; do not reset, stash, or overwrite it.
Preserve the completed commits `64327074b` (R3-4), `fef734235` (R3-2), and
`e22f3266e` (constraint leak), and preserve the uncommitted revision-10 spec,
verdict, and remediation edits.

The two dirty Rust files are an interrupted refresh implementation:

- `crates/wyrd/wyrd-server/src/components/auth/routes.rs` contains the required
  R2-4 replay containment. Finish it by importing `RefreshError` through the
  existing `crate::auth::refresh` re-export, then add the cross-transaction
  replay proof before committing it.
- `crates/wyrd/wyrd-auth/src/refresh.rs` contains useful consumed-row
  attribution, but its machine-principal rotation path follows the superseded
  task. Do not commit it as-is. Preserve the attribution when completing User
  refresh rotation, remove Service/Agent/TenantAdmin refresh issuance and
  rotation as required by R2-3, and leave `wyrd-client` on durable-credential
  re-exchange.

Make those two files compile and close R2-3/R2-4 first. Then continue through
every still-open acceptance row in this packet, beginning with the exact
OpenAPI keep/delete list; do not assume the interrupted status report is proof.
Run the focused tests for each checkpoint and the full required proof list only
after all findings are implemented.

## Outcome

Finish the approved administration capability by tightening its existing SQL,
audit, OIDC, secret, CLI, documentation, and retained-audit owners; split
machine renewal from human-session refresh; and reduce OpenAPI to one exact
`utoipa` runtime contract available at `/openapi.json`.
Reuse the existing mechanisms named below. Do not add a parallel audit path,
pool wrapper, error catalog, refresh service, secret wrapper, second
API-description framework, transport, or test harness.

The owner requires `utoipa`, its typed route/schema annotations, the runtime
`WyrdApiDoc`, `/openapi.json`, focused contract tests, and user documentation.
Delete the duplicate checked-in/YAML/generation machinery named below; do not
delete the runtime contract, documentation, SDK declarations, or client-tier
boundary. The owner has also selected one renewal model per caller: machine
clients re-exchange their durable credential; only human OIDC sessions receive
and rotate refresh tokens. The eventual UI will automate human refresh through
an HTTP-only server/BFF session; this packet prepares the server contract but
does not implement UI state or browser storage.

## Validated diagnosis and required correction

### `FIND-admin-principals-2` — broad database capability remains in live owners

`components/platform/provisioning.rs:93-116` and
`components/platform/recovery.rs:36-58` retain `WyrdPostgres`, contrary to the
mandatory `TenantConn`/`OperatorPool` boundary and the previous remediation.
Both served workflows can consequently open arbitrary tenant transactions.

Keep pool composition and tenant acquisition at the existing server/route
boundary. Give provisioning and recovery only the acquired `TenantConn` for
tenant work and the existing `OperatorPool` for platform work. Extend the
current boundary check to these fields and constructors; add no wrapper or
provider trait.

### `FIND-admin-principals-3` — public conflicts reveal database constraints

`components/admin/routes.rs:699-705,721-734` inserts
`SqlError::{UniqueViolation,FkViolation}.constraint` into public messages and
details. A physical rename therefore changes the API and authenticated callers
learn internal schema identifiers.

Keep the existing 409 variants and mappers, but make their response text static
and operation-specific. Emit the constraint only through structured server
tracing. Prove duplicate and referenced-delete responses preserve
`WYRD_AUTH_409_ADMIN_CONFLICT` without the sentinel physical name.

### `FIND-admin-principals-13` — keep one runtime OpenAPI contract and delete its duplicates

The required `utoipa` document is incomplete. Its stable-code validation skips
the Platform and Principals tags and uses weak substring matching. The
`/auth/token` operation omits reachable refresh-reuse, refresh-revocation,
delegation-depth, and missing-principal errors; administrative response DTOs
still expose untyped `serde_json::Value` fields; and revocation prose describes
an ordering the implementation does not provide.

Keep exactly these OpenAPI surfaces:

- the workspace and server `utoipa` dependency, without the YAML feature;
- the existing `wyrd-spec` and `wyrd-semver` optional server features,
  `ToSchema` implementations/derives, handler `#[utoipa::path]` annotations,
  `http::openapi::WyrdApiDoc`, `SecurityAddon`, and `ProblemMediaAddon`;
- the runtime `GET /openapi.json` endpoint and focused tests that prove its
  routes, authentication, typed bodies, problem media, and stable errors;
- the OpenAPI user-documentation page, API/reference navigation, and LLM index
  links, rewritten to describe `/openapi.json` rather than a repository file;
- `scripts/checks/client-tier.sh` and `mise run check:client-tier`, which keep
  server-only dependencies out of client-tier crates; and
- JSON Schema generation, the error catalog, MCP verification, required Python
  `.pyi` declarations, required TypeScript `.d.ts` declarations, and all of
  their existing code-generation/type-check tasks.

Delete exactly these duplicate or unimplemented surfaces:

1. Delete the checked-in repository-root `openapi.yaml`.
2. Delete `crates/wyrd/wyrd-server/examples/gen_openapi.rs`.
3. Delete only the `/openapi.yaml` route and YAML serialization branch from
   `crates/wyrd/wyrd-server/src/http/router.rs`; retain `/openapi.json`.
4. Remove `serde_yaml` from `crates/wyrd/wyrd-server/Cargo.toml` if the route
   deletion leaves no server use. Remove the `yaml` feature from the workspace
   and server `utoipa` declarations; retain `chrono`, `uuid`, and `utoipa`
   itself. Update `Cargo.lock` only as required by those manifest changes.
5. Delete the `codegen:openapi` task from `mise.toml`. From `codegen:regen`,
   remove only its `mise run codegen:openapi` call and `openapi.yaml` existence
   assertion. From `codegen:check`, remove only `openapi.yaml` from the snapshot
   set. Rewrite the affected task descriptions/comments; retain schema, MCP,
   Python-stub, TypeScript-declaration, and error-code generation/checks.
6. In `docs/scripts/generate_api_docs.py`, delete `OPENAPI_PATH`, `yaml_value`,
   `route_lines`, and all reading/parsing of `openapi.yaml`. Keep
   `render_openapi`, but reduce it to the maintained user page explaining the
   canonical `GET /openapi.json` endpoint and how Swagger-compatible tooling
   consumes it. Regenerate `docs/src/content/docs/api/openapi.md`; keep that
   page, `docs/src/content/docs/api/index.svx`, reference navigation, and both
   public LLM indexes.
7. In `architecture/operations/deployment-and-release.md`, remove only
   `OpenAPI/` from `public OpenAPI/schema/stub digest`. Do not add a replacement
   digest or release-manifest implementation; no such consumer exists.
8. Update only now-stale generated-artifact instructions in `AGENTS.md`,
   `architecture/agent-rules.md`,
   `architecture/references/languages/agent-harness.md`, and
   `architecture/references/languages/testing-workflows.md`: OpenAPI changes run
   the focused `wyrd-server` contract tests, while `codegen:check` continues to
   own checked-in schemas and language declarations. Keep all broader OpenAPI
   architecture and user documentation that describes the live contract.

Within the retained runtime document, make every administrative operation
declare its exact route, authentication, typed request/response bodies, problem
media type, and reachable stable error codes. Replace response-side
`serde_json::Value` fields with the already-existing concrete claim,
collection, and `CardRef` types, and correct the stale revocation prose. Do not
add another API-description framework, route catalog, error catalog, schema
owner, generated file, or check.

### `FIND-005-1` — issue-key allowance commits before its effect

`components/auth/routes.rs:365-393` calls standalone `record_audit`, commits the
allowed decision, and only then opens the transaction that issues and audits
the key. Later failure leaves a durable allowance for a nonexistent effect.

Acquire one existing `TenantConn`, append the returned allowance with the
canonical `audit::append_on`, issue the key and issuance evidence, then commit
once. Preserve independently durable denials. Prove a post-authorization
failure commits neither key nor allowance.

### `FIND-004-5` — operator configuration journey is incomplete

`wyrd-cli/tests/operator_journey.rs:63-117` creates a tenant and jumps to
principal creation without using the shipped tenant trusted-issuer/workload
configuration command. The operator page mirrors the omission and its command
synopsis claims only `init` exists despite the shipped `recover-root` action.

Extend the existing real-server CLI journey and the existing operator page:
tenant create -> tenant OIDC configuration -> restricted principal creation
and use. Add `recover-root` to the current synopsis. Do not add platform-human
identity CLI commands, another page, or another transport owner.

### `FIND-admin-principals-R2-3` — put refresh on the human-session path only

The server currently mints refresh tokens during API-key exchange even though
`wyrd-client` intentionally discards them and re-exchanges the API key. At the
same time, the OIDC callback already issues a User refresh token but
`refresh.rs` leaves User rotation as `todo!`. The implemented machinery is
therefore attached to clients that do not use it and absent where the eventual
UI needs it.

Make API-key and workload grants return `refresh_token: None`; workload exchange
already follows this rule. Remove refresh-row creation and refresh rotation for
Service, Agent, and TenantAdmin machine/API-key sessions. Keep the existing
optional response field because human OIDC exchange uses it. Complete the
existing User rotation path by re-reading current user roles, rotating through
the same refresh-family owner, and placing the consumed row id in both the
rotation audit and successor access-token context. Federated login itself has
no credential id; a request authenticated by its stored refresh row does.

Do not add refresh-token storage or a refresh grant to `wyrd-client`. Preserve
its current 30-second stale window, durable API-key/workload re-exchange, and
single `401` retry. Automatic human refresh belongs to the later UI's
server/BFF session, not the machine SDK.

### `FIND-admin-principals-R2-4` — human refresh replay containment rolls back

`wyrd-auth/src/refresh.rs:179-218` stages family revocation and its canonical
audit, then returns `RefreshError::Reused`. The route commits only `Ok`, so the
error path drops the transaction while returning `WYRD_AUTH_401_REFRESH_REUSED`.
An attacker's successor remains usable.

At the existing route boundary, commit the already-staged transaction only for
the typed `Reused` result before rendering the existing 401. All other errors
retain current rollback behavior. Through the human OIDC/session flow, rotate,
replay, and then prove the successor is refused and exactly one committed
family-revocation event is visible from another transaction.

### `FIND-admin-principals-R3-1` — platform audit names the wrong resource

`platform_authz.rs:104-110,152-172` accepts only an optional tenant and otherwise
records literal `platform`. Identity and credential callers therefore lose
their target. Provisioning audits a proposed id before
`insert_provisioning_tenant` can return a resumed tenant, leaving the committed
row attached to a discarded id.

Pass the exact resource already known by every caller into the existing
authorization owner. Use stable target spellings for connection, principal,
credential, and status operations; use the requested tenant slug for create so
fresh and resumed attempts remain truthful. Keep `target_tenant_id` separate
when the final tenant already exists. Do not mutate staged hash-chain rows or
add a metadata channel.

### `FIND-admin-principals-R3-2` — platform issuer equality is noncanonical

`components/platform/identity.rs:206-246` parses and normalizes an issuer but
persists the raw request. Registration copies that raw value, while login
reparses and searches with the normalized value. A valid trailing slash can
therefore make first login impossible.

Persist, return, and reuse the already-parsed `issuer.as_str()` for both the
connection and identity. Add no normalizer. Prove the served trailing-slash
configuration, preregistration, and first-login pinning path.

### `FIND-admin-principals-R3-3` — tenant issuer secrets are debug-visible

`wyrd-spec/src/auth/admin.rs:65-80` and
`wyrd-cli/src/auth/trusted_issuer.rs:28-50,252-278` retain client secrets as
ordinary strings in debug-derived structures. The repository already provides
`wyrd-spec/src/auth/secret_bearer.rs` for the redacted write-only wire shape.

Reuse `SecretBearer` on the wire and `SecretString` after CLI resolution, with
redacted debug behavior for the argument holder. Keep JSON compatibility as a
string and preserve the existing OpenAPI write-only/password schema. Do not
introduce another secret type or schema helper.

### `FIND-admin-principals-R3-4` — mandatory Rust contracts are incomplete

`wyrd-server/src/main.rs:90-105` adds fallible `recover_root` without an
`# Errors` section. New helpers in `platform_admin_e2e.rs:172-187,3455-3503`
panic through `expect` but omit required `# Panics` sections.

Document the actual failure and panic contracts on the existing items. No
helper, lint suppression, or test is required.

### `FIND-admin-principals-R3-5` — audit credential upgrade breaks history

Adding nullable `credential_id` changes the registered retained
`audit_log` fingerprint. Every publication calls `ensure_builtin`, which
rejects the old registration; migration 27 evolves only Postgres staging. The
same change inserts a zero byte into the hash preimage for every historical
null-credential row, contradicting the migration's reproducibility claim.

Keep the one retained table and publisher. In the existing catalog owner,
recognize only the exact prior `audit_log` fingerprint and perform one
idempotent additive evolution that appends nullable `credential_id`, preserves
all old Iceberg field ids, and advances the control fingerprint. Reconcile the
retry state where the physical schema evolved but control metadata did not;
continue rejecting every unrelated fingerprint mismatch. For hashing, retain
the legacy preimage whenever credential id is absent and append the credential
segment only when present. Do not add a general migration framework, hash
version column, parallel table, or history rewrite.

Seed an exact pre-change physical table, catalog registration, staging row,
chain head, and retained row; upgrade; verify the old hash; publish null and
non-null rows; restart/replay; and read one uninterrupted history with preserved
old field ids.

## Constraints and non-goals

- Preserve the two administrative planes, closed principal kinds, Card-free
  administrative principals, existing permission vocabulary, and canonical
  audit staging/publisher path.
- Preserve independently durable denied decisions and atomic same-plane allowed
  decisions/effects.
- Preserve platform and tenant isolation, human refresh rotation, machine
  credential re-exchange, fixed-cost API-key verification, last-admin
  protection, idempotent provisioning, and root recovery.
- Do not alter commit provenance or approved verified-change-contract content.
- Do not broaden CLI scope beyond the already approved operator journey.
- Preserve `utoipa`, `/openapi.json`, focused runtime contract tests, and the
  OpenAPI documentation page. Delete only the eight explicitly enumerated
  duplicate/unimplemented surfaces above; do not widen deletion to repository
  documentation, SDK declarations, JSON Schemas, MCP, or the client-tier gate.
- Do not widen generic built-in schema evolution or weaken fingerprint checks.
- Do not solve the excluded base-red auth failure, Card-name handoff, stale
  testing comment, or rustdoc-lane policy decision in this packet.

## Acceptance criteria

| Finding | Acceptance |
|---|---|
| `FIND-admin-principals-2` | Live provisioning/recovery owners contain only `TenantConn`/`OperatorPool` capabilities, enforced by the existing boundary check. |
| `FIND-admin-principals-3` | Admin conflict responses retain stable 409 codes and contain no physical constraint identifier. |
| `FIND-admin-principals-13` | `/openapi.json` serves the exact `utoipa` contract; the checked-in YAML, YAML endpoint/feature/dependency, emitter, OpenAPI codegen/drift wiring, snapshot parser, and release OpenAPI digest are absent; the documentation page, client-tier gate, JSON Schemas, Python `.pyi`, and TypeScript `.d.ts` surfaces remain. |
| `FIND-005-1` | Issue-key effect and allowance commit or roll back together; denial remains durable. |
| `FIND-004-5` | The real CLI journey performs tenant configuration before restricted-principal creation/use, and operator docs consistently include recovery. |
| `FIND-admin-principals-R2-3` | Machine grants return no refresh token and continue automatic durable-credential re-exchange; a human User refresh rotates and attributes the consumed refresh-row id. |
| `FIND-admin-principals-R2-4` | Human refresh reuse returns the existing 401 after durably revoking the family once; the successor cannot rotate. |
| `FIND-admin-principals-R3-1` | Every reviewed platform operation records its exact resource; resumed provisioning never records the discarded proposal id. |
| `FIND-admin-principals-R3-2` | Trailing-slash platform issuer configuration completes first-login pinning. |
| `FIND-admin-principals-R3-3` | Request/CLI debug cannot reveal a sentinel secret, JSON remains a string, and its existing OpenAPI schema remains write-only with password format. |
| `FIND-admin-principals-R3-4` | Every named new fallible or panicking item has the required accurate rustdoc section. |
| `FIND-admin-principals-R3-5` | A pre-change audit table/history upgrades, verifies, publishes both credential shapes, replays, and preserves old field ids. |

## Required proof

Add or tighten the focused tests named in each diagnosis, using the existing
fixtures and journeys. Every specifically named Rust test must be run with an
exact package, target, and `test(=...)` expression through `mise exec --`, with
the repository Postgres wrapper where required.

The renewal proof must be end to end, not only a rotation unit test:

- API-key and workload exchanges return no refresh token and create no refresh
  row;
- the existing `wyrd-client` stale-token and reactive-401 tests prove one
  durable-credential re-exchange and one request retry;
- a human OIDC session obtains access plus refresh, uses the access token,
  reaches access expiry, rotates the refresh token, and succeeds on the
  protected request with the successor; and
- replaying the consumed human refresh token commits family revocation and
  makes its successor unusable from a separate request/transaction.

The OpenAPI proof must be equally explicit:

- add a focused router test named
  `http::router::tests::only_json_openapi_is_served` proving
  `GET /openapi.json` returns the generated document and
  `GET /openapi.yaml` is not routed;
- retain and run each applicable `http::openapi::tests::*` contract test after
  tightening Platform and Principals coverage and replacing substring-only
  stable-code assertions;
- prove `openapi.yaml` and `examples/gen_openapi.rs` do not exist, and prove
  `mise.toml`, the server router/manifests, and the docs generator contain no
  `codegen:openapi`, `/openapi.yaml`, or repository-snapshot dependency;
- prove `architecture/operations/deployment-and-release.md` no longer promises
  an OpenAPI digest while retaining its schema/stub digest; and
- run `codegen:check` after the deletion to prove JSON Schemas, Python `.pyi`,
  TypeScript `.d.ts`, and the remaining generated contracts still regenerate
  cleanly.

At minimum run:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:platform:journey` twice consecutively
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:sql`
- `mise run codegen:check`
- `mise run check:examples`
- `mise run docs:check`
- strict `wyrd-sql` rustdoc with warnings denied

Do not substitute `mise run gate`; approved `VER-003` excludes the broad gate
for this change. Record exact focused commands and nonzero results in the
implementation evidence appended to this packet.

## Implementation evidence

Remediation commits, oldest first, on `claude/admin-principals-spec-qfsmjc`
above review base `c5c20754a`:

`64327074b`, `fef734235`, `e22f3266e`, `64b8c4a20`, `9aff46d11`, `040f5b6b7`,
`eb9f4234a`, `78fdbf427`, `eeb9183d4`, `5fbc53eae`, `25d3b30c1`, `096b080ed`,
`e8af5678c`, `4a49474fd`, `eceee25da`, `83390aba6`, `ae3b4f54b`, `ac4a20f06`,
`be71d3095`, `5c9e40928`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-admin-principals-2` — only `TenantConn`/`OperatorPool` in live provisioning/recovery | `5fbc53eae`: `components/platform/provisioning.rs`, `components/platform/recovery.rs` take the acquired `TenantConn` and the existing `OperatorPool`; pool composition stays at the route boundary | `mise run check:tenant-isolation` (exit 0); `mise run test:platform:journey` 33/33 | PASS |
| `FIND-admin-principals-3` — stable 409, no physical constraint identifier | `e22f3266e`: conflict mapping in `components/admin/routes.rs` renders the stable code without the constraint name | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::admin::routes::pg_tests::duplicate_create_conflicts) or test(=components::admin::routes::pg_tests::delete_issuer_with_live_binding_conflicts_then_cascades)'` (under `scripts/postgres/with-test-postgres.sh`) — 2 passed | PASS |
| `FIND-admin-principals-13` — one runtime `utoipa` contract, duplicates deleted | `eeb9183d4`: `openapi.yaml`, `examples/gen_openapi.rs`, the `/openapi.yaml` route and YAML branch, `serde_yaml`, the `utoipa` `yaml` feature, `codegen:openapi`, the docs generator's YAML reading, and the release OpenAPI digest are gone; `WyrdApiDoc`, `/openapi.json`, the docs page, client-tier gate, JSON Schemas, `.pyi`, `.d.ts` remain | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=http::router::tests::only_json_openapi_is_served)'` — 1 passed; `-E 'test(/^http::openapi::tests::/)'` — 6 passed; `mise run codegen:check`, `mise run check:client-tier`, `mise run docs:check` (exit 0) | PASS |
| `FIND-005-1` — issue-key effect and allowance commit or roll back together | `b1b3cf072`: `components/auth/routes.rs` acquires one `TenantConn`, appends the allowance with `audit::append_on`, issues, and commits once; denials stay independently durable | `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::auth::routes::pg_tests::a_failed_issue_commits_neither_the_key_nor_its_allowance)'` (under the Postgres wrapper) — 1 passed | PASS |
| `FIND-004-5` — CLI journey configures the tenant before restricted principals; operator docs include recovery | `e8af5678c`: `wyrd-cli/tests/operator_journey.rs` serves a real discovery document and runs `wyrd auth trusted-issuer add/list` before principal creation; `running-the-server.svx` mirrors it and the synopsis names `recover-root` | `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=operator_journey::operator_administers_a_deployment_through_the_cli)'` (under the Postgres wrapper) — 1 passed; `mise run test:cli:journey` — 24 passed, 5 ignored; `mise run docs:check` (exit 0) | PASS |
| `FIND-admin-principals-R2-3` — machine grants carry no refresh; human User refresh rotates with the consumed row id | `64b8c4a20`, `040f5b6b7`, `eb9f4234a`, `78fdbf427`, `83390aba6`: API-key and workload grants return `refresh_token: None` and write no refresh row; Service/Agent/TenantAdmin rotation removed; User rotation re-reads persisted roles and attributes the consumed row; `wyrd-client` keeps durable re-exchange | `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=exchange_api_key::pg_tests::delegation_issues_no_refresh_token) or test(=refresh::pg_tests::rotation_carries_the_roles_login_persisted) or test(=refresh::pg_tests::a_machine_refresh_row_cannot_rotate)'` — passed within the 12-test wyrd-auth focused run; `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=auth::tests::stale_token_re_exchanges_without_error) or test(=auth::tests::force_refresh_re_exchanges_once) or test(=auth::tests::workload_force_refresh_re_exchanges_once) or test(=auth::tests::exchange_caches_access_token_and_drops_refresh)'` — 4 passed; `platform_admin_e2e::a_tenant_administrator_renews_by_re_exchanging_its_credential` — 1 passed | PASS |
| `FIND-admin-principals-R2-4` — replay returns 401 after durably revoking the family once | `9aff46d11`: the route commits the staged transaction for the typed `Reused` result only; `refresh::pg_tests::f09_replay_containment_commits_and_kills_the_successor` reads committed state from a separate transaction | `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(=refresh::pg_tests::f09_replay_containment_commits_and_kills_the_successor) or test(=refresh::pg_tests::reuse_detection_revokes_family)'` (under the Postgres wrapper) — passed within the 12-test focused run; `identity_e2e::human_oidc_login_journey` drives rotate → replay → successor refused | PASS |
| `FIND-admin-principals-R3-1` — every platform operation records its exact resource | `096b080ed`: `platform_authz.rs` takes the caller's known resource; create uses the requested slug so a resumed provisioning never records the discarded proposal id | `mise exec -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(/^platform_authz::pg_tests::/)'` (under the Postgres wrapper) — 7 passed; `platform_admin_e2e::a_failed_provisioning_can_be_retried_with_the_same_slug` — 1 passed | PASS |
| `FIND-admin-principals-R3-2` — trailing-slash issuer completes first-login pinning | `fef734235` persists `issuer.as_str()`; `83390aba6` stores the claim mapping in the dotted-string shape the login resolver decodes | `mise exec -- cargo nextest run --locked -p wyrd-server --test platform_admin_e2e -E 'test(=a_trailing_slash_platform_issuer_completes_first_login)'` (`WYRD_AUTH_E2E=1`, under the Postgres wrapper) — 1 passed | PASS |
| `FIND-admin-principals-R3-3` — secrets are not debug-visible, JSON stays a string, schema stays write-only | `25d3b30c1`: `CreateTrustedIssuerRequest` uses the existing `SecretBearer`; the CLI wraps the argument in `SecretString` at the clap boundary | `mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=auth::trusted_issuer::tests::add_accepts_optional_client_secret_and_ttl) or test(=auth::trusted_issuer::tests::add_rejects_client_secret_and_file_together) or test(=auth::trusted_issuer::tests::resolve_client_secret_prefers_inline_then_reads_file) or test(=auth::trusted_issuer::tests::add_posts_secret_and_role_flags_in_body)'` — 4 passed; `http::openapi::tests::*` — 6 passed | PASS |
| `FIND-admin-principals-R3-4` — accurate rustdoc on every named fallible or panicking item | `64327074b`: `# Errors` on `recover_root`, `# Panics` on the named `platform_admin_e2e` helpers | `mise run lints` (exit 0); `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" mise exec -- cargo doc --locked --no-deps -p wyrd-sql` (exit 0) | PASS |
| `FIND-admin-principals-R3-5` — pre-change audit table upgrades, verifies, publishes both shapes, replays, keeps old field ids | `4a49474fd` + `ae3b4f54b` + `be71d3095`: `AuditLogTable` appends `credential_id` with an explicit `PARQUET:field_id`; the catalog recognizes only the exact prior fingerprint and performs one idempotent `ADD COLUMN`, reconciling the half-applied retry; `entry_hash` appends the credential segment only when present, so every historical preimage is byte-identical | `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=catalog::bifrost_catalog::audit_log_upgrade_tests::a_pre_credential_audit_log_upgrades_and_keeps_its_field_ids)'` (under the Postgres wrapper) — 1 passed; `mise exec -- cargo nextest run --locked -p vala-sql --lib -E 'test(/queries::audit_staging::tests::/)'` — 2 passed; `mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=audit_publication::retained_history_carries_both_credential_shapes)'` — 1 passed | PASS |

### Required proof list

Every lane below ran on the final tree and exited 0.

| Lane | Result |
|---|---|
| `mise run fmt:check` | exit 0 |
| `mise run lints` | exit 0 |
| `mise run check:client-tier` | exit 0 |
| `mise run check:unwrap-audit` | exit 0 |
| `mise run check:clippy-allow-audit` | exit 0 |
| `mise run check:tenant-isolation` | exit 0 |
| `mise run test:principals:unit` | 4 tests, 4 passed |
| `mise run test:principals:integration` | 15 tests, 15 passed |
| `mise run test:platform:journey` (1 of 2) | 33 tests, 33 passed |
| `mise run test:platform:journey` (2 of 2) | 33 tests, 33 passed |
| `mise run test:cli:journey` | 24 passed, 5 ignored |
| `mise run test:bifrost:journey:mcp` | 9 tests, 9 passed |
| `mise run test:sql` | exit 0 |
| `mise run codegen:check` | exit 0 |
| `mise run check:examples` | exit 0 |
| `mise run docs:check` | exit 0 |
| `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" mise exec -- cargo doc --locked --no-deps -p wyrd-sql` | exit 0 |

`mise run gate` was not substituted, per approved `VER-003`.

Beyond the minimum list, because `FIND-admin-principals-R3-5` changes the
catalog registration path every built-in table goes through:

| Lane | Result |
|---|---|
| `mise run test:bifrost:integration:redux` | 979 tests, 979 passed |
| `mise run test:bifrost:integration:server` | 67 tests, 67 passed |
| `mise run test:bifrost:integration:sql` | 115 tests, 115 passed |

### Material decisions the reviewer should see

1. **Login-time role persistence.** `040f5b6b7` mirrors the roles an IdP asserts
   into `wyrd.auth_user_roles` at federated login, so rotation can re-read
   current roles from the tenant's own store rather than re-trusting a token.
   This is a durable write on the login path and is reversible: removing it
   returns rotation to reading the claim, at the cost of the property R2-3
   requires.

2. **Four regressions this remediation introduced, found by running past the
   minimum list, and fixed.** Recorded because each one means a previously
   green surface went red inside this packet's work:
   - `83390aba6` — the platform connection wrote array-valued claim paths that
     `platform_connection_from_row` (the served login path) cannot decode, so
     federated platform sign-in failed entirely. Fixed at the writer.
   - `ae3b4f54b` — comparing declared and physical schemas by Iceberg field id
     *first* regressed every canonical ledger whose two sides number ids
     differently; `vala.traces.spans` stopped registering and its gRPC ingest
     stream aborted. Position is the primary comparison again and the id-aware
     pass runs only on schemas position rejects, so nothing previously refused
     is now accepted.
   - `be71d3095` — the retained-history projection pin asserted the
     pre-credential 14-column shape.
   - `ac4a20f06` — the FIND-005-1 rollback proof matched `AdminNotFound` while
     issuance refuses an unbound Card with `PrincipalNotFound`; the test had
     never been selected by any lane, so it was committed unrun.

3. **One superseded journey test replaced, not deleted.**
   `a_tenant_administrator_refreshes_and_cannot_replay` required an API-key
   exchange to advertise a refresh token, which R2-3 removes.
   `a_tenant_administrator_renews_by_re_exchanging_its_credential` proves the
   shipped split at the same journey level; human rotation and replay stay in
   `identity_e2e::human_oidc_login_journey` and
   `exchange_api_key::pg_tests::delegation_issues_no_refresh_token`.

4. **One stale fixture corrected outside the findings.** `5c9e40928` —
   `ingest_valid_token_is_not_rejected_as_unauthenticated` minted a token for an
   invented tenant id; per-request tenant admission refuses tokens for a tenant
   that was never provisioned, so the fixture now uses the test server's own
   tenant.

All non-goals held: no parallel audit path, pool wrapper, error catalog,
refresh service, secret wrapper, second API-description framework, transport,
or test harness was added; no generic schema evolution was broadened and no
fingerprint check was weakened; commit provenance and the
verified-change-contract commits are untouched; `FIND-TASK-001-10` stays waived.
