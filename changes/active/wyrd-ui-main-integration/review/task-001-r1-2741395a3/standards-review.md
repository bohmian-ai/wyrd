# Repository standards review

Result: **FAIL**

Subject: `8651b96fd0c049ab5e63b0606c98326af1fc8848...2741395a33d5e1e5ee5d7022916627917254a663`.
Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
The remediation range `deadf39af...2741395a3` was used to locate the new owners;
the audit covers cumulative distribution, browser authentication, and UI removal.
HEAD remained `2741395a33d5e1e5ee5d7022916627917254a663` at the final source check.
No other fresh discovery report or conclusion was supplied to this reviewer.

This is a repository-rule audit, not task acceptance or a complexity review.
The approved revision-10 architecture governs the Rust-owned session design;
the superseded confidential Node/OAuth client design is not an obligation.

## Authority coverage

| Changed surface | Applicable authorities | Source and consumer coverage |
|---|---|---|
| All changed production, tests, documentation, and tooling | `AGENTS.md` §§1–6, 9–12, 14–16; `architecture/agent-rules.md`; `architecture/references/README.md`; `languages/spec-driven-development.md`, `implementation-execution.md`, `maintainer-style.md`, `testing-workflows.md` under that reference root | Cumulative changed-file inventory and diff, immutable subject, remediation execution tables, manifests, owning symbols and retained consumers. No `.codegraph/` exists, so ordinary source navigation applies. |
| Protocol, new browser contracts, retired UI-only OAuth contracts, public errors | `architecture/wyrd-design.md` Doctrine, Client model, Runtime identity/browser-session boundary; `architecture/wyrd-doctrine.mdx`; reference `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md`, `languages/agent-harness.md` | `wyrd-spec/src/auth/{browser,device,mod,oidc,token}.rs`, `error.rs`; server route registrations and served OpenAPI construction; CLI error catalog; generated TS error-code union and docs error catalog. |
| Auth owner and sibling grants | Above plus `architecture/wyrd-security-posture.md` Principal/credential lifecycle, OAuth authorization server, First-party browser sessions, Authorization, Audit integrity, Cryptography | `wyrd-auth/src/{browser_sessions,callback,login,issuance,refresh,cli_logins,lib}.rs`; API-key exchange and shared issuer resolution; HTTP browser/callback adapter, `TokenGrants`, metadata/client identification, retained device/CLI/provider-test callers. |
| SQL and migration | `AGENTS.md` §§9, 15–16; agent rules SQL capability/RLS/caller-owned transaction/database-time rules; security Tenant and data isolation; operations Migration contract; reference patterns/Rust/testing rules | Actual production signatures and fields in new browser queries and auth owners; login-state and refresh-token query changes; migration `20261020000000_auth_browser_sessions.sql`; `pg_login_state`, `pg_migration`, server `pg_browser_sessions` fixtures and assertions. |
| Authentication middleware, permission/audit and served contracts | Security First-party browser sessions, Authorization and policy, Audit integrity; design Runtime identity; agent rules audit; reference agent-harness/errors/testing rules | `http/middleware/authenticate.rs`, `http/{router,openapi}.rs`, `auth/{browser,metadata,oauth,callback}.rs`, `components/auth/routes.rs`; tenant/Bifrost versus OTLP/MCP/platform/gateway ingress layers; `pg_openapi_contract` and browser tests. |
| Installer and native process lifecycle | `AGENTS.md` cross-platform/ownership/error/async/testing/rustdoc rules; operations Local server bundles, Browser-session deployment; security secret-handling rules; reference Rust/errors/testing/maintainer rules | `ServerInstaller::{installed,install}`, extraction/check/activation helpers and all changed callers; `DevServer` UI prerequisite/command/start/readiness/exit/settlement; installer and CLI PG tests, Python installed-bootstrap fixture. |
| UI rendering, auth forwarding, routes and retained settings | `AGENTS.md` server-owned semantics, secrets and accessibility; security First-party browser sessions; operations Browser-session deployment; `.agents/skills/wyrd-ui/SKILL.md`; its SvelteKit architecture, testing-verification and theming references; UI `brand/DESIGN.md`; reference TypeScript/maintainer rules | `hooks.server.ts`, browser-session owner/projection and upstream forwarding, root and tenant layouts/actions, login/recovery/settings pages, problem/session/development helpers, app CSS and component-catalog status, retained route tests and removed component/feature/mock consumers. |
| TypeScript and generated consumers | Reference `languages/typescript-guide.md`, `errors.md`, `agent-harness.md`; agent rules generated-artifact rule | UI exports and response types, generated SDK error-code union; package manifest/lockfile removes UI OAuth dependencies; no new N-API declaration or TypeScript SDK runtime API. |
| Python test support | `AGENTS.md` §§7–8, 11, 16; reference `languages/python-api-and-stubs.md`, testing rules | Only `sdks/wyrd-sdk-python/tests/integration/test_local_development.py` changes: installed production UI symlink, listener ownership, UI-child exit and restart cases. No Python runtime export, PyO3 wrapper or authored stub change. |
| Distribution, shell, CI/mise and documentation | Operations Supported topologies, Network/TLS, Rollout/version skew, Local server bundles and Release evidence; `AGENTS.md` test/gate/target rules; reference implementation-execution/testing/generated-output rules | `scripts/build.sh`, Debian/Rocky images and removed Ubuntu image, nginx/entrypoint, shared startup harness and changed startup/kind callers, identity provider lifecycle wrapper and new image identity runner, release matrix/manifests, `mise.toml`, docs troubleshoot/error changes, UI README and brand catalog. |
| Planning/reference packet | Spec-driven-development authority and subject's explicit approved-context classification | Active spec/tasks/reference assets and archived drafts are planning context rather than newly shipped runtime surfaces. No archived draft was promoted over active architecture. |

