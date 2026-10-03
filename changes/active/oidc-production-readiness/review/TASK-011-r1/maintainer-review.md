# TASK-011 Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `7c48ac7c99f018d3993922e63875839f3695c503`
- Candidate: `0b8919fff090d4b0109a506711cb119214d231f2`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-011-bff-openid-client.md`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Closure obligation also inspected: `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

I reviewed the complete base-to-candidate diff and the candidate source, without using another reviewer's conclusions. The candidate commit resolved to the supplied identity before and after this pass.

## Governing maintainer authority

- `AGENTS.md`, especially ownership, struct-centered workflow ownership, async boundaries, test taxonomy, and completion rules.
- `architecture/agent-rules.md`, including Rust item documentation and the prohibition on durable implementation-history notes.
- `architecture/references/languages/maintainer-style.md`.
- `architecture/references/languages/spec-driven-development.md`.
- `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and `architecture/wyrd-security-posture.md` for the UI/client, identity, token, and tenant boundaries.
- `.agents/skills/wyrd-ui/SKILL.md` and its SvelteKit architecture and verification references.
- The approved task and research direction requiring conventional `openid-client` and `jose` use, with SSO as the routine login and API-key sign-in as a separate recovery surface.

## Changed-surface coverage

| Surface | Symbols and owners inspected | Callers, consumers, and proof inspected | Maintainer assessment |
|---|---|---|---|
| Production browser-session owner | `BrowserSession` and `BrowserSessions` in `wyrd-ui/src/lib/server/auth/browser-sessions.ts`, including configuration discovery, cookie sealing, authorization start/callback, API-key exchange, access-token cache/renewal, read, logout, tenant switch, and metadata | `hooks.server.ts`; root switch/logout actions; login, callback, API-key recovery, tenant layout, and settings routes; `App.Locals`; `production-auth.integration.test.ts` | The workflow has one meaningful dependency- and state-owning concrete class. Public route operations are discoverable methods; small private helpers remain beside their owner. `openid-client` owns discovery, code grant, refresh, generic grant, and revocation; `jose` owns JWE operations. No second protocol abstraction or server-side session store was introduced. |
| Replaced session surface | Deletion of `server-sessions.ts`, `/login/complete`, flow-cookie/custom-CSRF consumers, and the old production-session tests; retained development-only `LocalSessions` in `session.ts` | All imports/references under `wyrd-ui/src`; local-auth routing tests; SvelteKit's built-in origin checking exercised by the production journey | Production and development identities remain visibly separate. Deleted parameters and component props were closed through their callers rather than retained as compatibility shims. |
| SvelteKit request boundary | `hooks.server.ts`, `+page.server.ts`, `/login/callback/+server.ts`, tenant layout, login route, API-key recovery route, and settings route | Generated route types through the checked `$types` imports; `routing/journey.test.ts`, `routing/tenant.test.ts`, `session.test.ts`, and the production journey | Route handlers are thin and delegate session mechanics to `BrowserSessions`. Secrets stay in server-only modules and cookies; page data is the narrow `SessionMetadata` projection. Error normalization remains centralized in `problem.ts`. |
| UI declarations and components | `app.d.ts`, `views.ts`, `Shell.svelte`, `TenantChooser.svelte`, login/recovery pages, settings page, and the changed Change Request components/routes | `Shell.test.ts`, `ChangesJourney.test.ts`, route/load tests, all changed component call sites | Props and declarations were updated consistently when `csrf` was removed. `App.Locals.browserSession` matches the server hook and settings consumers. No generated declaration was hand-edited; SvelteKit route declarations remain generated from source. |
| Dependency declarations | `package.json` and `pnpm-lock.yaml` | Direct imports in `browser-sessions.ts`; lock entries for `openid-client@6.8.8` and `jose@6.2.12` | Versions match the approved task. Direct `jose` use is declared directly even though it is also an `openid-client` dependency. No additional auth framework or custom OAuth transport package was added. |
| Production identity journey | `identity_ui_e2e.rs`, the `Browser`/gateway/provider helpers and four scenarios in `production-auth.integration.test.ts` | Two BFF processes, two bound Wyrd replicas, shared Postgres, Keycloak/Dex, TLS terminator, and the filtered identity lane in `mise.toml` | The helpers express caller outcomes and keep setup responsibilities localized. Scenario names state the behavior under proof. The host documents topology, failure conditions, and cleanup. The tests cover cross-replica use, browser secrecy, conventional OAuth operations, recovery login, tenant switching, provider replacement, and per-login logout. |
| Test-server seam | `WyrdTestServer::start_bound_replica` in `wyrd-testing/src/server.rs` | Existing `start_replica`, `bind`, and the sole new identity-journey caller | The method is a cohesive operation on the existing harness owner, reuses `start_replica`, and has substantive intent and `# Errors` rustdoc. It does not create a parallel fixture abstraction. |
| Lane wiring and task evidence | `mise.toml` UI filter handling; TASK-011 implementation-evidence table and command record | Exact host test selection and exact Vitest scenario selection | The special host-test name is explained inline and still resolves to one Rust host test; narrow UI journeys remain selectable. Full change journeys are correctly deferred to change review. |

