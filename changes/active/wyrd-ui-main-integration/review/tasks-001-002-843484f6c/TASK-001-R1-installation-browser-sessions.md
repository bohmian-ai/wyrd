---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 10
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-020, REQ-033, AC-001, AC-002, AC-003, AC-004, AC-005, AC-010, AC-012]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2]
---

# TASK-001-R1 — Valid installation and Rust-owned browser sessions

## Authority and immutable inputs

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
- Approved [spec revision 10](../../spec.md), explicitly selected by the caller
  after rejecting additional UI deployment secrets/environment variables.
- Original [TASK-001](../../tasks/TASK-001-distribution-login.md) and retained
  [TASK-002](../../tasks/TASK-002-remove-old-ui.md) workflows.
- Original cumulative review: base `8651b96fd0c049ab5e63b0606c98326af1fc8848`,
  candidate `843484f6cceba1f48139349f66794966221c5d37`.
- [Verdict](verdict.md), [validated ledger](findings-validation.md), and
  [authority follow-up](followup-review.md). Preserve their original subject
  and SPEC_REVISION_REQUIRED verdict. They are not rewritten as PASS.

Revision 10 resolves the reviewed authority conflict by replacing confidential
Node OAuth/encrypted credential cookies with ordinary Rust-owned opaque
browser sessions. This is a **separate human-directed change**, not a reviewer
finding and not a retroactive FIND ID. Original task instructions freeze auth
behavior only outside that approved change. This task supersedes the original
browser-session premise, not the distribution, tenancy or visual obligations.
No implementation is claimed by this assignment. Route execution to
`$wyrd-implement`; independent cumulative task review is still required.

## Outcome and Value

A usable signed installation cannot be replaced by an incomplete fresh or
cached UI. The retained UI signs in through Rust-owned sessions stored in the
existing Postgres, works across replicas and process restarts, and requires no
new deployment secrets, environment variables or session service. Real
provider login is exercised through both native UI and official nginx ingress.

## Issue diagnosis

| Issue | Current behavior, evidence and consequence | Required correction |
|---|---|---|
| FIND-TASK-001-1 — REQ-002/AC-001 | `server/install.rs::extract` checks marker names; its signed positive fixture contains empty entry/manifest/dependencies. `ServerInstaller::install` bypasses even those checks for cached binary-only versions; `installed` exposes the same incomplete version. `activate` can switch a healthy current to an unusable release and the suggested reinstall repeats successful cache activation. Source: `install.rs:190–249,436–455`, `tests/server_install.rs:50–58`; full tracing in validation. | One installer-owned concrete production-payload eligibility rule across extraction, cache reuse before activation, and installed lookup/refusal. Reject incomplete cached content usefully without switching current or silently deleting active data. Preserve healthy reuse and signed atomic staging. |
| FIND-TASK-001-2 — REQ-006/AC-004 | `startup-common.sh::ui_sign_in` proves API-key recovery and a device page, not federated login. The production identity driver uses native fixed-ORIGIN Node listeners, whereas nginx sends `/auth/*` to Rust and derives public origin through forwarding. Even running today's startup checks cannot prove that provider path. | Add one focused real-provider journey through the actual official nginx public origin using the existing identity fixtures and image/startup owners; prove completion/session use/logout and relevant refusal at that boundary. This is missing proof, not a claim that current nginx SSO is known to fail. |
| Human-directed REQ-033/AC-012 | `BrowserSessions` redeems OAuth codes, caches access tokens and serializes refresh/API-key credentials in cookies. Rust identifies UI as public while refresh handling assumes confidential; no compliant cookie/client-secret machinery exists on main or historical UI. | Rust owns ordinary browser sessions and provider completion; browser holds only an opaque identifier. Node forwards that selected session and safe metadata. No new UI client secret, encryption key or provisioning is added. |

## Owners, Scope, Consumers, and Reuse Evidence

