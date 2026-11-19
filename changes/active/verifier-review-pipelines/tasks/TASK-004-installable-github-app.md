> **REJECTED — 2026-11-09.** The user rejected this implementation and directed that it be scrapped. Implementation source has been restored to `dfd19aa129830696d68cf8d4a09c144805847345`. This task and its evidence are historical only: no PASS claim authorizes acceptance, reuse, remediation, or further implementation. A replacement requires explicit user agreement.

---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-verifier-review-pipelines
spec_revision: 24
contract_status: complete
replaces: TASK-004-github-repository-automation.md
depends_on: [TASK-001-R5]
requirements: [REQ-024, REQ-025, REQ-026, REQ-027, REQ-028, REQ-029, REQ-030, REQ-032, REQ-034, REQ-035, REQ-036, REQ-037, REQ-038, REQ-040, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058, REQ-059]
invariants: [INV-002, INV-003, INV-006, INV-007, INV-010, INV-012, INV-013, INV-017, INV-018]
acceptance: [AC-001, AC-002, AC-011, AC-014, AC-015, AC-016, AC-019, AC-026, AC-027, AC-028, AC-030, AC-031, AC-032, AC-048, AC-049, AC-050, AC-051]
---

# Install Wyrd's GitHub App and receive PR Checks

## Outcome and Value

A team installs the deployment's existing Wyrd GitHub App, selects repositories,
connects the installation to its Wyrd tenant, and receives Checks from its
configured Verifiers. SaaS customers never create an App or upload credentials.
A self-hosted operator configures their own App once; their teams use exactly
the same onboarding and runtime code.

Before: no accepted GitHub integration exists in the restored Wyrd source.
After: a team can start from GitHub or Wyrd, complete verified onboarding, and
open a PR that runs an ordinary registered Verifier and receives its exact-head
Check. Full code-review extraction/pipelines remain Tasks 005–006; this task
proves the lifecycle with an ordinary no-input Verifier.

This replaces the deprecated task in full. Its implementation and green evidence
at `487ccb0b6` are not the implementation starting point or acceptance proof.
The local Wyrd source has been restored to the pre-task tree at `8f1bb789e`;
confirm the accepted foundation before implementation. Preserve earlier tasks.

## What Changes and What Must Stay True

### Settled App and connection decisions

- One configured App/host per deployment, many tenant installations. Operator
  configuration supplies App identity/slug, origins, keys, webhook secret and
  OAuth client credentials through existing secret providers. Self-hosting and
  GHES change configuration, never application behavior or enterprise gates.
- Register repository permissions Contents read, Pull requests read and Checks
  write (Metadata read is implicit), with pull_request, push, check_run,
  check_suite and delete subscriptions plus GitHub's installation/access events.
  No source-write, workflow-write, comment or organization-member permission is
  requested. Setup/callback/webhook URLs are exact deployment URLs; use the
  host's documented GitHub.com or GHES web/API/OAuth endpoints, not string
  substitution of an API base into an installation URL.
- Use GitHub's Setup URL, then separate user authorization at the OAuth callback.
  These are different protocol steps; do not enable automatic OAuth-on-install
  alongside Setup URL. Both GitHub-first and Wyrd-first entry reach the same
  Rust-owned flow, using existing Wyrd login and explicit tenant choice.