Bifrost engine/storage references are not selected: the candidate changes HTTP
authentication on the existing Bifrost router, not ingest/query/admission/storage
semantics. PyO3 lifetime references are not selected: no binding implementation
changes. The existing Python/API reference is selected for its changed tests.

## Rule results

`PASS (static)` means source conformance was inspected. It does not claim a new
execution of a reported gate.

| Rule | Result | Exact rule/source evidence |
|---|---|---|
| Server owns durable identity/session behavior; clients project it | PASS (static) | `AGENTS.md` §§2–3, 9; security First-party browser sessions. Session creation/resolution/revocation lives in `wyrd-auth::BrowserSessions` and `wyrd-sql` tenant queries. UI `BrowserSessions` forwards requests/cookies and holds no durable session cache. |
| Pure `wyrd-spec`, typed requests/responses, derive-backed errors | PASS (static) | `AGENTS.md` §§2, 4, 9. `auth/browser.rs` contains serde/schema contracts, no IO or runtime; `OriginRefused` has canonical derive metadata and field docs. `ServerInstallIncomplete` follows the CLI catalog derive. No new dependency or Cargo feature is introduced. |
| Actual SQL capability types, not merely scanner results | PASS (static) | Agent rules permit only `TenantConn`/`OperatorPool`. New production query functions accept `&mut TenantConn<'_>`; browser-session auth opens transactions through `WyrdPostgres::tenant_conn`; slug/directory lookups use existing `WyrdPostgres` capabilities. No raw pool/connection/transaction production signature or field is introduced in the changed SQL/auth/boot owners. Test superuser queries remain fixture-only. |
| Tenant transactions compose; query callees never commit | PASS (static) | Agent rules caller-owned lifecycle. `insert_browser_session`, `live_browser_session`, `revoke_browser_session`, and login-state queries leave commit to the auth workflow. Auth workflows commit transactions they acquired themselves. |
| Tenant RLS and database-owned time | PASS (static) | `AGENTS.md` §15 and agent rules. New table enables and forces RLS; both tenant and operator policies mirror current authority. Tenant queries add no redundant tenant-selection predicate. SQL derives insertion expiry/revocation/purge eligibility with `statement_timestamp()` and bound duration; the table caps absolute lifetime at twelve hours. |
| Migration preserves old records while retiring their consumer | PASS (static) | Operations Migration contract and security OAuth authorization server. New migration adds browser binding and changes the initiation check without deleting old login/code rows; login-state consumption excludes `oauth_client_id IS NOT NULL`; old rows expire through existing purge. `pg_login_state` explicitly seeds a retained old row and verifies it is not consumed. No previously committed migration is edited. |
| Shared issuer, permission checks and non-blocking audit | PASS (static) | Agent rules audit; security Authorization and Audit integrity. Resolved sessions produce `VerifiedToken` through `TenantTokenIssuer::verify`, and middleware inserts the existing `AuthenticatedPrincipal` shape. Handler permission/audit consumers remain in place; allowed login staging uses the existing audit owner after commit and does not await an audit write. No alternate sink/staging table is introduced. |
| Browser trust boundary, cookie authority and bearer precedence | PASS (static) | Security First-party browser sessions. New cookies hold random opaque identifiers; SQL stores digest; secret-bearing newly created session/login values use `SecretString` and redacted `Debug`. Rust rejects missing/foreign Origin on mutations, ambiguous tenant-API cookies and invalid session authority; an explicit access-token header takes the bearer path. Tenant HTTP/Bifrost layers alone gain cookies. |
| BFF does not turn store failure into cached success | **FAIL — REP-002** | Security First-party browser sessions and operations Browser-session deployment require unavailable responses rather than cached/anonymous success. `browser-sessions.ts:262` maps every non-successful enumeration to `[]`, then retains the old selected session in page metadata. |
| Served OpenAPI tracks real routing; generated projections remain source-owned | PASS (static); execution reported | `AGENTS.md` §11, agent-harness reference. `BrowserSessionAddon` collects protected tenant/Bifrost route registrations; public browser endpoints use typed contracts and problem types. Served `pg_openapi_contract` probes validate cookie alternatives and dead-cookie refusals. Error docs and TS error codes project the new catalog code. Task evidence records successful codegen and served-document checks; this review did not rerun them. |
| Cohesive retained-state Rust owners; sync pure helpers and IO async | PASS (static) | `AGENTS.md` §§5–6. Existing `ServerInstaller` and `DevServer` retain installation/process resources; `BrowserSessions` retains issuer/connection dependencies. Cookie construction/parsing, permission projection and payload inspection stay synchronous; async auth/query operations await IO. |
| Rustdoc includes private items and durable cancellation/partial progress | **FAIL — REP-001** | `AGENTS.md` §16 and agent-rules rustdoc rule. Newly added `Debug::fmt` implementations lack rustdoc; newly added test modules lack module docs. Durable session creation/revocation docs omit relevant cancellation/uncertain-commit consequences despite separate statements/commit/response stages. See exact locations below. |
| Dependency ownership and language-runtime test placement | PASS (static) | `AGENTS.md` §§3, 7–8, 11. No dependencies are moved into foundational/client crates, no new PyO3/N-API lifetime tests are added to Rust, and Python installed-bootstrap behavior remains in Python. UI OAuth dependencies are deleted consistently from manifest/lock. |
| Installer/distribution target and owner conformance | PASS (static); platform execution limited | `AGENTS.md` supported targets; operations Local server bundles. Payload rule is shared by extraction, cache activation and active lookup; installer errors give actionable directory recovery. CLI launches production UI on system Node. Release matrix retains Debian amd64/arm64 and Rocky amd64; bundles contain production UI. Native/Linux completeness is not inferred from macOS evidence. |
| Tooling proof invokes real surfaces and owns dependencies | PASS (static); image execution unverified | `AGENTS.md` §11; testing-workflows. CLI journey now builds server/UI prerequisites. Identity provider lifecycle is factored into one wrapper; focused UI task drives real Keycloak/Dex, Rust replicas and production Node BFF. New container leaf invokes real nginx journey via existing startup harness and records immutable image provenance. Container build refused on macOS; no successful Linux/image evidence is present. |
| Test integrity and exact selectors | PASS for inspected changed assertions/commands; evidence limits below | Retired authorization-route tests compare against actual unrouted fallback; security-alternative expectations change with actual new auth behavior; SQL expected table inventory includes the actual new table. New named Rust proof recipes use nextest exact expressions. No newly added allowance disables a gate. Existing UI mocked feature tests are deleted with their production mock/components under the explicit removal outcome, not hidden behind skips. |
| UI server/client separation and basic accessibility | PASS (static), visual proof unverified | UI skill server-side load rule; AGENTS accessibility. Server-only session helpers are under `$lib/server`; page data receives safe context; forms/nav/buttons are semantic and labeled; global focus style uses declaration token; token palette/local fonts retained. Rendered mobile/theme/keyboard inspection is absent and is not marked passed. |
| Documentation and naming hygiene | Nonblocking notes only | New retained CSS uses a 2px control border/mono label font despite brand 1px/sans rules; `ui-library.test.ts` embeds `TASK-002` in a test title. Fully qualified type paths appear in new test signatures and new CLI error field. These are direct textual-rule discrepancies, but no independent security/behavioral/public-contract consequence is established; they do not drive FAIL. |