| Capability | Existing owner/symbol | Inspected consumers/proof | Precise gap and selected extension | Why new machinery is or is not needed |
|---|---|---|---|---|
| Installation eligibility | CLI `ServerInstaller::{install,installed}`, `extract`, `activate`; `DevServer::check_ui`; `scripts/build.sh::bundle` | Install dispatch, DevArgs auto-install; CLI `server_install` fixture and preservation/selection tests | Validate actual adapter-node payload consistently before exposure/activation; truthful invalid-cache refusal | Extend the existing installer, not an install-time Node runtime, generic import resolver, rollback controller or new bundle format |
| Provider/browser completion | `HumanConnections::begin_bound` in `wyrd-auth/src/login.rs`; `AuthorizationCodeExchange` in `callback.rs`; `LoginInitiation` and SQL login state | Provider callback, CLI/device, connection-test and UI identity journeys | Add browser initiation/correlation and direct session completion to the existing bound provider workflow | No duplicate issuer discovery, callback/JWT verifier, login-state store or provider transport |
| Durable browser sessions | Existing `wyrd-auth` issuance/grant/connection/credential owners and `wyrd-sql` auth query/migration owners; `RefreshTokens` and `PlatformSessions::confirm` as lifecycle references | Human renewal, key exchange, principal/credential revocation tests | A browser-specific digest record binds tenant/principal/lifetime and human revision or recovery credential; reuse current authority checks | OAuth refresh rows are human/client-bound and cannot represent recovery. A focused browser record is earned; a new token vault/session service is not |
| Browser request authentication | Server `require_authenticated`, Bifrost HTTP authentication wrapper, `AuthenticatedPrincipal`, `TokenGrants` and `RequestOrigin`; `/v1` handlers | Retained settings mutations; bearer-only CLI/SDK/platform/gateway/OTLP/MCP/gRPC consumers and OpenAPI tests | Explicit tenant HTTP cookie alternative with current authority/Origin enforcement and strict bearer precedence | Reuse existing permission/audit and API response owners. No universal cookie fallback, new `/v1` API facade, bearer minting endpoint for Node or duplicate role logic |
| UI consumer closure | `BrowserSessions`, `BrowserSession.api`, hooks, chooser/switch/logout actions, tenant layouts, login/recovery/callback/settings routes, `forwardAuth` | UI session/routing/upstream tests and real production identity driver | Replace OAuth/credential-cookie logic with session forwarding and safe metadata, preserve visible workflows | Existing Node request boundary remains; no new shared component, framework, SDK/domain client or durable Node cache |
| Public runtime proof | `identity_ui_e2e::production_ui_bff_journey`; `production-auth.integration.test.ts`; existing identity mise lifecycle; official nginx/entrypoint/startup owners | Two real Rust + two real Node replicas; startup dev/prod | Extend native opaque-session assertions and add official nginx provider path; share existing provider setup/cleanup for focused execution | No mock IdP, direct-route bypass, alternate orchestration or test-selector env var. Existing setup is currently inseparable from full suite and must be factored once |

### Fixed implementation outcomes

REQ-033 is the complete behavioral contract. In particular:

- Public browser operations are `POST /auth/browser/login`,
  `POST /auth/browser/api-key`, `GET /auth/browser/session` and
  `POST /auth/browser/logout`; shared provider completion remains
  `/auth/callback`. Use typed `wyrd-spec` request/response contracts and Rust
  handler registration. Safe session metadata retains tenant/principal,
  permission and absolute-expiry information needed by current UI consumers;
  it never returns session identifiers or keys/tokens. Optional tenant filtering
  selects only sessions already proved by cookies. Machine OAuth form/error
  conventions remain unchanged; browser APIs use existing Wyrd error mapping.
- Persist only random identifier digests and authority/lifecycle references,
  with tenant RLS and database time. Keep five-minute login transactions and
  twelve-hour absolute sessions, no sliding expiry. Use a focused browser
  record instead of overloading OAuth refresh semantics; exact table/private
  type layout remains implementation-owned. Existing auth maintenance owns
  expiry cleanup. No raw recovery key or refresh/access token is retained.
