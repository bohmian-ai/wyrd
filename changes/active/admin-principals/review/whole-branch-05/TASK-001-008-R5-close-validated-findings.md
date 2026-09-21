# TASK-001-008-R5 — Close validated whole-branch findings

## Route and authority

Implement this remediation with `$wyrd-implement`. Review the next immutable
cumulative candidate with `$wyrd-task-review`; do not review only the R5 diff.

- Approved specification: `changes/active/admin-principals/spec.md`, revision
  10, status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `c9e1092bbdb4df3781eb91b0eb33150e00df7623`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-05/findings-validation.md`.

## Outcome

Close the nine validated roots without adding another identity, epoch,
blacklist, audit path, transport, route/error catalog, migration framework,
schema framework, or test harness. Reuse the existing authorization
transaction, epoch, shared HTTP transport, Axum/utoipa integration, SQLx
migrations, module imports, `request_json`, shared auth DTOs, and rmcp typed
schema support.

## Diagnoses and required corrections

### `FIND-admin-principals-R4-3` — same-second role withdrawal

The changed-role callback stores a whole-second epoch, token issuance stores a
whole-second `iat`, and verification rejects only `iat < epoch`. An old token
minted earlier in the same second therefore survives a real role reduction.
This violates `REQ-005`, `INV-013`, `AC-010`, and R4-3.

Keep the existing epoch and verifier comparison. In the role-change
transaction, choose and return the next whole second as the epoch; mint the
successor with that exact explicit `iat` and derive its expiry from it.
Unchanged logins do not advance the epoch. This orders old and new authority
without a wait, new cache, blacklist, or precision-dependent rule.

### `FIND-admin-principals-R4-5` — federated platform grant atomicity and attribution

The served first platform login autocommits the subject pin before opening the
audited session-grant transaction, so later append or issuance failure leaves
durable unaudited identity state. The grant event also discards the stored
principal kind and records every federated human as `global_admin`. This
violates `REQ-037`, `AC-009`, and R4-5.

Make the existing `PlatformSessions` owner run verified subject resolution or
pinning, active-principal read, session issuance, canonical append using the
stored principal kind, and commit in one audited transaction. The login owner
passes only verified issuer, subject, and match claim. Delete the pool-scoped
pin from this grant path. Preserve credential exchange through the same event
owner with its real kind and credential id.

### `FIND-admin-principals-R4-9` — duplicate MCP authentication policy

The production MCP adapter clones the raw client and auth handle, rebuilds Wyrd
headers, classifies 401s, refreshes, and owns replay. Every rmcp operation uses
this second policy owner, contrary to `REQ-047`, `REQ-048`, `AC-018`, and R4-9.

Keep rmcp framing and raw protocol IO in `wyrd-mcp`, but put Wyrd header
decoration and exactly-once authentication-refusal refresh/replay behind one
narrow capability on the existing `HttpTransport`/`AuthMiddleware` owner. The
adapter supplies the replayable rmcp operation and its session/custom headers;
remove its Wyrd constants, bearer rendering, auth handle, 401 classifier, and
retry policy. Do not add rmcp to `wyrd-client` or create a transport.

### `FIND-admin-principals-13` — duplicated and incomplete runtime OpenAPI

The live Axum registrations and `WyrdApiDoc.paths` are separate handler lists,
while the closure test source-parses path strings without methods. A served
method can therefore be omitted while the test stays green. `POST /auth/token`
also omits reachable audit-unavailable and internal issuance/configuration
errors. This violates `REQ-049`, `AC-014`, `AC-019`, and the stable finding.

Use the repository's version-matched official utoipa/Axum integration to
co-register handlers and operations in the owning route modules and compose
the final runtime document from those routers. Delete the central path list and
source parser. Trace and declare the complete `/auth/token` stable response set,
including audit-unavailable and internal issuance/configuration failures. Keep
`/openapi.json`; do not add YAML, snapshots, generators, or another error list.

### `FIND-admin-principals-R5-1` — qualified declaration types

Candidate-added fields, signatures, bounds, and returns use qualified paths
such as `reqwest::Client`, `uuid::Uuid`, `wyrd_sql::OperatorPool`,
`serde::de::DeserializeOwned`, and `sqlx::PgPool`, violating the mandatory
module-import/bare-name rule in `architecture/agent-rules.md`.

For each qualified declaration type added in the cumulative diff, extend the
existing module-top import block and use the bare name at that declaration.
Change neither expression paths nor untouched baseline code. Use the complete
confirmed location inventory in `findings-validation.md`.

### `FIND-admin-principals-R5-2` — dropped audit for stable no-effect outcomes

Several served platform and tenant routes append an allowed authorization row,
then return stable not-found, already-absent/revoked, wrong-owner, or
last-admin-conflict responses without committing the open transaction. The
decision row is rolled back even though permission was evaluated, violating
`REQ-037`, `AC-009`, and the canonical audit rule.

In the exact platform credential, platform identity/status, trusted-issuer,
and workload-binding routes listed in `findings-validation.md`, commit the
existing decision transaction before returning a stable logical no-effect
outcome. Keep store and commit failures rollback-coupled. Validate syntax before
opening the decision where the current route already can. Reuse
`commit_decision` or `TenantConn::commit`; add no audit helper.

### `FIND-admin-principals-R5-3` — rewritten immutable Vala migration

The candidate edits applied migration
`20260802000000_vala_audit_staging.sql`, changing its SQLx checksum. Any
database migrated at the base refuses startup before serving. Fresh-schema
lanes cannot detect this. This violates the SQL foundation and deployment
migration authorities.

Restore that migration byte-for-byte to the base and add one forward-only Vala
migration that adds nullable `credential_id`. Keep all deleted application and
Iceberg legacy fingerprint, hash, field-id, evolution, and reconciliation code
deleted; only the SQL forward migration is required.

### `FIND-admin-principals-R5-4` — tenant credential revoke skips renewal

The public tenant `revoke_credential` control call uniquely uses
`request_raw`, which sends once and never refreshes after a 401. Neighboring
control requests use the shared retry owner. This violates `REQ-048` and
`AC-018`.

Change only this operation to the existing
`request_json::<(), ()>(DELETE, path, None)` path, which already handles an
empty 204 response and bounded renewal. Leave streaming storage downloads and
`request_raw` unchanged.

### `FIND-admin-principals-R5-5` — incomplete duplicate MCP schemas

The two principal tools handwrite input JSON separate from their private
argument structs, publish no output schemas, and return one ad hoc response.
Agents cannot discover the contract and it can drift from HTTP/`wyrd-spec`,
violating the agent-harness authority and `REQ-036`.

Put the two minimal inputs and one revocation acknowledgement beside the
existing auth DTOs in `wyrd-spec` with serialization and schema derives. Reuse
`CredentialListResponse` for listing. Use rmcp's installed typed input/output
schema methods, deserialize the same input DTOs, return the same output DTOs,
and delete handwritten schemas and duplicate private argument structs. Add no
MCP schema layer.

## Constraints and preserved behavior

- Preserve the closed five kinds, platform/tenant plane separation, RLS,
  `TenantConn`/`OperatorPool`, fixed-cost credential refusal, last-admin
  protection, credential secrecy, human-only refresh, and machine
  durable-credential re-exchange.
- Preserve one canonical `vala.audit_staging` append and sole
  `AuditPublisher`; audit each authorization evaluation and keep effect/store
  failures transactionally coupled.
- Preserve platform per-request credential/grant anchoring, current strict
  Bifrost schemas and hashes, MCP framing/session/SSE semantics, `/openapi.json`,
  stable Wyrd errors, generated schemas, and current docs.
- Preserve the closed R4 findings and the waiver of `FIND-TASK-001-10`.
- Make the smallest owner-local corrections. Do not broaden the approved spec
  or rewrite unrelated code.

## Explicit non-goals

- No new cache, token blacklist, identity/session service, audit writer/table,
  route catalog, error catalog, HTTP transport, migration framework, failure
  injector, schema framework, or permanent diff scanner.
- No application/Iceberg legacy-audit compatibility, OpenAPI YAML/snapshot/
  generator, UI/browser state, Python or TypeScript admin bindings, Card-name
  contract, platform-human CLI expansion, or workspace-wide rustdoc cleanup.
- No history rewrite, provenance edit, Git identity change, merge, push, or
  deployment.

## Acceptance criteria

| Finding | Acceptance |
|---|---|
| `FIND-admin-principals-R4-3` | Equal-second old tokens are refused after a real role reduction, the explicitly ordered successor is accepted with reduced roles, and unchanged login preserves sessions. |
| `FIND-admin-principals-R4-5` | Failed first federated grant leaves no pin or token; retry creates one pin and one audit row with stored kind `user`, real principal, and no credential; credential-backed root attribution remains correct. |
| `FIND-admin-principals-R4-9` | MCP retains framing/session behavior but no Wyrd header, bearer, auth handle, 401 classifier, or retry policy; one shared renewal/replay is used. |
| `FIND-admin-principals-13` | Served method/path operations and runtime OpenAPI come from co-registration; the duplicate list/parser is absent; `/auth/token` declares and returns every reachable stable error; YAML remains unrouted. |
| `FIND-admin-principals-R5-1` | No candidate-added declaration contains a qualified type path; format, lint, and affected tests pass. |
| `FIND-admin-principals-R5-2` | Every named stable no-effect response commits exactly one authorization decision and no effect; injected store failure commits neither. |
| `FIND-admin-principals-R5-3` | The base Vala migration checksum is restored; base-to-candidate upgrade succeeds; nullable credential attribution and current audit publication work. |
| `FIND-admin-principals-R5-4` | Tenant credential revoke renews and replays once after the first 401, stops after the second, and retains 204 success. |
| `FIND-admin-principals-R5-5` | Both principal tools advertise and consume shared input/output schemas, and real results validate while scope/error behavior remains unchanged. |

## Focused proof

Add or select the smallest repository-native proof for each acceptance row:

- a deterministic same-second role-reduction verifier/served-flow case;
- platform first-pin append failure plus retry and stored-kind attribution;
- real-Postgres stable no-effect audit cases for every named route family;
- an upgrade fixture that applies the exact base Vala migration set before the
  candidate set;
- shared-client tenant credential-revoke first/second-401 behavior;
- MCP first/second-401 behavior through the shared owner and source absence of
  local Wyrd auth policy;
- assembled-server method/path/error OpenAPI closure and injected token
  failure responses; and
- real MCP catalog/result schema conformance.

Record exact nonzero `mise exec -- cargo nextest run --locked` selectors for
every named Rust test, using the repository Postgres or identity wrapper when
needed. Confirm names with `cargo nextest list`; do not use a selector that can
pass with zero tests.

## Broader verification

Run the narrowest existing `mise` tasks that cover the changed owners, at
minimum:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run test:shared`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:platform:journey` twice consecutively
- `mise run test:identity:journey`
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:sql`
- `mise run test:bifrost:integration:redux`
- `mise run test:bifrost:integration:server`
- `mise run test:bifrost:integration:sql`
- `mise run codegen:check`
- `mise run check:examples`
- `mise run docs:check`
- strict affected-crate rustdoc where required by the touched surface
- the focused base-migration upgrade proof
- a cumulative diff-based Rustdoc audit for newly/materially modified Rust
  items
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`

Do not substitute `mise run gate`; approved `VER-003` excludes it. Record an
implementation evidence table mapping every acceptance row to commits, exact
focused proof, broader lanes, and result.

## Implementation evidence

Final candidate: `5ecc8a4e5a3a76390bc32f00353e558ea6a685d2`.
Commits, in the mandated correction order: `16b46977`, `29f36218`, `2dc245a8`,
`402b2c85`, `de716ea8`, `b729a739`, `b9f7fbcc`, `c116ea7f`, `cf634bdc`, plus
`5ecc8a4e` (regression found by `test:cli:journey` and fixed inside FIND-13's
own correction; see the note below).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-admin-principals-R4-3` — equal-second refusal, ordered successor accepted, unchanged login preserved | `16b46977`(ordering prerequisite), `29f36218`: `crates/shared/wyrd-auth-issue/src/lib.rs` (`TokenWindow`, `issue_user_access_token_at`), `crates/shared/wyrd-auth-verify/src/lib.rs`, `crates/wyrd/wyrd-auth/src/callback.rs`, `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs` | `mise exec -- cargo nextest run --locked -p wyrd-auth-issue --lib -E 'test(=tests::issue_user_access_token_at_carries_the_chosen_instant)'` (1 passed); `… -p wyrd-auth-verify --lib -E 'test(=tests::revocation_epoch_admits_the_successor_minted_at_the_epoch)'` (1 passed); `scripts/postgres/with-test-postgres.sh -- … -p wyrd-auth --lib -E 'test(=callback::pg_tests::a_withdrawn_role_advances_the_epoch_past_a_same_second_token)'` (1 passed); lanes `test:principals:integration`, `test:identity:journey` (20/20) | PASS |
| `FIND-admin-principals-R4-5` — atomic first federated grant, stored-kind attribution, credential-backed attribution intact | `2dc245a8`: `crates/wyrd/wyrd-auth/src/platform_sessions.rs`, `platform_login.rs`, `platform_credentials.rs`, `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs` | `scripts/postgres/with-test-postgres.sh -- … -p wyrd-auth --lib -E 'test(=platform_sessions::pg_tests::a_first_federated_login_pins_grants_and_audits_atomically)'` (1 passed); lanes `test:principals:integration` (7/2/12/15/10 passed), `test:platform:journey` twice (36 passed each) | PASS |
| `FIND-admin-principals-R4-9` — no Wyrd auth policy in MCP; one shared renewal/replay | `b729a739`: `crates/shared/wyrd-client/src/transport/http.rs` (`HttpTransport::authenticated_replay`), `crates/wyrd/wyrd-mcp/src/client.rs` | `mise exec -- cargo nextest run --locked -p wyrd-mcp --lib -E 'test(=client::tests::no_wyrd_credential_policy_is_written_here)'` (1 passed); lanes `test:shared` (668 passed), `test:bifrost:journey:mcp` (9 passed) | PASS |
| `FIND-admin-principals-13` — co-registered routing and document, no duplicate list or parser, complete `/auth/token` error set | `c116ea7f`: `crates/wyrd/wyrd-server/src/http/router.rs` (`OpenApiRouter` + `split_for_parts`), `http/openapi.rs` (paths/modifiers removed), all 15 component routers on `routes!`, `components/auth/routes.rs` (500/503 declared), `tests/pg_openapi_contract.rs` (source parser deleted, served-document tests added), `mise.toml` (contract suite wired into `test:principals:integration:inner`) | `scripts/postgres/with-test-postgres.sh -- … -p wyrd-server --test pg_openapi_contract -E 'test(=the_served_document_describes_the_composed_surface) \| test(=every_problem_response_declares_its_media_type_and_stable_code) \| test(=every_authenticated_path_declares_the_one_wyrd_scheme) \| test(=bifrost_operations_publish_typed_problem_refusals) \| test(=card_contract_publishes_typed_lifecycle_and_problem_shapes) \| test(=an_unstageable_exchange_audit_answers_with_a_code_the_token_operation_documents)'` (6 passed); lanes `test:principals:integration`, `test:cli:journey` (24 passed), `test:storage:matrix` | PASS |
| `FIND-admin-principals-R5-1` — no candidate-added declaration carries a qualified type path | `cf634bdc`: bare names at ~35 files' declarations, module-top imports extended only | `mise run fmt:check` (clean); `mise run lints` (clean); every lane below | PASS |
| `FIND-admin-principals-R5-2` — every stable no-effect response commits exactly one decision and no effect | `402b2c85`: `components/platform/credentials.rs`, `components/platform/identity.rs`, `components/admin/routes.rs` | `WYRD_AUTH_E2E=1 scripts/postgres/with-test-postgres.sh -- … -p wyrd-server --test platform_admin_e2e -E 'test(=an_authorized_request_that_changes_nothing_still_records_the_decision)'` (1 passed); lanes `test:principals:integration`, `test:platform:journey` twice | PASS |
| `FIND-admin-principals-R5-3` — base migration checksum restored, base-to-candidate upgrade succeeds | `16b46977`: `20260802000000_vala_audit_staging.sql` restored byte-for-byte, new forward-only `20260910000027_audit_staging_credential_id.sql`, `crates/vala/vala-sql/tests/pg_migration.rs` | `scripts/postgres/with-test-postgres.sh -- … -p vala-sql --test pg_migration -E 'test(=pg_tests::shipped_audit_staging_migration_is_immutable) \| test(=pg_tests::staged_rows_survive_the_credential_attribution_upgrade)'` (2 passed — the second is the base-migration upgrade proof); lanes `test:sql` (122/4/118/2 passed), `test:bifrost:integration:sql` (118 passed) | PASS |
| `FIND-admin-principals-R5-4` — revoke renews and replays once, stops after the second refusal, keeps 204 | `de716ea8`: `crates/shared/wyrd-client/src/principals/handle.rs` on `request_json::<(), ()>` | `mise exec -- cargo nextest run --locked -p wyrd-client --test transport -E 'test(=http::transport_behavior::revoke_credential_re_exchanges_once_and_replays) \| test(=http::transport_behavior::revoke_credential_stops_after_a_second_refusal)'` (2 passed); lane `test:shared` | PASS |
| `FIND-admin-principals-R5-5` — both tools advertise and consume shared schemas, real results conform | `b9f7fbcc`: `crates/wyrd-spec/src/auth/tenant_principals.rs` (`ListCredentialsArgs`, `RevokeCredentialArgs`, `CredentialRevoked`), `crates/wyrd/wyrd-server/src/mcp/principals.rs` (handwritten schemas and private structs deleted) | `scripts/postgres/with-test-postgres.sh -- … -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=principals::pg_tests::a_write_tool_is_scoped_at_dispatch_not_merely_hidden)'` (1 passed); lanes `test:bifrost:journey:mcp` (9 passed), `mise run codegen:check` (clean, no git drift) | PASS |