- Correlation, browser binding, five-minute database-clock expiry, current human
  administration, verified installation/user access and repository selection are
  fixed in [public contracts](../public-contracts.md#github-app-installation-and-tenant-connections).
  A bare installation ID or valid state is not sufficient authority. GitHub
  consent grants no Wyrd permission and is not a Wyrd login.
- Confirmation uses the existing allowlist to connect and enable selected repos.
  There is no second persistent repository snapshot/grant model or refresh-token
  service. New repository connections reuse the same verified selection flow.
- Existing deployment admission and Verifier/execution budgets enforce resource
  limits. Connection setup adds no repository quota, limit snapshot or limits API.
- Preserve one active installation per tenant and one tenant per installation.
  Disable retains its claim; disconnect releases it after fencing affected work.
  Neither removes deployment credentials or uninstalls another tenant's App.
- An unconfigured optional App refuses GitHub connection explicitly while
  unrelated Wyrd capabilities remain available. Invalid configured settings use
  existing typed startup validation. All provider destinations use
  current network screening, credential authority and bounded transport rules.

### Library decision — keep the generic change

**Keep**, do not revert, external commit
`9fdf6dc77a329f34d96b7ad4f397331d2d18216b` on
`bohmian-code-review`'s `change/github-app-checks-webhooks` branch. Inspection of
`GitHubApp::with_transport/installation/installation_token`, Check methods and
webhook helpers establishes that they are independent provider protocol, not
Wyrd tenant-App policy. The App client accepts a consumer-signed JWT; its added
HMAC dependency verifies webhook signatures, not JWT signing.

Extend that same client only for missing generic authorization-code exchange,
user-accessible installation/repository operations and required branch-deletion
event decoding. Wyrd owns App configuration,
JWT signing and secret resolution, installation URLs and callbacks, browser
transactions, tenant ownership/RBAC, listeners, durable work and when/why Checks
are published. The library stays standalone with no Wyrd dependency, Wyrd route,
tenant-administration schema or workflow service. Its generic review helpers,
GitHub client and storage traits/implementations remain independent facilities.
Wyrd supplies deployment/tenant-specific adapters under its existing owners.

Deliver a verified immutable descendant of that external commit and pin it in
Cargo.toml/Cargo.lock only for the server. Never restore the rejected Wyrd code,
pin a mutable branch/path, or assume the old external test covers new OAuth.
No merge, push, tag or release is authorized by this task; external publication
and the caller's final pin availability remain explicit delivery actions.

### Verification, configuration and durable effects

Repository identity remains tenant + canonical host + decimal repository ID.
Bindings are target-branch scoped; exact PR base/head commits are pinned by the
server. Extend existing execute/enqueue with Card-or-repository targets; reject
both/neither before GitHub or engine work and preserve Card JSON/200/202 behavior.

Strict root `wyrd.toml` selects registered Verifiers or local definitions at the
pinned base, never the head. Reuse the full shared loader/defaults/reference-slot/
validation/order and declaration/upload/completion workflow. Imported Agents,
Prompts and artifact-backed Cards retain dependency order, immutable identities
and publisher attribution. Do not traverse arbitrary JSON as reference slots.
Refuse absolute/traversal/symlink/submodule/remote imports. Validate the complete
suite's actual readiness/compatibility before admitting any member; ordinary
missing evidence keeps the engine's existing inconclusive semantics.

Unchanged config/dependency content retains a binding revision; changed content
revises it under the same branch/family identity, including changes that preserve
a selected Verifier UID. Removed selections/branches retire their bindings.
Invalid/missing/empty config yields one action_required configuration Check,
never an empty success. Invalid suites dispatch no valid subset.

Signed bounded ingress is persisted before 2xx. Delivery-ID dedup and logical
review identity differ; retry of one deliberate rerequest resumes its same
selected generation. New rerequests create new generations. All writers for
one PR, including Check rerequests, obey the same ordering/claim authority.
Newer work supersedes unfinished older work; old deliveries or expired workers
cannot cancel newer work. Draft, ready, close, target edit, metadata edit and
default-branch push behavior follow REQ-028. Maintenance stays pending for Task005.

Reuse existing queue/claim/lease/cancellation and delivery owners. Never hold a
transaction over GitHub IO or introduce another scheduler/retained-state store.
Claims/pages/lookups are bounded/indexed and unrelated repos progress. A stale
claim cannot initiate admission or remote effects after authority loss. Preserve
ambiguous-effect reconciliation and Check terminal monotonicity remotely as
well as in SQL. One Check per binding/generation/head; Check names identify their
branch-scoped binding. Delivery reads selected retained Bifrost execution/result
state, waits on uncertainty and never re-executes verification. Delivery failure
is visible through execution inspection without changing the verdict.

Revocation, disable, disconnect and GitHub suspension/removal stop affected new
IO and cancel affected active manual/queued/automated repository work, while
retaining evidence and unrelated work. Execution and publication principals must
hold their own current grants; installation access grants none. Audit stays
non-blocking. Required terminal delivery retains the foundation ACK contract.

Non-goals: tenant App creation/rotation, custom login, App Manifest automation,
PR-head enforcement, source edits/comments/annotations, partial-suite success,
full code-review extraction, Workflow/Repository Cards, new scheduler/cache/
loader/client, or stronger universal exactly-once external effects.

## Approach

1. Extend and verify generic provider operations in the independent library.
2. Add operator App configuration and Rust-owned installation/OAuth/confirmation,
   using existing login, tenant SQL, permissions and network boundaries.
3. Expose the simple connection UI and matching headless start/status/control
   operations, then connect verified repository access.
4. Extend repository targets and trusted configuration through existing owners.
5. Connect durable event admission, ordinary Verifier execution and independent
   Check delivery; preserve claims, revocation and retained-state recovery.
6. Prove the user journey and negative/recovery seams, and document operator
   setup separately from tenant installation.

## Ordered Implementation Scenarios

### Scenario 1 — Install Wyrd and connect the selected repositories

**Behavior.** An operator configures the App once. A signed-in team admin starts
Connect GitHub, chooses an organization/repos on GitHub, completes user
authorization and confirms the connection. A second team installs directly from
GitHub while signed out, signs into Wyrd, chooses its tenant and completes the
same flow. Repeat for self-hosted configuration without changing code.

| Case | Required observation |
| --- | --- |
| Shared App, two organizations/tenants | Distinct verified installation associations and repository access; neither tenant supplies keys. |
| Existing App installation/reconnect | Authorize the existing installation without requiring uninstall/reinstall; failed reconnect preserves active settings. |
| Wrong/expired/replayed state, other browser/admin/tenant, forged installation | No unauthorized association or repository work; exact safe refusal and restart path. |
| Unconsented, missing permission, suspended or pending organization approval | Connection remains unactivated; no fabricated successful setup. |
| Rotation, disable, disconnect | Operator rotation preserves identity; tenant control cannot change keys; affected work stops and evidence remains. |
| No App config, invalid host/redirect | Explicit bounded GitHub refusal, no secret leakage and unrelated server paths remain available. |

**RED.** Add the planned [connection journey](#connection-journey) and
[browser journey](#browser-connection-journey), with the generic protocol proof
below. Assert actual redirects, server-side GitHub proof, selected repositories
and absence of tenant credential fields. The restored code lacks this flow.

**GREEN.** Implement the selected shared-App contract through existing server
config/login/SQL/RBAC owners and the generic provider client. Make both entry
points usable; a UI button without the complete server callback is insufficient.

**REFACTOR.** Keep only integration-specific state; reuse sessions, CSRF, typed
errors, network screening and ordinary principal APIs. No parallel login system.

### Scenario 2 — Adopt trusted configuration and run the selected Verifiers

**Behavior.** Two repositories reuse exact registered Verifiers without reading
the definition repository. One also imports local definitions and transitive
artifact/Agent/Prompt dependencies. Base-tree configuration selects the complete
suite; branch revisions remain honest and Card verification keeps its contract.

**RED.** Add the planned [configuration journey](#configuration-journey). Assert
full graph activation/publisher attribution, replay after interrupted upload,
conflict/unsafe import refusal, changed config with unchanged Verifier UID,
branch retirement and no partial dispatch. Exercise repository/Card execute and
enqueue, exact response shapes, and both/neither targets before provider calls.

**GREEN.** Extend WyrdConfig, the shared loader/canonical reference visitor and
existing declaration/admission workflows; keep publisher and executor separate.
Check their current exact source/declaration/run permissions before respective IO.

**REFACTOR.** Preserve canonical dependency order and reference semantics across
heavy-card declaration boundaries. Delete duplicated traversal/orchestration.

### Scenario 3 — A PR produces one correctly mapped Check

**Behavior.** Signed PR events enter the durable queue, resolve current pinned
identity/configuration, run ordinary Verifiers and yield exact-head Checks.
Supersession, drafts/ready, closure, target edits, metadata-only edits, repeated
ordinary events and explicit rerequests follow the specified event policy.

**RED.** Add the planned [event journey](#event-journey). Include process loss
between rerequest admission and settlement, partial multi-binding submission,
concurrent ordinary/rerequest delivery and lease takeover. Assert original
execution IDs on replay, no stale cancellation and distinct intentional reruns.
Default push must leave durable maintenance pending, not claim completion.

**GREEN.** Extend existing durable queue/claim/admission/cancellation owners,
with tenant-qualified control records and independent delivery identities.
Webhook acknowledgement follows durable save; failed save answers retryably.

**REFACTOR.** Reuse current claim bounds/fences rather than adding a scheduler,
global lock, provider IO transaction or worker-specific ordering plane.

### Scenario 4 — Retry delivery and revoke access without rerunning work

**Behavior.** Passed/failed/inconclusive/errored/cancelled/superseded/timed-out
executions map to success/failure/action_required/action_required/cancelled/
cancelled/timed_out Checks. Lost create/update responses and operator-key
rotation reconcile one Check. Bifrost uncertainty waits. Revocation cancels
unfinished work without removing history or affecting other repositories.

**RED.** Add the planned [delivery/recovery journey](#delivery-and-revocation-journey)
and [bounded SQL proof](#bounded-control-proof). Assert one remote association,
no late running regression, no new verification calls, exhausted delivery visible
on execution status, active manual and automated cancellation, and independent
progress during a stalled provider. Force stale-claim effect refusal directly.

**GREEN.** Project the retained selected state through existing delivery/effect
and cancellation boundaries. Preserve the foundation's pending terminal delivery
until ACK, independent of revoked execution authority or provider outages.

**REFACTOR.** Keep control state out of Bifrost result payloads and results out
of PostgreSQL; reuse existing ambiguity reconciliation and short transactions.

### Scenario 5 — Use the connection and its results through public surfaces

**Behavior.** SDK/CLI/MCP users start the same browser onboarding, inspect the
connection, control repository access and read bindings/Checks/executions. The
UI completes normal browser authorization using shared server contracts. Read,
run and write grants remain independent and exact-host/repository scoped.

**RED.** Add the focused [public projections](#public-projection-proofs) and
[HTTP contract proof](#http-contract-proof). Assert typed redacted fields/statuses,
all relevant write/read denials, configuration/Check reads after real signed
ingress, and no tenant App-secret or manual installation-claim surface. Browser
completion cannot be performed using an API key as human-session proof.

**GREEN.** Project the same wyrd-spec/shared-client contracts through Rust,
Python, TypeScript, CLI and MCP; register typed served OpenAPI. Keep OAuth codes,
GitHub calls and tenant policy entirely on the Rust server.

**REFACTOR.** Reuse existing native binding/error/schema/docs patterns. Remove
obsolete per-tenant App wording/tool descriptors rather than retain aliases.

## Acceptance Criteria

| Obligations | Observable proof |
| --- | --- |
| REQ-024/029; AC-015/016; INV-002/010 | Connection + browser + generic provider proofs establish both entry points, same SaaS/self-hosted code, no customer App keys, verified tenant binding, generic library-only additions and immutable pin. |
| REQ-025/026/027; AC-014/015; INV-003 | Connection/delivery/public proofs establish default-deny before confirmation, selected access, exact independent grants, distinct principals, active revocation and retained evidence. |
| REQ-034–038/040; AC-002/026–028/030–032; INV-012/013 | Configuration proof establishes full shared/local reuse, complete preflight, pinned-base trust, honest branch/config/dependency revisions, unsafe/conflicting imports, no fake Card and unchanged Card behavior. |
| REQ-028; AC-011/016; INV-006/007 | Event/bounded proofs establish persist-before-2xx, independent delivery/logical dedup, one intentional rerun, monotonic mixed-event/reclaim behavior and pending default-push maintenance. |
| REQ-030; AC-019 | Delivery proof establishes exact conclusions, per-binding/head identity, authorized report links/completeness, ambiguity/rotation/late-update safety, visible delivery failure and no rerun. |
| REQ-054–059; AC-048–051; INV-017/018 | Delivery/bounded proof consumes retained selected state without old payload rows, waits on unavailability, fences stale workers, keeps bounded/indexed queries and unrelated progress. |
| REQ-032 contribution; AC-001/031; public contracts | Public projection/HTTP/browser proofs establish real headless/browser ergonomics, schemas, exact field/status/error/grant parity. Full extraction/reviewer pipeline remains Task005/006 proof. |

## Implementation Reference

These are inspected owners in the restored source, not permission to copy the
rejected implementation. Revalidate callers before extending them.

| Existing owner and consumers | Gap and selected reuse |
| --- | --- |
| Server config `NetworkPolicy::screened_http`, auth `BrowserSessions::begin/resolve`, `components/auth/routes.rs`, existing UI browser sessions/BFF | Existing login/session/Origin and screened outbound boundaries exist; add only App configuration and integration-specific correlated setup. GitHub authorization is not a new Wyrd identity provider. |
| Library `GitHubApp::with_transport/installation/installation_token`, `GitHubClient`, `HttpTransport`, checks/webhook helpers; `pipeline_journey` | Existing generic protocol has no tenant-App assumption. Add OAuth/user-query methods to its transport/identity/retry owner; inject Wyrd's screened transport, preserve bounds and credential-safe redirects. |
| Runtime `Permission/PermissionScope`, `audit::authorize`; SQL `TenantConn/OperatorPool`, existing session/connection queries | Extend closed grants and tenant-scoped connection/control state. Human OIDC/Operator records have different lifetimes; they do not become GitHub installation records. Use required RLS/composite tenant keys and DB-owned coordination time. |
| `WyrdConfig::load`, `wyrd_loader::load/build_declaration_input`, canonical resolve/validate/order/reference visitor; client `cards::saga::declare/prepare`; server `declare_card` and normal completion/storage | Missing trusted pinned bytes and server-held declaration inputs; extend the complete workflow with containment and preserved topological/heavy-artifact sequencing, not a second YAML/JSON parser or declaration lifecycle. |
| `VerificationControl::execute/enqueue/get_execution`, `SubmissionSink`, `VerifierExecutionQueue`, cache/readiness and `BackgroundAuthority` | Add repository subject/admission/inspection arms to the existing owners, preserving current accepted authority, Card results and selected Bifrost state. No copied engine/queue/read model. |
| Shared `ClaimLoop/LeasedWork`, existing OperatorWorker bounded attempts and Verifier runner lease-renewal/cancellation; SQL existing claims/binding projection | Reuse bounded dispatch/retry/fencing. Repository branch/generation/Check control has independent lifetime; add only its domain identity/state, never another scheduler. |
| WyrdTestServer/PgFixture, SDK integration target, server integration/OpenAPI/browser target, CLI/MCP and UI journey harnesses | Existing real-server test owners supply public journeys. Put SQL/fence/fault math in Rust server/SQL tests, not raw HTTP/SQL/poll loops inside SDK examples; checked-in Card fixtures only. |

## Verification and Evidence

All replacement selectors below are **planned**, except the existing library
protocol selector. Packages/targets and Postgres wrapper were checked in the
restored repository. Confirm each actual selector after adding it and before
RED. A missing target or zero tests is failure, not proof. Each scenario owns
its RED/GREEN record; old task results are not carried forward.

### Generic library protocol proof

Run in bohmian-code-review; extend the existing public journey with OAuth/user
lookup cases and separation of App/user/installation credentials:

```bash
mise exec -- cargo test --locked -p bohmian-code-review-pipeline --test pipeline_journey --all-features github_app_protocol_reuses_existing_client -- --exact --nocapture
```

Then run its owner `mise run test:pipeline`, `mise run fmt:check`, `mise run lints`.
Record actual external base/candidate, generic diff, standalone build/test proof
and immutable delivery identity. No Wyrd-specific types/routes may enter it.

### Connection journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=github_app_connection::install_wyrd_and_connect_selected_repositories)"'
```

### Browser connection journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd crates/wyrd/wyrd-server/wyrd-ui && WYRD_UI_INTEGRATION=1 pnpm exec vitest run src/lib/ui-pages.integration.test.ts -t "^GitHub App connection supports both installation entry points$"'
```

### Configuration journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=repository_verification::trusted_target_configuration_reuses_shared_declaration)"'
```

### Event journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_github_repository::deliveries_resume_one_generation_and_keep_current_pr_work)"'
```

### Delivery and revocation journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_github_repository::check_delivery_and_revocation_preserve_execution_identity)"'
```

### Bounded control proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration --test-threads=1 -E "test(=pg_github_repository::claims_and_parentage_preserve_tenant_and_pr_authority)"'
```

### HTTP contract proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E "test(=pg_openapi_contract::github_app_connection_contract_matches_live_operations)"'
```

### Public projection proofs

Mirror the connection-to-Check story and actual denials through each introducing
surface. These are focused tests, not complete language suites:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_github_app_connection.py::test_install_wyrd_and_connect_selected_repositories'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/github-app-connection.test.ts -t "^install_wyrd_and_connect_selected_repositories$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::github_app_connection_uses_shared_contract)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -P journey --run-ignored=all --test-threads=1 -E "test(=card_lifecycle::github_app_connection_uses_shared_contract)"'
```

Use only applicable [owner checks](verification.md#owner-checks), plus the UI's
existing check/build/format lanes for connection UI. No aggregate gate/full
journey suite is required here; integrated change review runs combined journeys
once. Document operator registration/secret rotation separately from customer
installation/reconnect/disconnect. GHES controlled-host protocol proof is required;
a release-specific appliance compatibility claim needs an explicitly invoked
live appliance run, not a fake-host test.

## Material Stop Conditions

No material GitHub decision remains open. A discovery requiring per-tenant App
keys, weaker ownership proof, another policy engine, Wyrd-specific library code,
a new scheduler/loader/client, or changed approved tenant cardinality returns to
specification authority. Private implementation choices and fixtures stay with
the implementer. A new upstream SHA is delivery evidence, not an undecided design.

## Authority Links

[Approved GitHub amendment, revision24](../spec.md) ·
[Public contracts](../public-contracts.md) ·
[Deprecated task](TASK-004-github-repository-automation.md) ·
[Workflow](../../../spec-driven-development.md) ·
[Wyrd design](../../../../architecture/wyrd-design.md) ·
[Security](../../../../architecture/wyrd-security-posture.md) ·
[Bifrost](../../../../architecture/bifrost-design.md) ·
[Testing](../../../../TESTING.md).

## Obligations

Line numbers refer to this file as approved. Planned assertions name the proof
selector that must contain them; a selector that exists without the assertion
does not discharge the obligation.

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| O-01 | L41-45 | One configured App/host per deployment; operator config supplies App id/slug, API/web origins, private key, webhook secret, OAuth client id/secret through existing secret providers | connection journey boots from operator config only; config unit test parses/validates every field |
| O-02 | L46-50 | App registration requests Contents read, Pull requests read, Checks write and the listed events; nothing more | docs operator page lists exactly these; callback refuses an installation lacking checks:write/contents:read/pull_requests:read |
| O-03 | L51-54 | Setup/callback/webhook URLs are exact deployment URLs; GitHub.com and GHES web/API/OAuth endpoints from config, never substitution | connection journey asserts install URL `{web}/apps/{slug}/installations/new?state=` and OAuth URL `{web}/login/oauth/authorize` for both hosts |
| O-04 | L55-58 | Setup URL then separate OAuth callback; both entry points use the same Rust flow, existing Wyrd login, explicit tenant choice | connection + browser journeys drive Wyrd-first and GitHub-first to one confirmed connection |
| O-05 | L59-62 | Correlation, browser binding, 5-minute DB-clock expiry, current admin, verified installation/user access and repository selection; bare installation ID or valid state insufficient | connection journey refusal cases (O-17..O-20) |
| O-06 | L63-65 | Confirmation uses the existing allowlist to connect and enable selected repos; no second snapshot/grant model or refresh-token service | SQL schema review + journey: confirm yields allowed+PR-enabled access rows; no user token persisted (column absence asserted) |
| O-07 | L66-67 | Connection setup adds no repository quota/limit API | diff audit: no limits route/field |
| O-08 | L68-70 | One active installation per tenant, one tenant per installation; disable keeps claim, disconnect releases after fencing work; neither removes deployment credentials | connection journey: second tenant claim refused 409; disable then other tenant still refused; disconnect then other tenant can claim after proof |
| O-09 | L71-74 | Unconfigured App refuses connection explicitly (503) while unrelated paths work; invalid config uses typed startup validation; provider destinations use network screening | connection journey on unconfigured server: 503 WYRD_GITHUB_503_UNAVAILABLE and /v1 health/cards still 200; config test rejects invalid host |
| O-10 | L77-99 | Library: keep 9fdf6dc; extend only with generic OAuth exchange, user-accessible installations/repos, delete decoding; no Wyrd code; immutable descendant pinned for server only | library journey selector; Cargo.toml rev pin; `cargo tree` shows only wyrd-server depends on it |
| O-11 | L101-104 | Repository identity = tenant + canonical host + decimal repository ID; bindings target-branch scoped; server pins base/head | config journey asserts binding repository arm {host,repository_id,target_ref} and result subject base/head pinned from GitHub |
| O-12 | L103-104 | Execute/enqueue take card or repository target; both/neither rejected before GitHub or engine work; Card JSON/200/202 preserved | config journey: both/neither → 400 with zero provider calls; Card execute 200 + enqueue 202 unchanged shape |
| O-13 | L106-112 | Strict root wyrd.toml at pinned base selects registered Verifiers or local definitions; reuse shared loader/declaration; imported graph keeps order, identities, publisher attribution | config journey: local Verifier + Prompt/Agent/artifact deps declared with publisher principal as creator |
| O-14 | L110-111 | Refuse absolute/traversal/symlink/submodule/remote imports | config journey: each unsafe path → action_required configuration Check, no declaration |
| O-15 | L111-113 | Validate complete suite readiness before admitting any member | config journey: one unknown ref in a 2-entry suite → zero executions |
| O-16 | L115-120 | Unchanged content keeps binding revision; changed content (incl. same Verifier UID) revises; removed selections/branches retire; invalid/missing/empty → one action_required configuration Check | config journey assertions per case |
| O-17 | S1 table r1 | Shared App, two orgs/tenants: distinct verified associations and repository access; neither supplies keys | connection journey |
| O-18 | S1 table r2 | Reconnect existing installation without reinstall; failed reconnect preserves settings | connection journey |
| O-19 | S1 table r3 | Wrong/expired/replayed state, other browser/admin/tenant, forged installation → no association, safe refusal + restart | connection journey, each case |
| O-20 | S1 table r4 | Unconsented, missing permission, suspended, pending org approval → stays unactivated | connection journey, each case |
| O-21 | S1 table r5 | Operator rotation preserves identity; tenant control cannot change keys; disable/disconnect stop work, keep evidence | delivery/revocation journey (rotation) + connection journey (no key fields) |
| O-22 | S1 table r6 | No App config / invalid host or redirect → bounded refusal, no secret leakage, unrelated paths available | connection journey + config unit test |
| O-23 | S1 RED | Assert actual redirects, server-side GitHub proof, selected repositories, absence of tenant credential fields | connection journey |
| O-24 | L122-128 | Signed bounded ingress persisted before 2xx; failed save retryable; delivery-ID dedup distinct from logical identity | event journey |
| O-25 | L124-127 | Rerequest retry resumes same generation; new rerequest creates new generation; all PR writers share ordering/claim authority | event journey |
| O-26 | L127-130 | Newer work supersedes unfinished older; stale deliveries/expired workers cannot cancel newer work | event journey + bounded SQL proof |
| O-27 | L129-130 | Draft waits, ready runs, close stops, target edit retargets, metadata edit ignored, default push leaves maintenance pending | event journey per action |
| O-28 | L132-135 | Reuse queue/claim/lease/cancel; no transaction over GitHub IO; bounded/indexed claims; unrelated repos progress | bounded SQL proof + delivery journey stalled-provider case |
| O-29 | L135-137 | Stale claim cannot admit or cause remote effects after authority loss | bounded SQL proof forced stale-claim refusal |
| O-30 | L137-140 | Ambiguous-effect reconciliation; Check terminal monotonic remotely and in SQL; one Check per binding/generation/head; names identify branch-scoped binding | delivery journey |
| O-31 | L140-142 | Delivery reads retained Bifrost state, waits on uncertainty, never re-executes; failure visible on execution inspection without verdict change | delivery journey |
| O-32 | L144-147 | Revocation/disable/disconnect/suspension/removal stop new IO and cancel affected manual/queued/automated work, keep evidence and unrelated work | delivery journey |
| O-33 | L146-148 | Execution and publication principals need their own grants; installation grants none; audit non-blocking; terminal delivery keeps foundation ACK | delivery journey denial cases |
| O-34 | S4 | Conclusion mapping passed/failed/inconclusive/errored/cancelled/superseded/timed_out → success/failure/action_required/action_required/cancelled/cancelled/timed_out | delivery journey table + unit test |
| O-35 | S4 | Lost create/update and operator key rotation reconcile one Check | delivery journey |
| O-36 | S5 | SDK/CLI/MCP start browser onboarding, inspect connection, control repository access, read bindings/Checks/executions | public projection proofs (Python, TS, MCP, CLI) + Rust client |
| O-37 | S5 | Read/run/write grants independent and exact-host/repository scoped; all relevant denials | public projection + HTTP contract proof |
| O-38 | S5 | Typed redacted fields/statuses exactly as public contracts; no tenant App-secret or manual installation-claim surface | HTTP contract proof field-set equality |
| O-39 | S5 | Browser completion cannot use an API key as human-session proof | connection journey: selection/confirm with API key → refused |
| O-40 | S5 | Typed served OpenAPI for every new operation | HTTP contract proof |
| O-41 | S5 | UI completes normal browser authorization via server contracts | browser journey |
| O-42 | Approach 6 | Operator setup documented separately from tenant installation | docs pages |
| O-43 | Verification | GHES controlled-host protocol proof | library journey (GHES) + connection journey GHES host |
| O-44 | Non-goals | None of the listed non-goals implemented | diff audit |

## Evidence

In progress; rows not listed are open. Journey selector:
`github_app_connection::install_wyrd_and_connect_selected_repositories`
(connection journey command above), PASS at a646c4c05.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| O-01 | `crates/wyrd/wyrd-server/src/config.rs` `GitHubAppConfig`; `components/github/mod.rs` `GitHub::from_config` | `config.rs` `github_app_config_parses_and_refuses_invalid_settings`; journey boots via `github_app(app_config(..))` | config unit test; connection journey | PASS |
| O-03 | `GitHub::installation_url`, `authorization_url` | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:274`, `:297`, `:710` | connection journey (GitHub.com and GHES) | PASS |
| O-08 | migration `github_connections_one_claim`; `GitHubControl::confirm`, `deactivate` | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:507`, `:529`, `:544` and the disconnect-then-claim 200 | connection journey | PASS |
| O-09 | `GitHubControl::github` (503); `ScreenedTransport` | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:208`, `:222`; config test rejects `http` API base | connection journey; config unit test | PASS |
| O-17 | per-tenant RLS rows; claim index | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:507`, `:544` | connection journey | PASS |
| O-18 | `connect` stage `authorize` for a claimed installation | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:588` (refused reconnect keeps active), `:629` (proven reconnect keeps 102, adds 101) | connection journey | PASS |
| O-19 | `setup`, `callback`, `require_bound`, `setup_by_state` expiry | replay `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:307` and spent callback; no-session setup; forged hint; other admin `:472`; other tenant `:477`; expired `:483` | connection journey | PASS |
| O-20 | `prove_installation` permission and suspension checks | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:673` (missing checks:write; suspended) | connection journey | PASS (pending org approval: GitHub returns no installation to the user, the same `user_installations` refusal; no separate assertion yet, open) |
| O-23 | `callback` code exchange and proofs | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:335` (PKCE S256 proven), provider proof URLs, `:441` field-set equality | connection journey | PASS |
| O-39 | `HumanBrowserSession` extension (never set for API-key sessions); `bound_selection` | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:388`, `:398` | connection journey | PASS |
| O-43 (connection half) | host from config | `sdks/wyrd-sdk-rust/tests/integration/github_app_connection.rs:710`, `:756` | connection journey GHES host | PASS |
| O-11 (subject half) | `VerificationSubject::Repository`; `GitHubControl::pin_pull_request` | `sdks/wyrd-sdk-rust/tests/integration/repository_verification.rs:144` (target_ref, base/head pinned), `:158` (token restricted to repository 101, contents/pull_requests read), `:174` (not-allowed repository 404), `:184` (no repositories:run → 403) | configuration journey | PASS (binding repository arm open) |
| O-12 | `VerificationRequest::target`; `VerificationService::run_direct`, enqueue | `sdks/wyrd-sdk-rust/tests/integration/repository_verification.rs:97` (Card subject raw CardRef), `:107` (enqueue 202), `:122` (both/neither 400 on execute and enqueue), `:128` (zero GitHub calls) | configuration journey | PASS |

### RED/GREEN and process exceptions

- Process exception: the server flow (`components/github/service.rs`, `routes.rs`), SQL module and migration were written before the connection journey existed. The journey's first run failed only on its own ordered field-name assertion (serde_json preserves insertion order). Fixed by comparing sorted names; then PASS.
- Process exception: the spec, permission and config unit tests were written alongside their production code, with no RED run first.
- Unit test `components::github::service::tests::cursors_bind_their_listing_and_states_name_their_tenant`: first run failed because the test mistyped the RFC 7636 Appendix B challenge literal. The computed value equals the RFC's `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`; the literal was corrected.
- Clippy `large_futures` in `verifier_orchestration.rs` was caused by the new builder field. Fix site: `wyrd-testing` `WyrdTestServerBuilder.github_app` is now boxed.
- Process exception: the repository-target spec, server pinning, and Rust client method were written before the configuration journey; its first run passed.