- Human sessions recheck exact Active connection revision, tenant/principal
  and grants. Recovery sessions recheck the referenced credential, preserve
  its permission restrictions and refuse its revocation/expiry. Logout revokes
  only the selected browser session. Two concurrent valid sessions remain
  independently revocable. Store failures are unavailable, not invalid-session
  success or fallback. Use existing issuance/credential owners' lock and
  authority ordering where applicable; Node never resolves permissions.
- Cookie authentication applies only to protected tenant `/v1` HTTP requests.
  Preserve existing handler shapes/audits and all bearer paths, including
  audience distinctions in the shared HTTP authentication wrappers. An explicit
  access-token header wins even if invalid; no fallback to a good cookie.
  Non-browser protocols and platform routes do not gain cookie authentication.
  Require exactly one selected cookie for tenant API authentication; refuse
  ambiguous multiple sessions instead of selecting the first. Only session-
  metadata enumeration accepts multiple tenant cookies. A session cannot cross
  the tenant bound by its record or widen authority.
- Reject missing/mismatched Origin on cookie-authenticated writes, login,
  recovery and logout. SvelteKit must check the incoming browser action before
  forwarding; use only trusted public-origin context and never turn missing or
  hostile Origin into an accepted server-generated one. Bound provider callback
  correlation replaces Origin only for that callback. GET metadata/settings
  reads cannot write. Keep existing form-origin protection and cookie flags.
- Preserve multiple signed-in tenants, chooser, switch, safe identity display,
  API-key recovery and settings. Rust validates every presented session rather
  than trusting cookie labels. Enumeration clears invalid/expired cookies and
  returns live per-tenant contexts; no valid sessions means authentication
  refusal. An invalid selected tenant never falls back to another session, and
  store unavailability is not silently skipped. Node forwards only the selected session to the
  configured upstream, preserves Set-Cookie/redirect/error behavior, and never
  adds cookies or credentials to caller-selected URLs.
- Session IDs are per-login random credentials, not operator-provided secrets.
  Do not add UI configuration/env vars, cookie keys, client credentials, shared
  Node state, bootstrap files, sticky sessions, Redis, new dependencies or Cargo
  features. Keep existing deployment signing/sealing/IdP/database configuration.
- Retire UI-only OAuth/code/refresh consumers and obsolete Node callback/
  credential-cookie logic after migrating their callers. Clear old cookies
  and require re-login; do not support a plaintext fallback. Preserve historical
  auth records and live CLI/SDK/device/platform/workload/provider-test contracts.
  Remove only actual orphaned browser machinery; no unrelated OAuth rewrite.
- Native/current containers keep their existing lifecycle/readiness/cleanup.
  Postgres is the shared authority; Rust/Node replacement must preserve valid
  sessions, and database interruption must not crash unrelated server services.
  Development-only local identity gates remain development-only; no new mock
  success is introduced.

## Approach

1. Close installer eligibility at its existing shared owner and prove old-current
   preservation; keep Node compatibility/runtime validation in the existing
   launch/build proof, not installation.
2. Add browser lifecycle persistence and provider completion inside the existing
   Rust auth/SQL boundaries, then typed browser routes and guarded tenant HTTP
   authentication. Reuse current permission/credential/audit resolution.
3. Migrate all retained Node/UI consumers together and remove retired browser
   credential logic. Close wire/schema/OpenAPI/generated and documentation
   consumers in the same task.
4. Extend real native identity proof, then exercise one official nginx deployment
   through the same managed providers. Factor existing fixture lifecycle once
   for exact focused commands; do not clone setup or require the full suite.
5. Run focused proof/checks, record findings separately from new REQ-033
   acceptance, and leave missing platform/human evidence explicitly unverified.

## Ordered Implementation Scenarios

Execute one scenario RED → GREEN → REFACTOR before the next. Record real selected
test counts, expected failure and final evidence. Test names marked **planned**
are additions to confirmed existing targets, not claims they already exist.

