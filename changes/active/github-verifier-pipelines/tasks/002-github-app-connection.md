---
id: TASK-002
status: proposed
approved_spec_revision: 4
requirements: [REQ-001, REQ-016]
acceptance: [AC-001, AC-011]
invariants: [INV-001, INV-003, INV-006, INV-007]
depends_on: []
---

# Connect the deployment GitHub App

Implement **approved-runtime-and-github.md → Operator App configuration and tenant
installation**, using **contracts.md → GitHub connection**. One operator App
serves SaaS, self-hosted and enterprise deployments through the same code.
Connection grants source access; it does not register a Card or review policy.

## Inputs, owners and selected flow

The operator supplies App ID/slug, configured web/API origins, private key,
webhook secret, OAuth client credentials and exact public URLs through existing
server configuration/secret providers. Tenant callers supply normal authenticated
connection requests and verified repository selections, never keys or arbitrary
hosts. Existing `server/src/config.rs`, `server/src/auth/{browser,callback}.rs`
and SQL `queries/auth/login_state.rs` own configuration and browser-bound
one-use authorization transactions. Reuse those protocols with explicit GitHub/
OIDC flow separation. Dedicated installation queries in `wyrd-sql` own the only
new table, `wyrd.github_installations`, under REQ-001. Operator credentials are
not GitHub installation association.

`wyrd-sql` owns one dedicated `wyrd.github_installations` table for tenant identity
(`data_tenant_id`), configured host, verified installation ID, selected accessible
repositories, suspension and revision, with connection identity and safe timestamps
as needed. Postgres enforces one associated installation per tenant and unique
`(host, installation_id)` across tenants, including concurrent claims. Suspension
retains ownership. Enable and force standard tenant RLS. Keep
`auth_human_connections`, its OIDC constraints and queries unchanged. An
installation grants repository access, while that table defines tenant OIDC login
trust; sharing it would require nullable OIDC fields and provider filtering in
security-sensitive login queries. The dedicated table avoids that coupling. Reuse
`auth_login_state` for temporary actor/tenant/browser-bound connection transactions
with explicit flow separation: neither callback may consume the other's state,
and stale callbacks cannot restore a disconnected or replaced connection. One
narrow SECURITY DEFINER installation-to-tenant lookup routes verified webhooks,
with pinned search_path and body-scoped operator elevation; subsequent work uses
TenantConn and normal authorization. Routing grants no permission. Store no App
keys or user/installation tokens in these rows. Private columns and generation
mechanics remain implementation-owned.

Discard the abandoned shared-table migration approach. The uncommitted draft
`20261110000000_github_app_connections.sql` was removed for revision 4;
implement the dedicated-table and login-state migration afresh. Applied
migrations remain immutable.

Implement the single `/v1/integrations/github` connect/read/disconnect flow from
the contracts. Start a one-use actor/tenant/browser-bound transaction with the
existing transaction expiry behavior. Support installation begun from Wyrd or
GitHub. Keep setup and OAuth callback separate: setup's installation ID is a
hint; callback verifies transaction and transient user authorization, revalidates
installation and user repository access, and conditionally associates one
installation with one tenant. Enforce the inverse uniqueness too. OAuth tokens
are transient/redacted. Background work obtains installation tokens and narrowly
authorized execution authority, never a saved personal token.

App key signing, webhook verification, connection callbacks and tenant policy
belong in Wyrd. The independent library supplies generic provider requests only;
deleted historical App branches/commits supply neither code nor an approved pin.
Retain conventional configured GitHub host support. Missing optional App config
makes only this integration unavailable; invalid present config uses typed
validation. Document Contents read, Pull requests write, Checks write and required
PR/source/installation-access events and the actual served setup/callback URLs.

Repository use requires both current installation access and tenant selection.
Synchronize installation removal/suspension/access events. Fence new source IO
and affected unfinished work on access loss; disconnect fences work before
releasing association. Reuse queue cancellation/revocation owners; task 006 adds
its PR-generation records to that same fence. Unrelated repositories continue.