## Owner and caller trace

`browserSessions` is constructed once in `browser-sessions.ts`. `hooks.server.ts` calls `read` for every protected production tenant request and installs the resulting `BrowserSession` into `locals`; the tenant layout asks the same owner for safe metadata. The login and callback routes call `begin` and `complete`; the recovery route calls `signInWithApiKey`; root actions call `switch` and `logout`; settings calls `BrowserSession.api`. There is no sibling production session writer or alternate token-refresh path left in the UI tree.

The access credential stays private to `BrowserSession`; its public projection is the typed tenant context plus `SessionMetadata`. The production journey verifies that the page/data channels and redirect history do not disclose credentials. The approved API-key tenant decision is visible in the recovery-route documentation and test: the route key remains routing context while the server-issued token supplies authority. The implementation does not add a competing tenant resolver, endpoint, or claim.

On the Rust side, `start_bound_replica` extends the existing `WyrdTestServer` owner and is consumed by `production_ui_bff_journey`. Every new or materially changed Rust item in the diff has substantive rustdoc; the new fallible method documents its error boundary.

## Generated and typed-contract parity

- `SessionMetadata` no longer declares `csrf`, and every changed Svelte prop and route consumer stopped supplying it.
- `App.Locals` replaced `serverSession` with `browserSession`, matching the hook, layout, and settings-route uses.
- New route handlers use their generated `$types` contracts rather than duplicating declaration shapes.
- The package manifest and lockfile agree on `openid-client` 6.8.8 and `jose` 6.2.12.
- No checked-in generated `.d.ts`, schema, or golden artifact was modified by hand.

## Verification evidence assessed

The task records successful focused proof for the complete write set:

- filtered `production_ui_bff_journey`, with all four real production UI scenarios passing;
- exact routing and Change Request Vitest cases;
- UI `check` with no errors or warnings;
- the complete UI test lane, 32 files and 177 tests;
- `mise run fmt`, `mise run lints`, and `git diff --check`.

I also ran `git diff --check` against the immutable range during this review; it reported no whitespace errors. Per the task direction, I did not require the unfiltered identity or every-language journeys here.

## Material findings

None.

## Non-blocking notes

- The recovery-route doc comment includes the label `REQ-010`. That label is not needed to understand the adjacent plain-language contract and may age independently of the code. This is wording-only, has no behavioral or public-contract consequence, and therefore is explicitly non-blocking under the review direction.
- The access-cache cap and its `ponytail:` comment are an implementation-level bound on the task-required per-replica cache. The limit is local, does not create a configuration surface, and cache eviction merely causes the already-required standard refresh path. I found no concrete maintenance defect to report.

## Uncertain preferences

None. `browser-sessions.ts` is sizable, but its methods share the same OAuth configuration, cookie key, and access-token cache; splitting it would separate one cohesive session lifecycle without a demonstrated maintenance benefit.

## Overall result

**PASS**

The changed behavior has a clear owner, conventional library-backed protocol calls, thin typed route consumers, complete caller cleanup, readable outcome-focused journeys, and synchronized declarations. I found no material maintainer defect and no missing Rust documentation.