### Scenario 1 — Reject incomplete fresh and cached UI without replacing current

**Behavior.** Reject signed empty-marker payloads, missing generated support or
production dependencies, and cached binary-only latest versions; leave good
current intact. Complete cache reuse still succeeds. Already-active incomplete
cache lookup/refusal gives truthful recovery, not successful no-op reinstall.

**RED.** Extend the existing preservation and healthy-selection assertions; prove
failure is the known payload/cache admission, not setup or compilation. Run:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_preserves_previous_on_failure)'
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_selects_latest_stable)'
```

**GREEN.** Apply concrete adapter-node production completeness consistently at
fresh/cached/lookup admission. Refuse invalid cache before activation and name
safe recovery; do not delete active bytes. Use a realistic positive fixture.

**REFACTOR.** Consolidate the eligibility definition in the existing owner,
retaining signatures/digests, compatibility, atomic staging, cancellation,
healthy reuse and useful error behavior. No generic JavaScript verifier.

### Scenario 2 — Persist revocable opaque tenant sessions

**Behavior.** Recovery creates an opaque session with no retained raw key;
expiry, wrong tenant, unknown/tampered ID, revoked/expired credential or disabled
principal/tenant refuse. Logout is immediate, session-specific and leaves the
API key usable. Store unavailability is distinct and never successful fallback.

**RED.** Add **planned**
`pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable`
to the existing server `integration` target, using real server/SQL/key owners.
It must fail because the browser lifecycle is absent, not missing fixtures:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable)"'
```

**GREEN.** Implement only the browser-session persistence/lifecycle and typed
recovery/session/logout HTTP needed for these outcomes; reuse current authority
and audit owners. Digest-only data and server-enforced expiry are asserted.

**REFACTOR.** Reuse the narrow existing auth/SQL helpers, preserving transaction
ownership/RLS and keeping browser and OAuth refresh lifecycle distinct. Add
supporting IO-free or SQL cases only when they exercise a cleaner failure seam.

### Scenario 3 — Protect browser API calls without changing machine authentication

**Behavior.** A valid selected session reaches the same tenant settings/API
permission checks. Missing/hostile Origin refuses writes; explicit bad bearer
cannot fall back to a cookie; no-session refusal, wrong-tenant authority, and
least-privilege recovery all remain enforced. Other protocol surfaces stay
bearer-only. Read-only requests do not mutate or slide expiry.

**RED.** Add **planned**
`pg_browser_sessions::browser_session_requests_enforce_origin_and_bearer_precedence`
in the existing integration target, driving real HTTP and auth decisions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::browser_session_requests_enforce_origin_and_bearer_precedence)"'
```

**GREEN.** Integrate session resolution at the existing tenant HTTP principal
boundary with explicit Origin/CSRF checks and normal authorization/audit. Do not
introduce another API response facade or change global token verification.

**REFACTOR.** Remove duplicate principal/permission logic if passing paths expose
it; preserve machine token audience, expiry, local verification and stable errors.

### Scenario 4 — Real SSO and retained UI across replicas and restart

**Behavior.** Production SSO callback creates an opaque session directly in
Rust. Chooser, tenant switching, identity, recovery, settings and logout work.
Session use continues on a different Rust and Node replica and after replacement;
issuer/state/callback/replay/connection-revision failures are refused. Expired,
revoked and old credential cookies prompt sign-in without a plaintext fallback.
No token/key/verifier is exposed in cookies or page data. No new deploy config.

**RED.** Extend existing `identity_ui_e2e::production_ui_bff_journey` and its
production TypeScript browser driver for this contract. Current JSON-cookie/
Node-OAuth behavior must fail the new assertions. The driver requires the Rust
fixture and must not be invoked independently with fabricated journey data.
Use the existing managed providers after the shared setup correction below:

```bash
mise run test:identity:ui
```

`test:identity:ui` is a **planned focused mise leaf**, not a current task. It
must invoke this exact existing test, inside the shared managed fixture lifecycle:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'
```