Amend **wyrd-security-posture.md → Trust boundaries / Principal and credential
lifecycle / Outbound credentials** and **wyrd-design.md → Runtime identity** for
the association and credential ownership. Reuse typed permission/audit owners
without prescribing another permission category or claim-management workflow.

## Outputs, surfaces and handoff

Return only the approved redacted connection/access view: configured App/host,
verified installation, connection state, selected accessible repository IDs/names
and safe timestamps. Tasks 003/004/006/009 use this verified identity/access and
installation-token facility; provider metadata alone never confers tenant rights.
The connect transaction receipt follows durable transaction creation; successful
association/disconnect follows committed association/fence state, not URL issue.

Ship typed HTTP, shared Rust client and thin Rust/Python/TypeScript SDK, CLI and
MCP projections here, including safe errors, discoverable reads and scoped
writes. Update operator and tenant setup/diagnosis guides here, with no UI.

## Connected proof

**AC-001:** A real server journey starts both entry paths against a local GitHub
protocol mock, finishes verified selection, reads the public connection and then
disconnects. Assert spoofed installation IDs, consumed/foreign transactions,
foreign tenant, concurrent claims in both directions, cross-flow state, stale
callbacks after reconnect and missing/invalid config fail safely;
keys/tokens never appear in view. Prove forced RLS, unchanged OIDC login and
suspension retaining ownership. Remove one repository/suspend/disconnect and
observe only affected existing queued work fenced, association released in the
right order and no Cards/bindings created. Task 006 extends this journey to
actual PR work; do not substitute a fabricated connection there.

**AC-011 (connection portion):** Follow the new CLI/SDK setup guide, inspect the
connection and use the served HTTP/MCP catalog under normal authorization.

Tier 1: SDK story journeys in `sdks/wyrd-sdk-rust` (package `wyrd-ai`), Python
and TypeScript integration trees, plus `wyrd-mcp`'s existing `mcp` target.
Existing runners: `mise run test:rust-sdk:journey`, `mise run py:test:integration`,
`mise run ts:test:integration`, `mise run test:bifrost:journey:mcp`. Tier 2:
conditional association/revocation in SQL's `integration` target via
`mise run test:sql`; tier 3: protocol/signature/config branches in their owners.
Use local provider mocks, real Postgres and WyrdTestServer; no live App keys.
Record new focused commands once tests exist. Run applicable checks from README,
including tenant isolation and OpenAPI; only `wyrd.github_installations` is a new
PostgreSQL table; no UI.

## Obligations