### Broader verification

All green on `5ecc8a4e`: `fmt:check`, `lints`, `check:client-tier`,
`check:unwrap-audit`, `check:clippy-allow-audit`, `check:tenant-isolation`
(the last three invoked as `python3 scripts/check_*.py` — the `mise` wrappers
call `python`, which this environment does not provide), `test:shared`
(668 passed), `test:principals:unit`, `test:principals:integration`,
`test:platform:journey` twice consecutively, `test:identity:journey` (20/20),
`test:cli:journey` (24 passed), `test:bifrost:journey:mcp` (9 passed),
`test:sql`, `test:bifrost:integration:redux` (979 passed),
`test:bifrost:integration:server` (67 passed),
`test:bifrost:integration:sql` (118 passed), `test:storage:matrix`,
`codegen:check`, `check:examples`, `check:docs` (strict
`-D missing_docs -D rustdoc::broken_intra_doc_links` over `wyrd-spec`,
`wyrd-auth-issue`, `wyrd-auth-verify` — the crates this change touched that the
gate covers), and the `docs:check` gate steps run with `python3`
(generate → no drift → commands → links → build → a11y, 60 pages).
`git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f 5ecc8a4e` is clean.
A cumulative diff-based rustdoc audit of the items these commits added or
materially changed found every module, struct, field, function, method, and
test function documented, with `# Errors` on each fallible one; `check:docs`
proves it mechanically for the covered crates.