**GREEN.** Extend bound provider initiation/completion and migrate the retained
UI request consumers. Keep existing errors/states and safe metadata; preserve
real CLI/device/provider-test siblings and all old valid refusal assertions.

**REFACTOR.** Delete obsolete Node exchange/cache/credential-cookie consumers
and UI-only OAuth issuance; keep only rendering/forwarding responsibilities.
Do not recreate the deleted UI components or add a domain SDK to prove login.

### Scenario 5 — Document and verify the actual cookie/bearer wire boundary

**Behavior.** Served OpenAPI matches browser operations, metadata and cookie
security alternatives; other protocols and machine OAuth forms retain their
contracts. Missing or refused credentials match the documented stable errors.

**RED.** Extend the existing served-document checks for the intentionally changed
browser contract. Keep the bearer and negative assertions strict rather than
removing checks when adding the cookie alternative:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_openapi_contract::tenant_login_operations_publish_their_contract) | test(=pg_openapi_contract::every_authenticated_path_declares_the_one_wyrd_scheme) | test(=pg_openapi_contract::unauthenticated_requests_match_each_operation_declared_security)"'
```

**GREEN.** Register typed browser handlers/contracts, document exact applicable
cookie alternatives and CSRF errors, and regenerate affected outputs from source.
Do not edit generated files or claim codegen proves served route registration.

**REFACTOR.** Reuse schema/security declarations; no new count guard or catalog.
If a test name is legitimately renamed for its changed contract, record its
exact replacement command and prove it selects the same/stronger cases.

### Scenario 6 — Real provider login through official nginx

**Behavior.** One reviewed official Linux image completes provider login,
callback, session use, settings/refusal, logout and session continuity at nginx's
actual public origin. Cookies/Origin/redirects follow deployed routing. An
API-key-only probe, direct Node request or synthetic `/auth` gateway is insufficient.

**RED.** Add **planned**
`identity_ui_e2e::production_ui_nginx_journey` in the existing identity integration
target, sharing provider/browser fixtures and the official build/startup owners.
Add a **planned focused mise leaf** to supply the same lifecycle:

```bash
mise run test:identity:ui:container
```

Its inner exact selector is:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_nginx_journey)'
```

**GREEN.** Reuse existing official image build, nginx/entrypoint and identity
fixtures for that real route. Record immutable image ID/source and public origin.
Keep the existing startup API-key/device, streaming and child-cleanup assertions;
correct routing only if that focused proof diagnoses a real defect.

**REFACTOR.** Share setup and driver code at their existing owners. No mock IdP,
second deployment framework, new test-selector env, duplicated full identity
suite, production secret in fixtures or alternate routing path.

## Verification and Evidence

### Setup and focused lanes

- `test:identity:journey:inner` currently embeds provider recreate/seed/cleanup
  before the full suite. Factor that lifecycle once in its existing mise/setup
  ownership so the original full lane and the two planned focused leaves share
  setup and guaranteed cleanup. Reuse existing Keycloak/Dex/Postgres values and
  migration/build owners. No copied provisioning, new selector env or standalone
  replacement harness. The new leaves own Postgres/migrations, providers,
  compiled server/CLI prerequisites and production UI build as applicable.
- Confirm new/renamed tests with `mise exec -- cargo nextest list` using their
  package and target; record selected/running counts. Planned names above are
  deliberate task additions, not current pass evidence. Tests requiring a
  real runtime remain in that runtime, not embedded Node/Python in Rust.
- Run UI checks, affected fast auth/routing/upstream tests and production build:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts src/lib/server/auth/session.test.ts src/lib/server/upstream.test.ts src/lib/server/routing/tenant.test.ts src/lib/ui-library.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
```

The existing focused CLI-refresh regression must still pass:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E "test(=refresh::pg_tests::happy_rotation_mints_new_pair_and_revokes_old_row) | test(=refresh::pg_tests::rotation_copies_the_connection_binding)"'
```

