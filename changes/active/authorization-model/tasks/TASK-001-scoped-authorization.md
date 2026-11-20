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
- Not yet proven in this cycle: direct/group/custom-role variants, exact UID vs future
  versions, CLI/Python/TypeScript/MCP/raw SQL, review sets/executions/evidence/approvals,
  Oracle row filtering, scoped invitations, password-change refresh refusal, GitHub
  configuration permission. Each remains FAIL until proven.
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