| ID | Source (task line) | Observation | Planned assertion |
|---|---|---|---|
| O1 | "operator supplies App ID/slug, configured web/API origins, private key, webhook secret, OAuth client credentials and exact public URLs through existing server configuration/secret providers" | Every App field parses from server config/env; secrets only from env/file, redacted in Debug | `config` unit test parses and redacts the GitHub section |
| O2 | "Missing optional App config makes only this integration unavailable" | Without App config the server boots, other `/v1` routes work and every GitHub operation refuses with a typed 503 | SDK journey: unconfigured server → typed unavailable error; ordinary read succeeds |
| O3 | "invalid present config uses typed validation" | Partial/invalid App config refuses startup with `ConfigError::Invalid` naming the field | `config` unit test for partial, bad origin, bad key |
| O4 | "Retain conventional configured GitHub host support" | Web and API origins are configurable (GitHub Enterprise Server shape); the view reports the configured host | config unit test + journey view `host` equals mock origin host |
| O5 | "Implement the single `/v1/integrations/github` connect/read/disconnect flow" | `POST /connect`, `GET`, `DELETE` served with typed bodies and in OpenAPI | journey assertions per surface + OpenAPI path assertion |
| O6 | "Start a one-use actor/tenant/browser-bound transaction with the existing transaction expiry behavior"; "connect transaction receipt follows durable transaction creation" | Connect returns the GitHub install URL and expiry only after the login-state row commits; the row binds actor and tenant, then the browser at setup, and expires with `LOGIN_STATE_TTL` | SQL integration: row bound/expiring; journey: receipt then setup works |
| O7 | "Support installation begun from Wyrd" | connect → setup (state) → OAuth authorize → callback associates | journey (Wyrd-begun) |
| O8 | "Support installation begun from ... GitHub" | setup without state, signed-in browser → transaction → OAuth → callback associates | journey (GitHub-begun) |
| O9 | "Keep setup and OAuth callback separate: setup's installation ID is a hint" | setup never associates; only callback does, after verification | journey: after setup, view still not connected |
| O10 | "callback verifies transaction and transient user authorization, revalidates installation and user repository access" | callback exchanges the code, checks the App installation, the user's installation access and user-accessible repositories before associating | journey with mock recording calls; selected repos = user-accessible repos |
| O11 | AC-001 "spoofed installation IDs ... fail safely" | A hint the user cannot access, or another App's installation, is refused and nothing is associated | journey negative |
| O12 | AC-001 "consumed ... transactions" | Replaying a consumed callback state is refused | journey negative |
| O13 | AC-001 "foreign transactions" | A callback state presented by another principal's browser session is refused | journey negative |
| O14 | AC-001 "foreign tenant" | A transaction of tenant A cannot be completed in tenant B's session | journey negative |
| O15 | "conditionally associates one installation with one tenant. Enforce the inverse uniqueness too" | An installation associated with tenant A is refused for tenant B; a tenant connected to installation X is refused for installation Y; reconnecting X refreshes selection | SQL integration + journey |
| O15a | AC-001 "concurrent claims in both directions"; "Postgres enforces ... including concurrent claims" | Two tenants racing for one installation: exactly one wins; one tenant racing two installations: exactly one wins | SQL integration with concurrent transactions on real Postgres |
| O15b | AC-001 "cross-flow state"; "neither callback may consume the other's state" | GitHub callback refuses an OIDC login state and the OIDC callback refuses a GitHub state; neither consumes the other's row | SQL integration + journey negative |
| O15c | AC-001 "stale callbacks after reconnect"; "stale callbacks cannot restore a disconnected or replaced connection" | A transaction begun before disconnect/reconnect cannot complete afterwards | SQL integration + journey negative |
| O15d | AC-001 "Prove forced RLS" | `wyrd.github_installations` has RLS enabled and forced; tenant B reads/writes none of tenant A's rows | SQL integration catalog + cross-tenant assertion |
| O15e | AC-001 "unchanged OIDC login" | `auth_human_connections` untouched by the migration; OIDC browser login still completes | migration inspection + existing OIDC journey selector |
| O15f | AC-001 "suspension retaining ownership" | A suspended installation stays associated with its tenant and still refuses another tenant | SQL integration + journey |
| O16 | AC-001 "missing/invalid config fail safely" | = O2/O3 | as O2/O3 |
| O17 | "OAuth tokens are transient/redacted"; AC-001 "keys/tokens never appear in view" | No user/installation token, key or secret in view, errors or Debug; nothing stored | journey: serialized views contain no mock token/key; unit Debug redaction |
| O18 | "Return only the approved redacted connection/access view" | View = host, App id/slug, installation id when verified, state, selected accessible repositories (id, full name), safe timestamps | journey asserts exact view shape |
| O19 | "Repository use requires both current installation access and tenant selection" | Installation token minted only for an associated, unsuspended, selected and still-accessible repository | server integration test of the token facility |
| O20 | "Background work obtains installation tokens and narrowly authorized execution authority, never a saved personal token" | Token facility narrows `repository_ids` and permissions; no user token persisted | integration test inspects mock request body |
| O21 | "Synchronize installation removal/suspension/access events" | Signed `installation` deleted/suspend/unsuspend and `installation_repositories` removed update the connection; bad signature refused | journey via signed webhook + unit HMAC test |
| O22 | "Fence new source IO ... on access loss"; AC-001 "Remove one repository/suspend/disconnect" | After removal/suspension/disconnect the token facility refuses the affected repository; unrelated repositories keep working | integration test |
| O23 | "disconnect fences work before releasing association"; AC-001 "association released in the right order" | Disconnect fences, then releases; afterwards the installation may associate with another tenant | SQL integration + journey |
| O24 | AC-001 "observe only affected existing queued work fenced" | Unrelated queued executions remain unfinished after removal/suspend/disconnect | SQL integration: queued execution survives |
| O25 | "Connection grants source access; it does not register a Card"; AC-001 "no Cards/bindings created" | Card list empty after connect | journey |
| O26 | "Document Contents read, Pull requests write, Checks write and required PR/source/installation-access events and the actual served setup/callback URLs" | Operator guide lists exactly these and the served paths | `mise run docs:check` + doc citation |
| O27 | "Amend wyrd-security-posture.md → Trust boundaries / Principal and credential lifecycle / Outbound credentials and wyrd-design.md → Runtime identity" | Sections describe association and credential ownership | doc citation |
| O28 | "Reuse typed permission/audit owners without prescribing another permission category" | Read/connect/disconnect use the existing `identity_connections:write` (corrected; see Evidence); decisions audited | journey: unprivileged caller 403; audit unit |
| O29 | "Ship typed HTTP, shared Rust client and thin Rust/Python/TypeScript SDK, CLI and MCP projections ... safe errors, discoverable reads and scoped writes" | Each surface reads/connects/disconnects; MCP read tool listed for readers, write tools only for writers | per-surface journeys |
| O30 | "Update operator and tenant setup/diagnosis guides here, with no UI"; AC-011 | Guide commands match served CLI/SDK operations | docs:check + CLI journey runs guide commands |
| O31 | Tier 2 "conditional association/revocation in SQL's integration target" | Association conditional on bound slot; uniqueness; release | `pg_github_installations` tests |
| O32 | REQ-001 / INV-007 dedicated storage and unchanged OIDC trust | Only wyrd.github_installations is added; forced RLS and bidirectional uniqueness hold; flow-separated state refuses stale callbacks; OIDC trust shape/login behavior stay intact | Migration/source inspection; real Postgres races and isolation; callback negative journeys and existing OIDC regression coverage |