Run `mise run fmt`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation`, `mise run codegen:check`, and the served-document
lane `mise run test:principals:integration` required for changed OpenAPI. Use
`git diff --check` and a full tracked/untracked write-set audit. If implementation
touches docs-site sources, add `mise run docs:check`; architecture/task Markdown
alone does not require building an unrelated frontend. Run Cargo-backed checks
sequentially. No `mise run gate`, full identity suite, full multi-language
journey aggregate or every-image matrix is required for task iteration.

### Runtime and manual acceptance that remain real gates

Run affected startup dev/prod owners when their files change:

```bash
mise run test:server:startup:dev
mise run test:server:startup:prod
```

Current startup-common builds Debian only. Reuse its build/probe owner when
selecting other official variants; do not claim those commands cover Rocky or
both architectures. All selected native source-free signed-bundle/runtime/digest
and Debian amd64/arm64 + Rocky amd64 deployment evidence remains required at
integrated acceptance, not as repeated full matrices inside this remediation.
Prove accepted extracted production UI starts source-free using the existing
native package/build runtime owner; an empty marker fixture or checkout symlink
cannot establish that. Missing Linux capability on this Mac is UNVERIFIED,
never a pass or reason to invent a platform-specific prerequisite.

Human checkpoint remains chooser, SSO/API-key recovery, signed-in tenant page,
settings and logout/error/expired/refused states at native and nginx public
origins. Record desktop/mobile, light/dark, keyboard/focus, touch and narrow/long
content inspection. Use normal production/native/container launch and repository
identity data. No new page/components/gallery work is authorized. TASK-002 has
no code finding but its human gate is still pending. Dependent acceptance must
not advance from automated/fixture results alone.

### Diagnosis and static proof

Document-only authority reconciliation needs static link/consistency proof,
not manufactured RED. New migrations/handlers/session branches need meaningful
behavioral proof above. Use tracing before diagnosing any unexpected failure;
record symptom, trace, cause and shared fix site. Follow the repository's fresh
diagnostician requirement before changing assertions/timeouts/retries to clear
unexpected failures. Intentionally superseded credential-cookie/OAuth assertions
are replaced with REQ-033 coverage, not silently weakened.

## Acceptance Criteria and Closure Mapping

| Criterion | Finding/requirement | Required evidence |
|---|---|---|
| Incomplete fresh/cached UI cannot activate or masquerade as installed; truthful recovery and healthy reuse | FIND-TASK-001-1; REQ-002 | Exact installer negatives and good-current preservation plus realistic positive/source-free runtime proof |
| Rust owns bound login, digest-only tenant session, absolute expiry and current revocable authority for SSO/recovery | Human-directed REQ-033; AC-012 | Real HTTP/SQL tests and production provider/browser journey; no raw credential retention |
| Node only forwards selected session/trusted context; retained chooser/switch/settings/logout work without credential cookies or extra config | REQ-006/033; INV-002/004/005 | UI checks, native two-replica/restart journey, no new env/secret/service/dependency audit |
| Invalid ID/tenant/Origin/bearer/key/connection and store outage are correctly refused; no widening, fallback or unrelated API-key/session revocation | REQ-033; AC-003/012 | Focused session/auth negatives, unchanged bearer/CLI and platform behavior, ordinary permission/audit evidence |
| Official nginx provider login/session/logout/refusal path actually runs, with immutable identity | FIND-TASK-001-2; REQ-005/006 | Focused container journey, actual public origin and image/source ID; no direct-route substitute |
| Served contracts/generated consumers and retired UI-only behavior agree with new boundary | REQ-033; AC-012 | Source-driven codegen and exact served-document tests; no plaintext compatibility route |
| Native/image/visual and first-class regression obligations remain honest | REQ-001–006/020; AC-001–005/010 | Explicit per-runner evidence or UNVERIFIED; human inspection separately recorded |

## Expected Write Set and Consumer Closure

Owning Rust contracts/errors/schema in `wyrd-spec`; auth lifecycle/issuance/
provider completion in `wyrd-auth`; auth rows/queries/migrations in `wyrd-sql`;
server browser routes, origin/principal middleware and served schema; retained
UI auth/session/hook/load/action/routing consumers and their real identity
fixtures; installer and fixtures; existing image/startup/identity mise proof
owners; source-driven outputs/docs directly affected by those contracts.
These are discovery boundaries, not a private file/helper allowlist. Foundation
TASK-003 later owns any shared SDK session-transport projection; do not add a
Cards/Query/Bifrost/domain client here to test auth. Preserve all unrelated
trace edits and original task/review evidence. No commit/push/deploy is implied.

## Material Stop Conditions and Preserved Non-goals

REQ-033 and revision-10 authorities already authorize the necessary browser
persistence, typed endpoints, session principal integration, safe UI consumer
closure and old-cookie re-login. Do not stop merely because those are new;
choose reversible private schema/helpers inside those boundaries. Stop for a
required new deployment secret/env/service, stored recoverable credential,
platform-browser expansion, changed bearer/device/SDK/workload semantics,
unapproved security/tenant/audit or destructive-data decision. Finish unaffected
work and report the exact conflicting authority. Do not choose another session
architecture or silently preserve the retired UI OAuth flow.

No new product feature, domain data/page/component, auth framework, generic
session/transport abstraction, live production credential, relaxed origin check,
platform-specific process/resource mechanism, weakened gate or schema allowlist
is in scope. Preserve existing signature/compatibility/install lifecycle,
server initialization/saved credentials, platform separation, provider tests,
non-blocking audit and all runtime targets. Implementation review remains the
completion gate; revision approval alone does not accept either original task.

## Authority Links

- [Spec revision 10](../../spec.md), [plan](../../plan.md),
  [verification context](../../references/verification.md).
- [Original verdict](verdict.md), [validated findings](findings-validation.md),
  [focused authority follow-up](followup-review.md).
- Repository `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
  `architecture/wyrd-doctrine.mdx`,
  `architecture/operations/deployment-and-release.md`.