## Proposed material findings

### REP-001 — VIOLATION: new Rust session items do not meet the mandatory rustdoc gate

**Authority:** `AGENTS.md` §16 requires every new/materially modified item,
including private methods and test modules, to have substantive rustdoc; every
fallible method requires `# Errors`, and durable async operations must document
relevant cancellation/partial-progress behavior. Agent rules explicitly classify
missing/placeholder rustdoc as `BLOCK_BEFORE_MERGE`.

**Exact changed locations and evidence:**

- `crates/wyrd/wyrd-auth/src/browser_sessions.rs:71`, `:88`, `:106`:
  `CreatedBrowserSession::fmt`, `BegunBrowserLogin::fmt`, and
  `BrowserSessions::fmt` are newly introduced, have no rustdoc at all, and return
  `fmt::Result`. The first two implement security-significant redaction of cookie
  and login-binding material. The type docs do not document these methods or
  their formatter error behavior.
- `crates/wyrd/wyrd-auth/src/browser_sessions.rs:486` and
  `crates/wyrd/wyrd-server/src/auth/browser.rs:511`: newly introduced `tests`
  modules have no module rustdoc. The latter file's `pg_tests` module, by
  contrast, does have a substantive description above its declaration.
- `crates/wyrd/wyrd-auth/src/browser_sessions.rs:136`, `:172`, `:197`, `:288`
  and the HTTP creation/logout boundaries in
  `crates/wyrd/wyrd-server/src/auth/browser.rs:204`, `:353`, `:490` explain the
  successful workflow and error returns but do not explain cancellation or
  uncertain delivery after the owner commits. A cancelled begin can leave
  five-minute login state without delivering its binding; recovery can commit
  the new digest row before the HTTP cookie is delivered; logout can commit a
  revocation before cookie clearing is delivered. Those are relevant persisted
  partial-progress/retry consequences, not a demand for a new runtime mechanism.