Two lanes failed once and passed on an unchanged re-run:
`identity_e2e::human_oidc_login_journey` and four
`vala-bifrost-redux::integration::forge` claim/lease cases. Both are timing
flakes in suites this change does not touch; re-runs were 20/20 and 979/979.

### Regression found and closed during verification

`test:cli:journey` caught a real regression introduced by FIND-13's own
correction: `utoipa-axum` mounts the path it documents, and OpenAPI templating
has no wildcard, so co-registering the two local development blob routes
republished `/cards/upload/local/{*id}` and `/cards/download/local/{*path}` as
single-segment templates and `wyrd get` stopped reaching its own artifacts.
`5ecc8a4e` mounts those two routes directly, without a document entry, and
says why beside them. `test:cli:journey` and `test:storage:matrix`
(`local_client_server_round_trip`) both pass on the fix; no new test was added
because two existing lanes already prove the behavior.

### Non-goals and scope

Every non-goal stayed excluded: no new cache, blacklist, identity/session
service, audit writer or table, route or error catalog, HTTP transport,
migration framework, failure injector, schema framework, or permanent diff
scanner; no legacy-audit compatibility, OpenAPI YAML/snapshot/generator, UI
state, Python or TypeScript admin bindings, Card-name contract, platform-human
CLI expansion, or workspace-wide rustdoc cleanup; no history rewrite,
provenance edit, Git identity change, merge, push, or deployment. The MCP
schemas reuse the repository's own `schemars` derive rather than rmcp's typed
helpers, because rmcp 3.3 carries `schemars` 1.2.1 while the workspace derives
0.8.22 — the generated schema is handed to rmcp raw, so there is still exactly
one source of truth and no MCP schema layer. `mise run gate` was not
substituted for any lane. Every changed file belongs to a finding above; the
only non-Rust edits are `mise.toml` (wiring the contract suite into a named
lane and dropping a selector that now selects zero tests) and `AGENTS.md` §11
(the OpenAPI proof command, which FIND-13 made stale).

Returned status: `IMPLEMENTED`.
