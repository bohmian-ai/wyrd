---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-authorization-model
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-010]
depends_on: []
---

# Deliver scoped authorization through the existing server and clients

An administrator can give Alice access to payments, or provision checkout's
runtime account, through HTTP/CLI and the first-class clients. Every ingress
enforces those grants, including retained evidence and raw SQL. Existing tokens
follow current server authority while restricted credentials retain their limits.
This is the complete backend outcome; UI work must not be needed to close an
authorization bypass or prove the programmatic journeys.

## Approach and connected owners

Follow [the approved spec](../spec.md), [roles and bindings](../design/notes.md#roles-groups-and-bindings),
[effective authority](../design/notes.md#effective-authority-and-freshness),
[derived data](../design/notes.md#derived-data-and-sql), and
[Rust responsibilities and token flow](../design/rust-model.md).
The security posture's [authorization owner](../../../../architecture/wyrd-security-posture.md#authorization-and-policy)
and Bifrost's [public boundary](../../../../architecture/bifrost-design.md#public-surface)
are reconciled with this model.

Extend the existing role, account, credential and checker owners. They already
own account provisioning, exchange, root validation and audited decisions; the
missing capabilities are scoped assignments and server-resolved current grants.
The shared verifier cannot acquire them because it is synchronous and SQL-free.
The server hosting boundary acquires authority before handing a caller to Gate
or another consumer. Oracle consumes a visibility projection of this same
authority, rather than introducing another grant resolver or policy service.

Carry the flow through these existing owners and their consumers:

- `wyrd-spec/src/auth`, shared `wyrd-runtime` permission/principal/request-context
  and built-in-role modules: operation bundles, typed scopes/bindings/ceilings,
  containment and runtime views. Reuse Resource/Action/Permission semantics;
  move necessary pure wire definitions to the contract owner and re-export,
  without duplicate DTO vocabulary or IO in contracts.
- `wyrd-sql` auth queries/rows and schema definitions: scoped bindings, local
  groups, revisions and the named credential/login/invitation changes.
  [Persistence and authority changes](../spec.md#persistence-and-authority-changes)
  is the schema authorization, including support-column/index/policy limits.
  Add ordered migrations where needed; never edit applied migrations or reset
  a database. No historical conversion, parent backfill or cutover machinery.
- `wyrd-auth` issuance, CLI login, refresh and credential owners, plus shared
  issue/verify shells: cached current authority, revision confirmation and
  invalidation, identity/restriction claims, local device anchors, scoped
  invitation consumption, delegation and accepted ceilings. Replace superseded
  role-only assignment callers; keep bootstrap and platform owners intact.
- `wyrd-server` principal/admin/auth composition, Card, verification, GitHub,
  storage and gRPC owners; Bifrost Gate/Scribe/Oracle/table/peer owners: capture
  and retain trusted read parents, filter before expressions/aggregation and
  use current authority at HTTP, MCP, gateway, OTLP, native Bifrost and peers.
  Preserve the separate owning workload, subject, Verifier and actor fields.
- `wyrd-client::Principals` and existing auth/Card/Bifrost clients, CLI principal
  and login commands, server MCP projections, Rust/Python/TypeScript SDKs:
  consume the same typed administration/access-summary contracts and errors.
  Update exports, authored Python stubs, generated projections, affected fixtures,
  examples and served documentation together. Minimally adapt existing UI
  consumers to changed contracts; new Access pages belong to TASK-002.

Inputs are real administrator requests, local/provider login state, existing
Card/Service identity, signed credentials and accepted execution context. Writes
produce tenant-qualified bindings and revised authorization state; requests
consume compiled account views, not roles copied from a token. Admission emits
trusted parent and frozen authority metadata through existing execution/Scribe
owners; point reads, downloads and Oracle scans consume that exact metadata.
These working contracts, access summaries, local approval/refresh behavior and
filtered reads are the concrete outputs consumed by TASK-002.

## Acceptance and implementation evidence

Execute these three cycles in order. Begin each with its smallest observable
case, then extend that journey with the listed variants. Add actual assertion
locations, focused commands, observed RED/GREEN and limitations to each entry.
For new tests, confirm their actual names/targets and record commands before RED;
the paths below identify existing owners, not invented test selectors.

### 1. Give Alice payments access and observe the same boundary everywhere

Maps AC-001 to REQ-001, REQ-002, REQ-006, REQ-007, REQ-008 and REQ-010.
Start with fresh repository PostgreSQL, WyrdTestServer, a tenant admin, local
Alice, payments and another Repository with retained reviews sharing a Verifier.
No IdP is configured. Through public principal administration, assign viewer at
payments family. Alice logs in and explicitly approves the existing CLI device
code; use the returned saved login and real client credentials for reads.
First prove payments succeeds and the other family is absent. Expected RED:
the current broad assignment contract cannot deliver this scoped boundary.

Extend the same observation to direct/group/custom-role grants, exact UID versus
future family versions, and HTTP/CLI, Rust/Python/TypeScript, MCP and raw SQL.
Compare actual membership, pages and counts across review sets, members,
executions, inputs/outputs/events/results/evidence/approvals and downloads.
Opening the shared Verifier definition still needs its own grant; approval
needs the proper operation and readable parent. Retain visibility after queue
cleanup, Card deletion and GitHub disconnect without a provider request.
Cover memtable and published sources, views/joins/subqueries, reusable query
state and peer execution; no table grant, unknown parent or internal read may
disclose the other family's rows, even through aggregation or predicates.
Ordinary tables, audit and gateway payloads retain their separate gates.

Use programmatic create/invite/manage operations as both admin and non-admin.
Scoped invitation acceptance creates the intended User/bindings together;
missing/invalid scope or stale issuer/role authority creates neither. Exercise
one-use/expiry and local password-anchor refresh refusal after password change.
GitHub configuration/inventory remains denied to Alice; explicit configurator
access does not grant OIDC administration or data. Preserve provider identity,
rename, duplicate-family and new-work admission checks. Cross-tenant and
invalid/inapplicable assignments fail without grants or unauthorized work.
Verify read-only built-ins, applicable operations, source summaries, last-admin
protection and ordinary credential secret/rotation behavior through these owners.

Proof: tier-1 public SDK stories in `sdks/wyrd-sdk-{rust,python,ts}` existing
principal-role integration owners; CLI `tests/principal_journey.rs` and
`tests/cli_login_journey.rs`; HTTP server `tests/integration` principal,
verification, Repository and approval owners; MCP `tests/bifrost/mcp/principals.rs`;
Bifrost `wyrd-testing/tests/bifrost/oracle` journeys for actual server/query
results. Tier-2 SQL tests prove atomicity/RLS; redux Oracle integration tests
pin storage/plan seams. Lower tiers do not replace returned-data journey proof.

Evidence (in progress):

- Smallest case — PASS (Rust SDK only). `principal_roles::a_family_bound_viewer_reads_only_that_family`
  (`sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs`): no IdP; admin
  `create_user` binds viewer at the `Verifier/code-review` family; Alice signs in
  with her local password and approves the CLI device code
  (`HumanSso::local_cli_login`); her saved login lists only `code-review`, reads it,
  and gets `WYRD_REGISTRY_404_CARD_NOT_FOUND` for `unrelated-check`.
  Command: `scripts/postgres/with-test-postgres.sh -- bash -l` running migrations then
  `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=principal_roles::a_family_bound_viewer_reads_only_that_family)'`.
  RED 1: `the device login begins: ... invalid_request` (a tenant with no IdP could not
  approve a device code). RED 2: `Alice lists Verifiers: PermissionDeniedRbac ... lacks
  Cards/Read ... scope all` (Card reads needed tenant-wide read). GREEN after commit
  `9995904d8`. The test first asserted `403` for the hidden family; it was corrected to
  not-found per REQ-006 ("inaccessible/unknown exact objects have ordinary not-found
  behavior") and REQ-009 ("Unknown and inaccessible targets remain indistinguishable").
- Card visibility owner: `EffectivePermissions::card_objects`
  (`crates/shared/wyrd-runtime/src/permission.rs`, unit test
  `permission::tests::card_objects_are_exactly_the_granted_cards`, GREEN) feeds
  `authorize_card_objects`/`require_visible` (`wyrd-server` cards routes) and the SQL
  `push_visible` predicate shared by Card list and version list, so pages and cursors
  never count a hidden Card. Point, latest, by-ref, artifacts, download-init,
  local-blob and cached hosted-evidence reads apply the same objects.
- Local device approval: a recent password/provider browser session approves with its
  own human anchor (`CliLogins::approve_signed_in`, `device_decision`); otherwise the
  provider redirect is unchanged.
- Oracle managed-row visibility. Owner: `restrict_verification_rows` and
  `authorize_resolved_tables` (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs`),
  applied in `Oracle::plan_logical` before optimization. Every `bifrost.verification`
  scan, including views, joins and subqueries, gets a filter on the registry-stamped
  `subject_kind/subject_name/subject_card_uid` parent. A restricted reader also gets a
  projection that nulls every column outside its visible parents, so pushed or reordered
  user predicates never evaluate hidden values. DataFusion's `FilterExec` absorbs parent
  filters ahead of its own predicate (`datafusion/physical-plan/src/filter.rs`
  `handle_child_pushdown_result`), so a bare filter could not meet the design rule
  against a movable predicate. The plan stays native, so peers execute the same masking.
  Rows without a provable parent are read by nobody. Execution read at any scope admits
  the managed tables and only them. A table grant alone reads no row.
  Unit tests: `oracle::tests::verification_rows_are_filtered_by_the_readers_card_scope`
  (family, exact, table-only, tenant-wide, join, `IN` subquery, CTE, count, other-family
  predicate, a failing cast over a hidden row) and
  `oracle::tests::execution_read_admits_only_managed_verification_tables`.
  Command: `cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::<name>)'`.
  RED 1: the family reader saw `["o1","p1","u1"]`. RED 2: the masking case failed with
  `Cannot cast string 'hidden' to value of Int64 type`. GREEN after both changes.
  TDD VIOLATED for the admission slice (`authorize_resolved_tables`/`admit_capability`):
  the test followed the code.
- Trusted parent on evidence and approvals. The approval writer
  (`verification/approvals.rs::row`) and the deferred-evidence writer
  (`verification/evidence.rs::row`, subject per subscriber from
  `VerificationService::subscribers`) now stamp the execution's exact subject. Scribe
  then stamps its kind and name from the registry. Results, inputs, outputs and events
  already carried the subject through `ResultRun.card_uid`. No historical conversion.
- Diagnosis, journey regression after row visibility (40 of 78 SDK journeys failed).
  Symptom 1: `role seeds: Io("role datasets_reader names a scoped operation ...")` at
  `support/mod.rs:240` and `query_bifrost.rs:776`. Cause: `WyrdTestServer::seed_role`
  refused scoped operations after roles became operation bundles, but 84 fixture
  callers seed a role at one scope and then bootstrap with it. Fix site:
  `wyrd-testing/src/server.rs`. `seed_role` records the operations' single shared scope,
  and `grant_role`/bootstrap bind there. A role with several scopes is refused.
  Symptom 2: `SdkVerificationWaitTimedOut ... last_status: Null` with no error in a
  `WYRD_LOG=wyrd_server=debug` trace. Every `GET /v1/verification/executions/{id}` read
  returned no row. Cause: `SystemReadAuthority` held only table-UID grants, so the new
  parent filter hid every managed row from the server's own record reader. Fix site:
  `verification/authority.rs::SystemReadAuthority::resolve`, shared by the execution,
  group, drift, inventory and runner readers. It adds tenant-wide execution read when a
  requested table is in `bifrost.verification`. Caller-facing reads authorize the
  caller's access to the execution before this read.
- Diagnosis, the 15 journeys still failing after the fixes above.
  Symptom A, fixture: multi-scope roles and the retired `{"verifier": uid}` grant scope
  were refused. Fix site: `seed_role` stores one part role per scope and binds each part
  at its scope (`<name>_scope<n>`, because `RoleRef` forbids `.`). Grants now use the
  Verifier Card version scope; an execution read is `executions:read` there, since
  `verifier:read` is not valid at a Card scope.
  Symptom B, `QueryVisibilityUnavailable`, and GitHub delivery "recorded outcome is not
  visible yet". Status and delivery rows carried no subject, so no parent was provable.
  Fix site: `ScribeWrite::status` and `github/delivery.rs::row` stamp the execution's
  subject. The cancel SQL returns `card_uid` for the same reason.
  Symptom C, `PEER_SECURITY` on forwarded queries. `oracle/forwarding.rs::validate_context`
  demanded `bifrost_query:read` while `query/service.rs::admit_capability` admits Card
  readers. Both now use `EffectivePermissions::reads_bifrost`.
  Symptom D, `ROLE_CORRUPT` 500 for every caller in a tenant that held one corrupt role.
  `TenantAuthority::compile` failed the whole tenant. Fix site: `wyrd-auth/authority.rs`
  marks only the corrupt binding's subjects. `TenantAuthority::check` refuses them at
  issuance, API-key exchange and `current_grants`.
  Symptom E: a saved login dropped the gRPC endpoint (`TransportDown`); fixed in the
  `principal_roles` fixture.
  Symptom F: a hosted `bifrost.query` refusal surfaced as `WYRD_TOOL_500_CALL`.
  `skald-tool/toolerror.rs` lost every `WYRD_VALA_*` code. It now reconstructs Bifrost
  unit codes (`toolerror::tests::tool_errors_project_onto_the_catalog`; test written after
  the fix, TDD VIOLATED).
  Diagnostician report: no production defect remained in five journeys whose assertions
  predate REQ-006 and the role table. Card read admits the query surface, so an uncovered
  ordinary table is `WYRD_VALA_403_QUERY_FORBIDDEN`, not RBAC denial. An editor cannot
  approve ("no approvals or access administration"). The hosted no-domain case must query
  an ordinary table (`bifrost.traces.spans`). The grouped coding journey binds
  `bifrost_query:read` at the verification schema. These were updated in the Rust, Python,
  TypeScript and MCP counterparts.
- Diagnosis, the MCP, Python and TypeScript journeys after those fixes.
  Symptom G: PR supersede returned 503, "no column found for name: card_uid". Cause:
  `CancelledRow` gained `card_uid`, but the three PR statements in
  `verifier_executions/pull_requests.rs` did not return it. Fix site: those `RETURNING`
  lists; `CANCEL_PRINCIPAL_SQL` was already updated.
  Symptom H: delegating to a new role-less account answered 401 `inactive`. Cause:
  account and credential writes never advanced the authorization revision, so the
  authority cache never reloaded the new account, and a revoked key stayed usable until
  the freshness bound. The spec requires every access-affecting change to advance it.
  Fix site: the `wyrd-sql` account and credential writers advance the revision
  themselves: user and Service insert, suspension and deletion, workload restriction,
  key insert and key revocation.
  Diagnostician report (stale tests, no further production defect):
  - The MCP catalog tests predate `github_connection.review_pull_request` (`f4ce139ce`).
  - Tenant-wide `cards:read` reads executions, outputs and publications (REQ-006), so
    those refusal cases now use a `verifier:run`-only caller.
  - An exact execution read is `executions:read` on the execution's subject Card, its
    trusted parent, not the Verifier.
  - The access summary promises no grant order, so the TypeScript `principal-roles`
    assertions sort.
  - The speculative-disposition reader held only `verifier:read`, and the trace refuses it
    with `lacks Executions/Read`: `verifier:read` covers the Verifier Card alone. The
    reader now holds `executions:read` in the Rust, Python and TypeScript journeys.
  - The MCP onboarding check that no tool name contains "pull" misread
    `github_connection.review_pull_request`; it now tests the operation's leading word.
- Open, unreproduced: `verifier_orchestration::repeat_stops_or_exhausts_without_overlapping`
  timed out once (`last_status: null` after 98s) in a 14-test parallel run. Alone it passed
  in 10s; two traced reruns of the same 14-test set passed 14/14 in 38s. No cause has been
  established and no fix has been made.
- TDD: these production fixes preceded their focused failing tests, which is a process
  violation. They are the hosted refusal code (`toolerror.rs`), the PR `RETURNING`
  lists, and the account and credential revision advance. The journeys above observed
  each defect first; the unit test for the hosted refusal code was added after the fix.
- GitHub configuration permission (REQ-007) — PASS (HTTP and MCP).
  `pg_github_connection::github_configuration_needs_its_own_permission`
  (`crates/wyrd/wyrd-server/tests/integration/pg_github_connection.rs`):
  - A `github_connections:read`+`write` configurator reads (200) and connects (201), and is
    refused OIDC administration (`GET /v1/identity/oidc/connections` 403) and Card reads
    (`GET /v1/cards` 403).
  - An `identity_connections:write`-only administrator is refused the read and connect
    (403); a reader of the payments Repository family is refused the read (403).
  - `installation_tokens_follow_repository_access` now pins the audited permission per
    operation (`github_connections:read` for the read, `:write` for the others).
  Command: inside `scripts/postgres/with-test-postgres.sh` after migrations,
  `cargo nextest run --locked -p wyrd-server --test integration -E 'test(/^pg_github_connection::/)'`.
  RED: `the configurator reads` got 403, because `GithubConnections::authorize` checked
  `identity_connections:write`. GREEN: 5/5 after `6b7ef570f`. MCP
  `-E 'test(/github|discovery|connectivity/)'` 10/10.
- Password-change refresh refusal — PASS (Rust SDK).
  `principal_roles::a_family_bound_viewer_reads_only_that_family`
  (`sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs`): after Alice changes her
  local password (`HumanSso::change_local_password`) and her saved access token expires,
  a fresh client from the saved login fails to connect with
  `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE`, reason `refresh_refused`. Command:
  `cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all
  -E 'test(=principal_roles::a_family_bound_viewer_reads_only_that_family)'` inside
  `with-test-postgres.sh`, 1/1. No RED: the server already compared the password revision
  on refresh (`wyrd-auth/src/issuance.rs`); the first run observed the refusal.
- Diagnosis, the full server integration run after the revision advance.
  Symptom I: six `pg_verification_routes` tests answered 401 `inactive` for a caller
  bootstrapped moments before its request. Evidence: `AuthorityCache::authorize`
  refused any subject, actor or key missing from a warm snapshot; the creating write had
  advanced the revision, but this process had not yet applied the notification. Cause: a
  warm snapshot cannot tell "created after my load" from "removed". Fix site:
  `crates/wyrd/wyrd-auth/src/authority.rs`. `VerifiedToken` now carries `iat`. When a
  token issued no earlier than the snapshot's load second misses, the cache reloads once
  (the sanctioned cache-miss statement). An older token is refused without a reload, so a
  removed account's tokens cost no statement. Every `VerifiedToken` construction was
  updated (`wyrd-auth-verify`, `issuance.rs`, `principal_extractor.rs`, the Gate test).
  RED: the six tests in the full run. GREEN: `-E 'test(/^pg_verification_routes::/)'`
  answers no 401 `inactive`.
  Diagnostician report, stale tests in the same run, no further production defect:
  - Role assignment bodies use `{"assignments": [{"role", "scope"}]}`, and stored
    bindings are in `wyrd.auth_role_bindings` (`pg_browser_sessions`,
    `pg_verification_runtime`, `pg_verification_groups`, `identity_e2e`,
    `platform_admin_e2e`). The retired `PUT /v1/principals/{id}/roles/{role}` is now
    `POST /v1/access/bindings`.
  - A decider needs `verifier:approve` and `executions:read` on the parent
    (`inspection.rs`). The editor deciders in `pg_approvals`, the PR extraction harness
    and the CLI GitHub journey are approvers that also read; for delegation, the user and
    the actor both are.
  - `WyrdTestServer::retained_records` inspects every parent's managed verification rows,
    so its inspection context also holds `executions:read`.
  - The PR "Verifier only" reader holds `executions:read` on the Verifier Card, which does
    not reach the Repository parent.
  - The CLI journeys use `--grant ROLE:SCOPE`.
  Symptom J: a reader holding a key whose id the snapshot did not yet hold was authorized
  with the stale grants and no key ceiling (`direct_execution_refusals_are_stable`, 403).
  Cause: `TenantAuthority::authorize` treated an unknown key as active without a
  ceiling, but the snapshot holds every stored key, revoked ones included, so an
  unknown key was committed after the load. Fix site: the same method; an unknown key
  is now a miss and reloads under the rule above. RED:
  `authority::tests::an_unknown_key_is_a_miss` failed on the unknown-key assertion. GREEN:
  `cargo nextest run --locked -p wyrd-auth --lib -E 'not test(/pg_tests/)'`, 23/23.
  Symptom K (regression from the Symptom J fix): `identity_e2e::human_oidc_login_journey`,
  `revocation_journey` and `revoking_a_human_kills_the_session_refresh_authority` were refused
  delegation with `WYRD_AUTH_401_CREDENTIAL_REVOKED` (identity_e2e.rs:226). Cause: a refreshed
  human token carries the consumed refresh row as `credential_id`
  (`TenantGrant::Refresh`, issuance.rs `credential_id`), which is not a key. Treating every
  unknown credential as a miss refused it. Fix site: `TenantAuthority::predates` decides the
  one reload (issued no earlier than the load, and the account, an actor or the credential
  is unknown). `TenantAuthority::authorize` again gives an unknown credential no key ceiling.
  Unit test: `authority::tests::an_unknown_credential_reloads_only_for_a_newer_token`
  replaces `an_unknown_key_is_a_miss`. TDD: the production fix preceded this unit test
  (process violation); the identity journeys are the RED.
  Third diagnostician report:
  - Principal revocation: `platform_admin_e2e::a_tenant_revokes_a_compromised_principal_with_its_reason`
    and the two `identity_e2e` revocation journeys asserted that a pre-revocation token
    "keeps its snapshot". Posture 325-329 and REQ-005 make that stale: an issued token
    spends current account authority. The tests now assert 401 (`details.reason ==
    "inactive"`) at the next request.
  - Key revocation: posture 328-329 says "Credential revocation ends renewal; it does not
    itself withdraw an already-issued bearer". `TenantAuthority::authorize` refused a
    revoked key's bearer, which is drift introduced in 8e33913ed. The key-rotation journeys passed only
    because the notification had not yet landed. Fix site: `KeyState` is now
    `Bounded(ceiling) | Corrupt`, and a revoked key's bearer stays bounded by its ceiling.
    The unused `active` key column is removed from `AUTHORITY_STATE_SQL`.
  - `pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees`: another
    test's runtime on the shared database claimed its queued execution. `RUNNABLE_TENANTS_SQL`
    is cross-tenant by design. The owning lane `test:verification:sandbox` isolates this
    test; the failure came from my combined ad-hoc invocation, not from a lane.
  Second diagnostician report (no further production defect):
  - `WyrdTestServer::bootstrap_user` bound every role tenant-wide through the provider
    replacement, ignoring the scopes `seed_role` recorded. Scoped negative cases passed
    over-granted. Fix site: `wyrd-testing/src/server.rs::bootstrap_user_in_tenant` binds
    each seeded part at its scope with provider source. This corrects the earlier
    "bootstrap binds there" note.
  - A PR execution's parent is its Repository (`STATUS_SQL` `card_scope` from
    `e.card_uid`); `executions:read` on the Verifier alone is refused once the fixture
    binds it at its scope.
  - Retained PR membership: a tenant-wide Card reader reads membership (REQ-006). The
    refusal cases are a caller with no read (`DENIED_RBAC`) and a reader of another
    Repository family, who sees no rows and counts zero.
  - `pg_merge_http_protected` minted tokens for accounts that do not exist; it now
    bootstraps a User. `pg_operator_connection_routes` stored role operations with a
    `scope` field, which operations no longer carry.
  - The verification route docs described `verifier:read` on the Verifier; they now
    name `executions:read` on the trusted parent.
  - `pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees`: see the
    third report below (the cause is test isolation).
  Fourth diagnostician report (revoked account still authorized in-process): an in-process
  `WyrdTestServer` never ran the `authorization_cache` worker (listener plus confirm), so a
  revocation never reached a snapshot that already held the account. Fix site:
  `WyrdTestServerBuilder::start_in_process` spawns that worker under the state shutdown
  token. The two `identity_e2e` revocation journeys now assert 401.
  Fifth diagnostician report (Python):
  - `test_principal_roles.py` assumed a grant order the access summary does not promise.
    Its `grants` helper now sorts, as the TypeScript twin's does.
  - The MCP reader tests in `test_agent_frameworks.py` exposed a production defect from
    8e33913ed. Direct execute resolved the target before authorizing, so a caller without
    `verifier:run` got 404 for an unknown target but 403 for a real one, which reveals
    whether a target exists (REQ-009, posture 665-666, 732). Fix site:
    `VerificationService::run_direct` checks `verifier_run_any` for an unresolved target,
    as enqueue does. RED: the new reader-unknown-target case in
    `pg_verification_routes::direct_execution_refusals_are_stable` (404 instead of 403).
    GREEN: `-E 'test(/^pg_verification_routes::/)'`, 27/27.
  Runs at e5101241d:
  - `test:identity:journey` exit 0: 31/31 server journeys plus the UI BFF, CLI device
    login and each SDK saved-login journey.
  - `test:rust-sdk:journey`: 195/195.
  - `ts:test:integration`: 40/40 files.
  - `py:test:integration`: 5 failed before the fixes above; after them, the affected files
    `test_principal_roles.py` and `test_agent_frameworks.py` pass 14/14. The full Python lane
    has not been rerun.
  - CLI `--test cli`: 59/61. `cli_device_login_journey` belongs to the identity lane (green).
    `retired_tables_are_unknown` failed once at server start ("peer security audit cannot be
    composed") and passed twice alone; no cause has been established.
  - Server `pg_*`/`platform_admin_e2e` combined run: 439/441 at 2936d9e4c; both failures
    were diagnosed above. `test:verification:sandbox` 34/34.
  - `lints` and `py:lints` are clean.
- Not yet proven in this cycle: direct/group/custom-role variants, exact UID vs future
  versions, CLI/Python/TypeScript/MCP, review sets/executions/approvals through
  dedicated reads, peer execution of the masked plan, scoped invitations. Each remains
  FAIL until proven.
- TDD: VIOLATED for slices committed in `8e33913ed` and `0c3a7d36f` (production code
  preceded tests). Test-first resumed from the smallest case above.

### 2. Provision one Service and run two releases with exact attribution

Maps AC-002 to REQ-001, REQ-003, REQ-005 and REQ-010.
Start with two checked-in checkout Service release fixtures and their dependency
Cards. Admin provisions checkout-runtime through public HTTP/CLI, with its
Service family restriction and explicit operation bindings. Inspect the same
account/family/grants through the public access summary. Use one returned key
and principal for real SDK operations under each release's workload_uid.
Assert both operations complete and retained records keep each exact root,
owning Card, subject, Verifier and principal/actor chain. Expected RED: provisioning
or downstream scoped checks still depend on broad role assignments/token grants.

Within this journey, try an ungranted dependency operation, foreign/malformed
root, signed-root mismatch and a duplicate live runtime account for the family;
no unauthorized execution, evidence emission or account mutation may occur.
Preserve root-required behavior and unrestricted automation. Declaring a release
creates no account/grant/key; dependency references alone grant nothing.
Exercise restricted Service credentials and delegation without replacing
WorkloadRestriction with CardPermissionScope. Preserve optional federation's
trust checks, explicit issuer grants and source-qualified provider assignments;
authentication alone grants no fallback viewer authority, and provider updates
leave direct bindings intact.

Proof: tier-1 principal-role/runtime stories through the three public SDKs,
in-process CLI and HTTP/MCP projections; use the existing Service provisioning
and WyrdState workload-root path, then query actual retained attribution.
Shared auth/runtime unit tests cover pure root/ceiling containment and SQL
integration tests cover family uniqueness. Existing federation regressions run
with repository identity providers; local Service journeys require none.

### 3. Change access under existing credentials and observe bounded enforcement

Maps the programmatic portion of AC-003 to REQ-001, REQ-004, REQ-005, REQ-008
and REQ-010. TASK-002 supplies the Copy access request presentation in REQ-009.
Start with existing Alice sessions/bearers, restricted keys, a delegated token
and admitted work. Admin grants another family using HTTP/CLI; the ordinary
existing bearer gains access after normal propagation, while frozen key/token
ceilings and accepted work do not expand. Remove grants, group membership or
an actor's authority and observe the same callers lose affected access.
Expected RED: today's token grant snapshot retains removed grants and cannot
use newly assigned ones until replaced.

Use real replicas over shared PostgreSQL to observe committed changes within
five seconds. Prove warm requests use resolved memory without permission SQL
or group recompilation, and unchanged background confirmation does not rebuild
grants. Interrupt confirmation: bearer requests with previously confirmed state
may allow only within five minutes since that confirmation; requests, renewal
and failed refresh cannot renew it. Known invalidation, cold state and expired
freshness refuse as unavailable, not ordinary missing permission. Streams stop
before subsequent output when authority becomes unusable. Isolate bearer proof
from the browser's independently database-dependent session validation.

Exercise absent/empty/malformed ceilings, family/exact intersections, current
credential state and inherited/delegation participant restrictions. Reject
unrepresentable intersections without signing a broader token. Retry/restart
accepted work with newer grants and prove its frozen limit, original attribution
and existing principal-revocation cancellation survive. No SYSTEM substitution,
anonymous fallback, old-token reader or parallel assignment store is permitted.
Inspect actual issued claims for the revised version/identity/restriction fields,
absence of roles/grants and preserved audience/root/delegation validation.

Proof: tier-1 public clients against real replicas in server integration and
WyrdTestServer journeys, plus server/Bifrost stream and peer consumers. In-owner
auth/runtime tests can deterministically exercise clock/freshness and compile
counters; combine them with real database notification/replica proof rather
than using long sleeps or substituting unit tests for propagation. SQL/auth
integration proves transactional revisions and invalidation; issuer/verifier
unit tests pin claim refusal. Record performance and wait evidence here.

## Verification and handoff

Use the existing `mise.toml` runners and repository-managed setup, with Cargo
commands sequential. Run newly added tests by their recorded exact selectors
first. Owner lanes are `test:principals:unit`, `test:principals:integration`,
`test:cli:journey`, `test:identity:journey` for optional-provider regressions,
and the affected `test:bifrost:journey:{oracle,server,otlp,mcp}` and gateway
journey slices. SDK stories use `wyrd-ai --test integration` (journey profile),
`py:test:integration` and `ts:test:integration`; select affected stories during
iteration using those runners' setup. Local login proof must run without an
IdP even though the existing provider regression lane starts Keycloak/Dex.

Complete touched owner integration/unit tests, `codegen:check`, `check:deps`,
`check:tenant-isolation`, `fmt`, `lints`, affected Python format/lint/type checks
and TypeScript format/lint/type/N-API checks. `ui:check` applies to minimal
existing-UI adaptations. Integrated review owns full shipped-surface journeys;
do not use a broad gate as a substitute for focused task proof.

For the hot paths, enumerate every added/moved PostgreSQL, Scribe, object-store
or Vault wait and match it to the sanctioned boundary. Run full clean
`mise run bench:l1:verification`, `mise run bench:l1:gateway` and
`mise run bench:l1:bifrost`, each VALID and compared with the newest record for
the same experiment/host in `benchmarks/history/`. Restore development SDK
builds after diagnostic benchmark builds as the runners require. Smoke,
partial and dirty runs are insufficient. Warm checks add no IO; audit staging
must remain nonblocking. No measurement is claimed by this plan.

Hand off the working contracts and their actual consumer proof to TASK-002.
Additional schema purposes or changed authorization semantics require a spec
revision; reversible implementation details remain with the implementer.