**Consequence:** the explicit repository documentation gate remains unsatisfied;
maintainers lack the required explanation of secret redaction and durable
commit-versus-response boundaries. The task-review skill explicitly retains
missing rustdoc as blocking, even where style-only findings ordinarily do not.

**Smallest testable correction:** document the listed new methods/modules and
the cancellation/partial-progress outcomes on the owning durable methods and
HTTP boundaries, referring to the existing transaction/lifecycle owner where
appropriate. Do not add implementation machinery or change session semantics.
Perform the same documentation pass over cumulative materially modified Rust
items, rather than limiting it to public API docs. Closure is source inspection
against §16 plus normal focused format/lint verification; no new runtime test
is needed to prove prose exists.

### REP-002 — INCORRECT: session metadata converts a store refusal into successful cached identity

**Authority:** security First-party browser sessions requires enumeration to
return only live per-tenant contexts, a refused selected tenant never to fall
back, and store failure to refuse the whole request. Operations Browser-session
deployment likewise requires unavailable responses, never cached success.

**Exact location:**
`crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:262`.

**Producer-to-consumer trace:**

1. `hooks.server.ts` calls `BrowserSessions.read` for the route tenant and puts
   the resolved `BrowserSession` into `locals.browserSession`.
2. `routes/t/[tenantKey]/+layout.server.ts:11` calls `metadata` on that earlier
   session. `metadata` makes a second Rust `/auth/browser/session` request with
   all presented tenant cookies.