- `architecture/references/languages/{spec-driven-development,implementation-execution,maintainer-style,testing-workflows}.md`;
  repo `wyrd-ui` skill for retained UI conventions.

## Implementation Evidence

Not started. Record criterion → implementation → exact command/result with
PASS/FAIL/UNVERIFIED, diagnosis and remaining human/runtime gates here during
execution. The review's FIND IDs retain their identities; REQ-033 remains the
separate caller-directed obligation.

### Scenario 1 evidence (FIND-TASK-001-1)

Commit `b16c8e68f`. Write set: `crates/wyrd/wyrd-cli/src/{error.rs,server/install.rs,server/dev.rs}`,
`crates/wyrd/wyrd-cli/tests/{server_install.rs,pg_tests.rs}`, generated
`docs/src/content/docs/api/errors.md`, `docs/.../operate/troubleshoot-a-deployment.md`.
`scripts/build.sh` unchanged; its bundle layout set the rule.

**Reuse/owner.** Existing owner `ServerInstaller`. The gap: `extract` checked
marker names only, `install` skipped all checks when a cached `wyrd-server` existed,
and `installed` checked only the binary. Extension: one private
`install.rs::check_payload` rule, called from `extract`, from cached reuse
before `activate`, and from `installed`. It is static only, with no Node and no import resolver.
The rule follows the pinned adapter-node 5.5.7 output that `scripts/build.sh bundle` stages:
- non-empty `wyrd-server` and `ui/build/{index,handler,env,shims}.js`;
- `ui/build/client/_app/version.json`;
- at least one file under `ui/build/server`;
- `ui/package.json` parsed with `"type":"module"`;
- `ui/node_modules/<name>/package.json` for every declared production dependency.