## Evidence

Abbreviations: **RJ** = `sdks/wyrd-sdk-rust/tests/integration/github_connection.rs`;
**SQL** = `crates/wyrd/wyrd-sql/tests/integration/pg_github_installations.rs`;
**SRV** = `crates/wyrd/wyrd-server/tests/integration/pg_github_connection.rs`;
**OAS** = `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs`;
**CFG** = `crates/wyrd/wyrd-server/src/config.rs`; **CLI** =
`crates/wyrd/wyrd-cli/tests/github_journey.rs`; **MCP** =
`crates/wyrd/wyrd-mcp/tests/bifrost/mcp/github.rs`; **PY** =
`sdks/wyrd-sdk-python/tests/integration/test_github_connection.py`; **TS** =
`sdks/wyrd-sdk-ts/wyrd/tests/integration/github-connection.test.ts`; **DOC** =
`docs/src/content/docs/operate/connect-github.md`.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| O1 App config from config/secret providers, redacted | `GithubConfig` + `apply_env_overrides` in CFG | CFG:6819-6838 (every field and all three secrets parse from env and the key file), CFG:7006-7019 (nonsecret fields from TOML, webhook and OAuth secrets from `_FILE`), CFG:6842 (Debug redacts), CFG:7033 (no secret from TOML) | `config::tests::github_app_config_parses_from_env_and_files_and_redacts`, `github_app_config_reads_toml_fields_and_secret_files`, `github_secrets_never_come_from_toml` | PASS |
| O2 missing config: only GitHub unavailable | `GithubConnections::app` → `not_configured()` | RJ:559 (begin/get/disconnect 503), RJ:572 (webhook 503), RJ:573 (Card list still served); PY:133; TS `github-connection-not-configured.test.ts`:18 | RJ `an_unconfigured_deployment_refuses_only_github`; PY/TS lanes | PASS |
| O3 invalid present config: typed validation | `GithubConfig::resolve` | CFG:6938 (`ConfigError::Invalid` naming each of seven fields), CFG:6950 (partial section: `Invalid` naming `app_id`) | `config::tests::github_app_config_is_optional_but_complete_once_present` | PASS |
| O4 configured GitHub host | `web_origin`/`api_origin` | CFG:6821 (GHE host), CFG:6881 (default), RJ:193 (view host = mock host) | config tests; RJ | PASS |
| O5 `/v1/integrations/github` served with typed bodies, in OpenAPI | `components/github/routes.rs` | OAS:318 (five operations, success status, security), OAS:327-346 (typed `GithubConnectionView`/`GithubConnectResponse`/`GithubConnectRequest` schemas), OAS:350 (webhook clears bearer); SRV:218, SRV:238 (DELETE/POST/GET served) | `pg_openapi_contract::github_connection_operations_publish_their_contract`; SRV | PASS |
| O6 one-use, actor/tenant/browser-bound transaction, existing expiry; receipt after commit | `begin` (`TRANSACTION_TTL = LOGIN_STATE_TTL`), `bind_github_login_state` | RJ:164-167 (receipt `expires_at` is the 5-minute login-state lifetime); RJ:168-174 (a separate browser request finds the committed row and reaches authorization); SQL:607-608 (browser + hint bound), SQL:583-589, SQL:616-638 (no rebind, other browser/principal/tenant), SQL:683 (expired refused) | RJ `connections_associate_only_after_github_confirms_them`; SQL `github_transactions_are_flow_principal_and_browser_bound` | PASS |
| O7 begun from Wyrd | connect → setup → authorize → callback | RJ:193-198 | RJ | PASS |
| O8 begun from GitHub | setup without state begins a browser-bound row | RJ:237-238 | RJ | PASS |
| O9 setup only hints; only callback associates | `setup` never associates | RJ:174-177 (setup stopped at authorization; still not connected) | RJ | PASS |
| O10 callback verifies code, installation, user access, user-accessible repos | `callback` (`service.rs`:451 exchange, :458-479 user access, :482-488 App-JWT lookup) | RJ:357 (a code GitHub never issued is refused `authorization_refused`), RJ:298 (hint the user cannot access), RJ:498 (suspension known only from the App-JWT lookup at `service.rs`:488 refuses), RJ:197 (hidden repo excluded) | RJ | PASS |
| O11 spoofed installation IDs | hint used only if the user can access it | RJ:298 (inaccessible hint and foreign App's installation) | RJ `spoofed_foreign_stale_and_conflicting_connections_are_refused` | PASS |
| O12 consumed transaction | one-use consume | RJ:209-214, RJ:368 (spent by the forged code); SQL:649-654 | RJ; SQL | PASS |
| O13 foreign (other principal) transaction | actor + browser binding | RJ:316; SQL:583-589, SQL:623-629; PY:118; TS:116 | RJ; SQL; PY; TS | PASS |
| O14 foreign tenant | tenant RLS on login state | RJ:316 (tenant B browser); SQL:632-638 | RJ; SQL | PASS |
| O15 bidirectional uniqueness; reconnect refreshes | conditional association | RJ:404, RJ:411, RJ:439 (reconnecting 101 refreshes repositories); SQL:159-160, SQL:231 | RJ; SQL | PASS |
| O15a concurrent claims both directions | unique indexes + slot revision | SQL:317-381 | `pg_github_installations::concurrent_claims_have_one_winner_in_both_directions` | PASS |
| O15b cross-flow state | `flow` separation | SQL:560, SQL:574, SQL:661; RJ:386 (OIDC callback refuses a GitHub state), RJ:392-398 (that state still completes GitHub) | SQL; RJ | PASS |
| O15c stale callbacks after disconnect/reconnect | revision binding → `Stale` | SQL:183, SQL:195; RJ:427 | SQL; RJ | PASS |
| O15d forced RLS | `FORCE ROW LEVEL SECURITY` | SQL:425, SQL:450, SQL:236 | `pg_github_installations::slots_are_tenant_confined_and_track_removed_access` | PASS |
| O15e unchanged OIDC login | migration `20261110000000_github_installations.sql` never names `auth_human_connections`; its only `CREATE TABLE` is line 24 | `crates/wyrd/wyrd-server/src/auth/callback.rs`:396 (OIDC browser login creates one session), :403 | `auth::callback::pg_tests::finish_creates_and_audits_a_browser_session` (in the 28 `auth::` lib tests); migration inspection | PASS |
| O15f suspension retains ownership | `set_github_installation_suspended` | SQL:248, SQL:269 (suspended installation still claimed against another tenant); RJ:492 (view keeps installation while suspended) | SQL; RJ | PASS |
| O16 = O2/O3 | as O2/O3 | as O2/O3 | as O2/O3 | PASS |
| O17 no token/key in view, errors, Debug or storage | `SecretString`; no token columns | RJ:240 (every view/error/body); SRV:201 (no mock code, token or App secret in any `github_installations` or `auth_login_state` row); CFG:6842; CLI:131; MCP:35; PY:99; TS:96 | journeys; SRV | PASS |
| O18 exact redacted view | `GithubConnectionView` | RJ:193-199 | RJ | PASS |
| O19 token only for associated, unsuspended, selected, accessible repo | `installation_token` | SRV:112, SRV:132, SRV:161-162 (deselected in Wyrd: refused without asking GitHub), SRV:167 (GitHub no longer grants), SRV:174 (suspended) | `pg_github_connection::installation_tokens_follow_repository_access` | PASS |
| O20 narrowed token request, no personal token persisted | `GithubApp::installation_token` | SRV:133-143 (exact `repository_ids` + three permissions), SRV:296, SRV:201 (no user token stored) | SRV | PASS |
| O21 installation events synchronize; bad signatures refused | `webhook` | RJ:480, RJ:492, RJ:525, RJ:533-536; `crates/shared/wyrd-crypt/src/lib.rs`:517-545 (RFC 4231 vectors, wrong and truncated tags refused) | RJ; `wyrd-crypt` lib tests | PASS |
| O22 access loss refuses only the affected repo | token facility reads the slot | SRV:161-163, SRV:167-168, SRV:174-178, SRV:281 | SRV | PASS |
| O23 disconnect fences then releases; installation reusable | `fence_and_release`: the conditional release under the slot row lock is the fence, so no separate fence step exists to order | RJ:444 (after a disconnect another tenant connects 101); SRV:281 (no token after disconnect); SQL:280-286 | RJ; SRV; SQL | PASS |
| O24 unrelated queued work survives | no queue mutation on access loss | SRV:285, SRV:293 | SRV | PASS |
| O25 no Cards/bindings created | connection writes only its slot | RJ:253-255 (Card count unchanged); SRV:203 (no verification binding) | RJ; SRV | PASS |
| O26 permissions, events, served URLs documented | DOC | DOC:25-29, DOC:36-38, DOC:40 | `mise run docs:check` | PASS |
| O27 architecture amendments | security posture + design | `architecture/wyrd-security-posture.md`:38, :495, :735; `architecture/wyrd-design.md`:931 | review | PASS |
| O28 existing permission/audit owners | every operation uses `identity_connections:write`, the permission the OIDC connection reads also require (`components/admin/identity.rs`:5) | SRV:272 (setup, callback, disconnect, connect and an allowed read each audited `allowed`), SRV:278 (viewer read audited `denied`); RJ:285-287 | SRV; RJ | PASS |
| O29 every surface | HTTP, `wyrd-client`, Rust/Python/TS SDK, CLI, MCP | SRV:218, SRV:238; RJ:193-225; PY:70-96; TS:63-86; CLI:88-123; MCP:95, MCP:166-169, MCP:179 | per-surface journeys; MCP catalogs | PASS |
| O30 guides match served operations, no UI | DOC | DOC:67, :81, :90 run by CLI:88-123 | `docs:check`; `github_journey::tenant_connects_github_through_the_cli` | PASS |
| O31 conditional association/revocation in SQL target | `queries/github_installations.rs` | SQL:142-200 | `pg_github_installations::association_is_conditional_on_the_bound_slot_revision` | PASS |
| O32 dedicated storage, unchanged OIDC trust | one new table only | `pg_migration.rs`:1013-1014; O15a–O15e rows | `pg_migration`, SQL, RJ, OIDC callback tests | PASS |

Ledger corrections after the checker pass: O28's observation said "existing `identity_connections` read/write", but only `identity_connections:write` exists and the OIDC connection reads already require it, so the GitHub read reuses it unchanged. O24 is asserted in SRV instead of SQL. The remaining checker findings were closed with the assertions cited above.

### Commands

- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E 'test(/^(pg_github_installations|pg_login_state|pg_migration|pg_platform_identity)::/)'` — 40 passed.
- `mise run test:sql` — 217, 7, 88 and 3 passed.
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner; cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(/^github_connection::/)"'` — 4 passed.
- `... cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^(pg_github_connection|pg_openapi_contract)::/)'` — 1 and 24 passed; `cargo nextest run --locked -p wyrd-server --lib -E 'test(/config::tests::github_/)'` — 4 passed.
- `... cargo nextest run --locked -p wyrd-cli --test cli -E 'test(/^github_journey::/)'` — 1 passed.
- `... cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(/^github::/) | test(/^discovery::/) | test(/^connectivity::/)'` — 7 passed.
- `... cargo nextest run --locked -p wyrd-auth --lib -E 'test(/^(callback|connections|platform_login|cli_logins)::/)'` — 8 passed; `-p wyrd-server --lib -E 'test(/^auth::(browser|callback)::/) | test(/^components::admin::identity::/)'` — 28 passed.
- Python: `pytest -m "integration and not local_dev" tests/integration/test_github_connection.py` — 3 passed; TypeScript: `test:bifrost:journey:typescript:inner` — 205 passed.
- `mise run fmt`, `mise run lints`, `mise run codegen:check`, `mise run docs:check`, `mise run check:tenant-isolation`, `mise run check:deps`, `mise run ts:napi:check`, `py:lints`, `py:typecheck` — pass.
- Lanes: the GitHub server, CLI and Rust journeys run in `test:wyrdstate:journey:inner`; the MCP journey in `test:bifrost`; Python and TypeScript in their journey lanes.

### Process exceptions and diagnoses

- **No RED.** Production code was written before the SQL, config, journey and integration tests, so no scenario has a recorded missing-behavior failure. This is a process exception, not a pass of the RED step.
- **Card baseline.** Symptom: RJ "a connection creates no Card" saw left 2, right 1. Evidence: `Deployment::key` bootstraps a Service with a root Card. Cause: the baseline was read before fixture setup. Fix site: the test's baseline is now taken after fixtures; the assertion is unchanged.
- **Disconnect 401 in SRV.** Symptom: DELETE answered 401. Evidence: `oneshot_authenticated` sends `x-wyrd-access-token`. Cause: the test sent `Authorization`. Fix site: the test uses the server's access-token header.
- **OpenAPI security.** Symptom: `pg_openapi_contract::unauthenticated_requests_match_each_operation_declared_security` failed for the webhook. Evidence: 503 without a token while the operation inherited document security. Cause: the HMAC-authenticated webhook did not clear the bearer requirement. Fix site: `security(())` on the webhook path in `components/github/routes.rs`.
- **MCP catalogs.** Symptom: `discovery`/`connectivity` catalog mismatch. Evidence: five verification tools from 86597947d were missing from the expected lists. Cause: catalog drift from that earlier commit on this branch. Fix site: the expected catalogs in both test files.
- **Mechanical deviations.** O24 was planned as a SQL integration test but is asserted in SRV, because a queued execution and the token facility meet only in the server. O15e uses the in-process OIDC callback tests; the Keycloak/Dex `identity_e2e` lane was not run.
- **Non-goals.** No UI, no Card or binding writes, and no PR, check-run or source work. `auth_human_connections` is unchanged.