3. The Rust enumeration deliberately returns 503 if any session's lookup fails
   due to unavailable storage (`auth/browser.rs::session` → `live_view`), and
   401 if no session is live. Revocation, expiry or a storage failure between
   the two requests is reachable; an enumeration failure affecting another
   tenant cookie is also sufficient after the selected-cookie read succeeded.
4. `response.ok ? ... : []` discards that authoritative refusal. The next line
   unconditionally constructs `tenants` from the earlier session, and the
   function returns its prior principal/expiry with successful page metadata.
   A 200 enumeration that no longer contains the selected tenant is handled the
   same way because the selected tenant is inserted regardless of `live`.

**Observable consequence:** the tenant layout renders a successful signed-in
identity and incomplete tenant chooser when the authoritative enumeration says
storage is unavailable, or retains a selected identity after it is no longer
live. No backend permission escalation is claimed; the defect is precisely the
forbidden cached-success/error-state behavior at the browser boundary.

**Smallest testable correction:** enforce the enumeration's result in the
existing `BrowserSessions.metadata` owner. Reuse the current safe problem/refusal
mapping rather than treating non-2xx as an empty list; require the selected
tenant to be present among the returned live contexts before returning its
metadata. Preserve dead-cookie relay and independent tenant sessions. Add a
focused case to existing `browser-sessions.test.ts` for successful selected read
followed by 503 enumeration, and for a now-absent selected tenant (401, or 200
containing only another live tenant). Assert refusal rather than old-session
success. Run that exact Vitest file through `mise exec --` and UI check; no new
harness, dependency, service or session store is required.

## Verification limits and non-findings

- This independent review ran source/diff/manifest inspections only. It did
  not run Cargo, codegen, shared services, image builds or broad gates, as the
  orchestrator required. The remediation task records successful scoped SQL,
  auth, CLI, UI, codegen, lint and served-OpenAPI commands; those claims were
  compared with real registered tests/owners, not treated as proof of source
  correctness.
- Linux native source-free bundles, Debian amd64/arm64 and Rocky amd64 runtime,
  the real nginx/provider journey and human visual inspection are explicitly
  unverified in the execution evidence. The image journey is written and wired;
  this report does not misclassify it as absent or passed. Acceptance ownership
  of those remaining proofs belongs to the task reviewers/orchestrator.
- An initial concern about missing duplicate Origin checks in production
  actions was investigated and is not a material finding: pinned SvelteKit
  `respond.js` rejects foreign/missing Origin for form content types, and
  `page/actions.js::call_action` rejects non-form content types with 415 before
  dispatch. The candidate does not disable these checks. No reachable action
  bypass was established.
- Existing raw-pool migration-test helpers and pre-existing undocumented items
  were not reported merely because they are nearby. REP-001 names new items and
  new durable workflow documentation responsibilities.

Skipped execution and visual inspection as instructed; runtime proof gaps remain
explicit, and the two proposed findings require independent validation.
