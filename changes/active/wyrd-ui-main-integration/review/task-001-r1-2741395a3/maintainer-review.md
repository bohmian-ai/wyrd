# Independent maintainer review

Result: **PASS** (maintainer scope).

Subject: base `8651b96fd0c049ab5e63b0606c98326af1fc8848` → candidate `2741395a33d5e1e5ee5d7022916627917254a663`. The remediation range from `deadf39af` was used for navigation; the review includes the cumulative distribution, retained-page and browser-session changes. HEAD remained the recorded candidate. No source was edited.

## Authority and coverage

Read the task-review skill, maintainer guide, repository agent rules, original TASK-001/TASK-002 and R1 task/evidence. Applied revision-10 browser ownership in the spec, the identity/browser sections of `wyrd-design.md` and `wyrd-security-posture.md`, doctrine, and deployment/release authority. No CodeGraph directory exists.

| Changed surface | Owning symbols and caller/test expansion | Assessment |
|---|---|---|
| Signed installation and cached/current selection | `ServerInstaller::{installed,install,activate}`, `extract`, `check_payload`, `contains_file`, `DevArgs::run`; `server_install` positive/corruption/cache/recovery fixtures and `LocalDev` | One shared eligibility rule covers all admission paths. Errors distinguish absent installation from incomplete installation and name actionable recovery. Existing signature, staging and activation owners remain discoverable. |
| Native lifecycle | Full `dev.rs`: `DevServer::{new,command,ui_command,check_ui,start,run,ready,exited,stop}`, `settle`, provisioning/saved-client/sealing-key helpers; CLI `pg_tests` and Python local-development process tests | The existing meaningful lifecycle owner retains both children. Startup, saved state, cleanup and prerequisites stay in that owner; no second controller or generic process framework was added. Added test setup extends the existing deployment fixture. |
| Auth contracts and declarations | `auth/browser.rs`, `LoginInitiation`, token/client metadata contracts and auth re-exports; stable errors and generated error-code/docs changes | Request/result types state the browser boundary and safe projection. Removal of UI-only code/client contracts is paired with caller migration; CLI/device contracts remain explicit. No Python or N-API signature change is introduced. |
| Browser lifecycle and authority | Full `browser_sessions.rs`, changed `HumanConnections::begin_bound`, `AuthorizationCodeExchange` callback/completion/fencing workflow, `TenantGrant` shaping and `TenantTokenIssuer::verify`, refresh owner | New retained state is held by a cohesive concrete owner. Recovery reuses `ExchangeApiKey::authenticate`; request authority reuses the issuer. Provider discovery/redemption, identity mapping, role synchronization, locks and callback outcomes remain in existing connection/callback owners. The new session record is an approved distinct lifecycle, rather than a second OAuth exchange workflow. |
| SQL ownership | Browser-session migration and query module; login-state binding encoding/decoding/consumption; refresh-row removal and query re-exports; migration/login-state tests | Digest-only records, explicit authority enum, database time and tenant transaction ownership are readable from the module documentation and signatures. Existing query module conventions are retained. Historical OAuth columns remain in the migration instead of inventing a compatibility execution path. |
| HTTP integration and served schema | `auth/browser.rs`, callback adapter, `components/auth/routes.rs`, OAuth/client identification, metadata, authentication middleware, router composition and `BrowserSessionAddon`; integration browser-session and OpenAPI consumers | HTTP adapters move typed input/cookies and delegate durable behavior. Cookie authentication feeds the existing `AuthenticatedPrincipal` boundary. The served-document alternatives derive their path set from the same mounted groups, avoiding a second route catalog. |
| Node projection and retained UI | Full `BrowserSessions`/`BrowserSession`, `call`, `relayCookies`, upstream forwarding, hooks, root actions/load, tenant load/layout and settings action consumers; auth, upstream, routing and production-auth tests | Node holds request forwarding context and safe metadata, with the opaque selected cookie private. Existing SvelteKit action and error patterns are retained. Deleted OAuth dependencies and callback are closed with their callers. Settings continues through `session.api` and Rust permission checks. |
| Removal and plain retained pages | Cumulative deleted component/feature/mock/domain routes, retained root/login/tenant/settings markup, development identity gate, app CSS and UI-library regression test | Deletion removes the obsolete UI instead of introducing a replacement component framework. Repository search found no surviving imports of removed component/feature/domain mock owners, removed OAuth types/functions, or the mock-data switch in shipped consumers. Retained local identity remains behind its existing development gate. |
| Packaging and deployment proof | `scripts/build.sh`, release workflow, Debian/Rocky Dockerfiles, entrypoint/nginx, startup common/dev/prod/identity owners, kind image selection, mise prerequisites, deployment docs | One bundle producer remains authoritative. Image/runtime startup extends the existing owners. Managed-provider setup was extracted from the original mise body and shared by focused/native/container lanes, rather than cloned. |
| Real identity journeys and fixture lifetime | Full `identity_ui_e2e.rs` and production-auth Vitest driver, browser-session integration module, changed identity/OpenAPI assertions, installer and local-process assertions | Outcomes are observable at public HTTP/UI boundaries. Replica and nginx proof have explicit fixture owners and lifecycle cleanup. Tests distinguish refusal, cookie clearing, safe metadata, revocation, selected authority and bearer precedence. Written nginx proof is not treated as executed proof. |

## Whole-workflow reuse assessment

The main risk was rebuilding the auth workflow behind a new well-shaped struct. The source instead keeps provider initiation in `HumanConnections`, provider redemption and user/role completion in `AuthorizationCodeExchange`, key authentication in `ExchangeApiKey`, principal shaping in `TenantTokenIssuer`, and request authorization/audit in existing server consumers. `BrowserSessions` owns the newly approved session lifetime and persistence coordination; its purpose is not satisfied by the retired refresh-token lifecycle. The HTTP and Node classes project that owner rather than implementing another durable authority.

The installer likewise extends `ServerInstaller` once; cached selection, fresh extraction and active lookup invoke the same private payload check. Static layout checks remain separate from runtime imports in the existing build/launch owners. The provider setup extraction is shared by its original identity lane and new focused lanes. No material duplicate workflow, unearned trait/framework, misplaced durable owner, or declaration mismatch was found.

## Material findings

None.

## Non-blocking note

`wyrd-ui/src/lib/ui-library.test.ts:15` retains the ephemeral `TASK-002` identifier in its test title. The agent rules and maintainer documentation advise permanent code to describe behavior without task history. A behavior-only title would improve test output outside this packet; it has no runtime or public-contract consequence and does not block acceptance.

## Verification limits

This was a static maintainer audit; no heavy builds, identity-service mutations, or shared test lanes were run. Recorded focused tests and checks were inspected as evidence claims, not independently rerun. Linux native/image runtime, source-free signed-bundle runtime and human visual acceptance remain explicitly unverified in the task evidence. Those acceptance gates are for the orchestrator to reconcile; this PASS establishes maintainer shape and consumer closure, not cross-platform deployment acceptance.
