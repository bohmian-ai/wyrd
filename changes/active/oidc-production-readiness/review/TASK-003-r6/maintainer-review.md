# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
- Approved specification: `SPEC-oidc-production-readiness`, revision 7
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation tasks: TASK-003 R2, R3, R4, and R5 as supplied by the human
  owner; R3, R4, and R5 were explicitly authorized.
- Human directions: the supplied issuer-binding direction, real connection-test
  direction, and the R5 direction replacing `FIND-TASK-003-18` with
  per-session refresh-chain retirement under RFC 7009 grant scope.

The candidate resolved to the stated commit before and after inspection. I
reviewed the cumulative base-to-candidate change and used
`989d0734b0a9b04f314ef4b52aa7d8510f26fe11..ad3b92ad0f326917c731383fd3b57cb7ac6a8c82`
only to locate the latest remediation edits. I did not modify reviewed source.
The repository has no `.codegraph/` directory, so navigation used Git, `rg`,
and direct source inspection.

## Authorities Read

- `AGENTS.md`, especially ownership, struct-centered Rust, async boundaries,
  public contracts, test tiers, and mandatory Rust documentation
- `architecture/agent-rules.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/maintainer-style.md`
- Applicable identity and client/server authority in
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/wyrd-security-posture.md`
- The approved specification, original task, all supplied remediation tasks,
  all supplied human directions, and the prior cumulative finding record

## Changed-Surface Coverage

| Surface | Changed owners, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| Auth wire contracts and provider callback | `ConnectionTestRequest`/`Response`, `LoginInitiation`, `CallbackQuery`, `ProviderMetadata`, `AuthorizationCodeExchange`, callback handlers, generated callback schemas, and contract tests | The types use domain names and keep provider response tolerance and conditional issuer binding on the existing exchange owner. Source and generated schemas agree; no duplicate callback or connection-test contract was added. |
| Human connection management | `HumanConnections::{begin_test,tested_candidate,stamp_test_sign_in}` and stage/activate/deactivate/remove callers, SQL state, admin transport adapters, settings actions, and provider-backed tests | Candidate testing reuses the ordinary PKCE/state/nonce and code-exchange path while keeping its distinct result on the existing concrete owner. The transport and UI remain projections rather than second lifecycle owners. |
| Browser-session lifecycle | `BrowserSessions::{complete,exchange_api_key,read,authority,current,renew,logout}`, `Renewal`, credential sealing/opening, BFF handlers, `ServerSessions`, hooks/routes, and unit/Postgres/journey tests | One dependency-owning Rust struct owns durable creation, renewal, and logout; one TypeScript class owns BFF cookies, CSRF, and server-only authority. Refusal, containment, retryable internal failure, and mode-specific logout remain explicit and discoverable. |
| R5 session-chain identity | `BrowserSessions::complete`; `BrowserSessionWrite`; `LockedBrowserSession`; `insert_browser_session`; `lock_browser_session`; migration `20261001000003`; every struct construction and field consumer | The only new durable value is the non-secret id of the login's first refresh row. It is captured at completion, carried by the existing session row, and constrained to OIDC mode with a tenant-scoped composite foreign key. Field and migration documentation state that it is the chain root, not a new credential or public identity. |
| R5 logout and refresh SQL | `BrowserSessions::logout`; `lock_refresh_family`; `revoke_refresh_chain`; refresh insertion/rotation/replay callers; SQL re-export; focused logout test | Logout stays on its natural owner, reuses the established family lock, calls one narrow tenant-scoped recursive update, and wipes the browser row in the same caller-owned transaction. The table invariant lets the method branch on `refresh_chain_id` without a duplicate mode check. No trait, retry framework, second revocation owner, or public API was introduced. |
| Session-chain proof | `oidc_session`; `oidc_logout_retires_only_its_refresh_chain_without_opening_it`; `stored_state`; ordinary `RefreshTokens::execute`; API-key logout journey | The focused Postgres test creates the caller-visible failure: a committed successor plus an unreadable stored envelope. It proves the successor is retired, cannot rotate, the browser row is wiped, and another login for the same User remains active. It reuses the existing fixture and refresh owner rather than adding a harness. |
| SQL and sealing inventory | Browser-session queries/migrations, refresh-token queries, login state, human-connection sealed-secret inventory, `TenantConn` call sites, migration tests, and boot sealing tests | Persistence code remains on `TenantConn`; row shapes expose stored invariants; sealing inventory still covers the credential envelopes but correctly excludes the non-secret chain root. New fields are documented at the write, locked-row, raw-row, migration, and test construction sites. |
| Private BFF transport and server composition | `BffChannel`, its typed handlers and service-key middleware, auth state, router/boot/config assembly, callback completion, and transport tests | The private routes remain thin adapters over `BrowserSessions`; service-key admission is centralized, and server-derived tenant/session authority is not moved into SvelteKit. Boot composes existing concrete owners without another service layer. |
| SvelteKit production UI | `ServerSessions`; hooks; upstream validation; root, login, completion, tenant layout, settings routes; shell/chooser components; action helpers; Vitest tests | Public and internal TypeScript operations have explicit result types and descriptive names. Cookie/tenant checks, CSRF, token confinement, settings projection, and tenant switching remain findable through one server-session boundary. |
| User journeys and provider fixtures | Rust identity hosts, two-replica Vitest journey, Keycloak/Dex fixture support, selector wiring in `mise.toml`, and existing focused/unit selectors | Tests are named for caller outcomes and keep process/provider mechanics in existing helpers. The R5 proof is local to the owning Rust module; no parallel fixture or speculative concurrency harness was added. |
| In-process test-server shutdown fix | `WyrdTestServer::shutdown`, `Mode`, `AppState` shutdown token, `Bifrost::shutdown`, production boot rollback, and explicit-shutdown callers including the previously aborting card-registration tests | The shared harness owner now cancels and drains in-process Bifrost before fixture drop. Bound mode still delegates to its serve task. The change is a small mode branch on the existing owner and reuses Bifrost's ordered drain/abort behavior; the rustdoc records the otherwise non-obvious database-drop/Oracle failure chain and error policy. |
| Documentation and generated artifacts | Security posture, self-hosting authentication/SSO docs, schema docs, callback JSON schemas, and generated aggregate docs | Operator-facing setup, browser credential confinement, candidate testing, issuer binding, and sealing guidance match the inspected code. Generated schema copies remain identical. |

## Material Findings

None.

## Prior-Finding and Direction Closure

| Item | Current-source maintainer evidence | Result |
|---|---|---|
| `FIND-TASK-003-18` as replaced by human direction | `BrowserSessions::complete` records the initial refresh-row id; `logout` takes the existing User family lock, invokes `revoke_refresh_chain`, then wipes and commits the browser row. The focused Postgres test proves an unreadable envelope and committed successor cannot survive while a separate login stays active. | CLOSED |
| R4 classifier and stored-envelope findings | Error classifiers remain crate-visible only; `Renewal` preserves the containment/refusal/failure transaction meanings; `open_text` and its caller docs still separate envelope opening from lifecycle policy; focused tests remain present. | CLOSED |
| R3 renewal and rustdoc findings | Replay containment, exact-expiry behavior, bounded relock, retryable internal failures, and the materially changed test helpers retain substantive documentation and focused proof. | CLOSED |
| R2 and earlier cumulative findings | Conditional issuer binding, real candidate sign-in, fixed-cost API-key refusal, tenant-derived chooser/session state, canonical sealing inventory, trusted upstream TLS, typed settings projection, and two-replica/provider journeys remain on their established owners. | CLOSED |
| Human issuer direction | Present `iss` is compared exactly; absence is required only when the provider advertises the parameter. | CLOSED |
| Human connection-test direction | The candidate test uses the normal browser authorization-code exchange and stamps only the exact tested revision after authority recheck, without issuing a User session or credential. | CLOSED |
| Human logout-scope direction | Logout retires descendants of only the session's stored root; it does not call principal-wide `revoke_refresh_family`, and API-key logout continues to wipe only browser state. | CLOSED |

## Uncertain Preferences Kept Out of Findings

- `refresh_chain_id` stores the root refresh-token row rather than a separate
  chain entity. `refresh_chain_root_id` would be slightly more literal, but the
  field, migration, query, and method documentation all state the invariant;
  renaming would add churn without removing a concrete maintenance hazard.
- `revoke_refresh_chain` relies on its caller to hold `lock_refresh_family`.
  This matches the existing refresh-query convention, is documented adjacent
  to the function, and currently has one workflow caller; a wrapper or new type
  would be unearned indirection.
- `WyrdTestServer::shutdown` logs a failed in-process drain rather than adding a
  new harness error variant. `Bifrost::shutdown` already executes the abort
  fallback, so widening the public harness error surface would not improve
  cleanup or the caller's recovery choice.

## Verification Assessment

The R5 implementation record reports the exact focused browser-session selector
and all eight browser-session tests green, followed by `fmt`, `lints`,
`codegen:check`, `check:tenant-isolation`, `test:sql`, `test:wyrd`, and
`test:identity:journey`. After the shared harness shutdown correction it reports
`test:wyrd` green with 2348 tests, and reruns of formatting, lints, the identity
journey, and `git diff --check` green. Earlier cumulative evidence covers the
UI test/typecheck lanes, two-replica real-provider journeys, and documentation
checks.

I inspected the named tests, their assertions, current lane wiring, cumulative
source, and latest remediation diff. I did not rerun Cargo, Postgres, provider,
browser, mise, or pnpm commands in this review-only role. `git diff --check` for
the immutable base-to-candidate range is clean.

## Overall Result

**PASS**

The cumulative implementation remains organized around the existing concrete
identity, browser-session, SQL, BFF, and harness owners. R5 closes the logout
gap with one stored root id and one SQL operation, and the authorized harness
fix belongs on the shared shutdown method. No material layout, owner/method
shape, naming/type, test-clarity, documentation, generated-parity, or
unjustified-complexity defect remains.