New stable error `WYRD_CLI_409_SERVER_INSTALL_INCOMPLETE` names the directory
and its recovery: remove it, then rerun `wyrd server install`. `installed()` now
returns `Result<Option<_>>`, so `wyrd server dev` refuses an incomplete active
version locally, with no network call. Previously it reached `check_ui`'s "run
`wyrd server install`" advice, which would have reactivated the same version.

**Diagnosis (unexpected failure during GREEN).**
- Symptom: `pg_tests::server_dev_repeat_and_partial_failure_preserve_state`
  panicked at `pg_tests.rs:666`; it expected `"production UI"`.
- Evidence: stderr contained `WYRD_CLI_409_SERVER_INSTALL_INCOMPLETE`.
- Cause: the interim `installed()` folded "incomplete" into `None`, so DevArgs
  called `install()`. That made a live GitHub listing request and then refused.
- Fix site: shared `ServerInstaller::installed` became `Result<Option<_>>`
  (callers: `dev.rs::run` and the test helper). The test expectation moved to
  the new truthful code, which the finding requires.

A fresh read-only diagnostician independently reported the same cause, fix site
and callers.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Signed empty-marker payload rejected; current kept; no partial version | `extract` → `check_payload` | `server_install_preserves_previous_on_failure` (`empty_markers`) | PASS |
| Missing generated support (server chunks, `handler.js`) or production dependency rejected | `check_payload` | same test (`no_server_chunks`, `no_handler`, `no_dependency`, `no_ui`) | PASS |
| Cached binary-only / missing-deps latest refused before `activate`; current unchanged; cached bytes kept | `install` cached branch | same test (`incomplete_caches` loop) | PASS |
| Truthful recovery: following it (remove dir, rerun) succeeds; no reactivation | `ServerInstallIncomplete` message/remediation | same test (recovered installs); `assert_incomplete` | PASS |
| Already-active incomplete version: lookup refuses with recovery, reinstall does not reactivate | `installed()` → `Err`; `dev.rs` `?` | same test; `pg_tests::server_dev_repeat_and_partial_failure_preserve_state` | PASS |
| Healthy complete cache reused with no bundle download | `install` cached branch | `server_install_selects_latest_stable` (offline v0.2.4) | PASS |
| Realistic positive fixture; the real checkout UI layout passes | `UI_PAYLOAD` fixture; real `wyrd-ui` dir via `pg_tests` LocalDev | 4 `pg_tests::server_dev_*` with built UI | PASS |
| Signatures/digests, compatibility, staging, cancellation preserved | unchanged code paths | existing corruption cases in the same test | PASS |
| Source-free extracted-runtime load/start with system Node; Linux native | — | not run on this Mac | UNVERIFIED |

Commands (all from the worktree):
- `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_preserves_previous_on_failure)'`
  - RED: failed with `a bad update fails: InstalledServer { version: 0.2.1 … }`, a signed incomplete payload accepted.
  - GREEN: 1 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_selects_latest_stable)'`
  - 1 passed. This is a guard: healthy reuse already worked, and it stays green.
- After `cargo build --locked -p wyrd-cli -p wyrd-server --bins --tests` and the pnpm UI install/build, the four `pg_tests::server_dev_*` exact tests (the `test:local-dev:journey` nextest step under `with-test-postgres.sh`) passed: 4 passed.
- `mise run test:cli:journey`: 45 passed, 0 failed, 3 ignored.
  - Lane limit: this lane does not build `wyrd-server` or the UI. Without those artifacts, its four `server_dev_*` tests fail on their "build … through mise first" precondition. `test:local-dev:journey` owns building them.
- `cargo nextest run --locked -p wyrd-cli --lib`: 70 passed.
- `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run docs:check` (after `docs:generate`), and `git diff --check` all passed.

Non-goals held: no auth/UI/server/SQL/nginx/identity file touched, no Node at
install time, no generic JS verifier, no new bundle format, no `cfg(target_os)`.
`check_ui`'s UI-entry check was left in place as DevServer's launch prerequisite,
as the finding directs.
