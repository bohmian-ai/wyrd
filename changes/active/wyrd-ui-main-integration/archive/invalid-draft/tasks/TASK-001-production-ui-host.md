---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-010, REQ-014, REQ-052]
invariants: [INV-002, INV-004, INV-007, INV-008]
acceptance: [AC-008]
depends_on: []
parent_task: null
remediates: []
---

# TASK-001 — Production UI plumbing and login proof

## Outcome and Value

The existing SvelteKit UI ships in the official Wyrd image behind the existing
public nginx listener, alongside `wyrd-server`. The same canonical image build
works from supported macOS and Linux development hosts, and the existing
production UI login journey remains green.

This task establishes deployment plumbing only. It does not onboard Cards,
verification, Observe, Query, Bifrost, or any other product data into the UI.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| SvelteKit production application | `wyrd-ui`'s existing adapter-node `build` and login/session routes | `hooks.server.ts`, login/callback routes, `browser-sessions.test.ts`, `production-auth.integration.test.ts`, `identity_ui_e2e::production_ui_bff_journey` | The application and login already exist, but current main's official image does not ship or start the built UI | Package and start the existing production build without changing its product behavior | None; reuse the current application and login boundary |
| Official build | `scripts/build.sh::{bundle,image}`, `mise.toml::docker:build`, official distro Dockerfiles | Release image jobs and `test:server:startup:prod` | `image` copies a host-built bundle and rejects non-Linux hosts | Extend the existing image build so its Linux runtime artifacts are built for the image target when invoked from either macOS or Linux | No second build system, cross-compilation framework, or public configuration surface is justified |
| Public process and route ownership | Official `entrypoint.sh` and `nginx.conf.template` | Server startup journeys; the historical `wyrd-server-ui` image route split | Current main starts only Rust and nginx and sends every HTTP route to Rust | Start the built SvelteKit server beside Rust and retain one nginx-owned public HTTP endpoint: established Wyrd server routes go to Rust and UI routes go to SvelteKit | None; extend the existing official entrypoint and proxy |
| UI login proof | Existing SvelteKit login action/session flow and `identity_ui_e2e::production_ui_bff_journey` | `production-auth.integration.test.ts` and `mise.toml::test:identity:journey` | The existing login journey must prove the packaged host work does not break login | Run the current identity/UI journey unchanged and repair only demonstrated regressions at their existing owner | None; no new identity host, fixture, browser dependency, or selector |

The task owns only build, package, process, proxy, and login-regression closure.
Later tasks may consume the server-only UI boundary when they introduce an
actual live page.

Prohibited changes:

- No Bifrost client, query, table discovery, or analytical test.
- No generated HTTP client or generated-contract workflow.
- No Card, verification, Observe, Query, home-data, or other live page call.
- No new `WYRD_*` environment variable, endpoint override, test target, test
  filter, or exact-selection mechanism. Tests use the repository's existing
  public/runtime configuration and existing owner lanes.
- No new browser automation dependency or separate test-environment
  controller solely for this task.
- No new public listener, compatibility route, browser-visible credential, or
  second deployment topology.
- No auth protocol, permission, tenant, session, or UI redesign. A discovered
  defect is repaired only at its existing owner.

## Approach

1. Extend the current official bundle/image path to include the existing
   adapter-node production build and the runtime it already requires.
2. Make the canonical image build produce Linux image artifacts when invoked
   from either macOS or Linux; do not copy incompatible host-native artifacts
   into the image.
3. Extend the existing official entrypoint to supervise Rust, SvelteKit, and
   nginx, preserving one public listener and clean failure when a required
   process exits.
4. Re-establish the historical route ownership under current paths: Wyrd API,
   auth, platform, MCP, OpenAPI, health, readiness, and OAuth discovery remain
   Rust-owned; UI pages and SvelteKit data/action routes reach SvelteKit with
   the forwarded origin needed by the existing login flow.
5. Exercise the existing UI login through its current unfiltered identity
   journey, without adding identity setup to the startup lane or adding selector
   plumbing.

## Ordered Implementation Scenarios

### Scenario 1 — The official image serves the existing UI

**Behavior.** The official image starts its required processes behind one nginx
HTTP endpoint. Rust-owned routes still reach `wyrd-server`; a UI route reaches
SvelteKit; and forwarded host and scheme remain intact for the existing auth
boundary. Mapped obligations: REQ-010, REQ-052, INV-002, INV-008, AC-008.

**RED.** Add these route assertions to the existing official-image production
startup journey and run `mise run test:server:startup:prod`. On reverted main,
baseline, the UI route cannot reach a packaged SvelteKit process, so the new
assertions fail before production plumbing is added. Do not add an environment
variable or selector to isolate the assertions.

**GREEN.** Add only the bundle, image, entrypoint, and proxy behavior required
to serve the UI while all earlier startup assertions remain green.

**REFACTOR.** Remove duplicated staging or routing work where the existing
official owners can express it directly. Keep one canonical image build, one
entrypoint, and one public route map.

## Acceptance Criteria

- The repository's canonical official-image build invocation succeeds on a
  supported macOS host and on a supported Linux host.
- The resulting image contains Linux-compatible runtime artifacts regardless
  of the build host; a macOS executable is never copied into a Linux image.
- The image starts `wyrd-server`, the existing SvelteKit production build, and
  nginx under the existing official process owner, and terminates when a
  required process fails.
- Existing Rust-owned public routes remain Rust-owned. UI routes, SvelteKit
  data routes, and login actions reach SvelteKit through the same public HTTP
  endpoint with the correct forwarded origin.
- The existing production UI login journey succeeds, reaches an authenticated
  tenant page, and does not expose an API key, access token, refresh token, or
  session credential in browser-visible content.
- A signed-out request to a protected tenant page cannot read protected UI
  content.
- Existing login/session behavior is reused. The task adds no auth protocol,
  tenant semantics, permission semantics, public route family, or required
  deployment setting.
- No new environment variable or test-selection interface is introduced.
- No Bifrost, query, generated HTTP contract, native SDK, or live product-page
  data work is present in the task diff.
- The existing login is visually inspected at representative desktop and
  mobile widths; because no UI design change is in scope, any visual change is
  treated as a regression rather than accepted as part of this task.

## Expected Write Set and Consumer Closure

Likely owners:

- `scripts/build.sh` and the existing `mise.toml` build description/invocation;
- official distro Dockerfiles and existing `docker/official/extras` entrypoint
  and nginx template;
- `scripts/server/test-startup-prod.sh` and its existing startup support;
- UI package/build metadata only if the existing adapter-node production build
  cannot be staged as-is.

Consumer closure includes release image jobs, official-image startup behavior,
one-off `wyrd-server` image commands, health/readiness/API/MCP routing, process
shutdown, forwarded-origin login behavior, and the existing UI login/session
tests. No Wyrd API contract, Bifrost surface, Rust SDK, or live page consumer is
expected to change.

## Verification and Evidence

This is a mixed task. New image routing follows the RED / GREEN / REFACTOR
scenario above. Cross-host image construction is build/configuration proof, and
the already-existing login receives regression proof rather than a manufactured
failure.

Run the existing environment-owning lane without a new test selector:

```bash
mise run test:server:startup:prod
```

Exercise login through the existing environment-owning identity lane, without
adding a selector or a second identity environment:

```bash
mise run test:identity:journey
```

Run the current UI static/build checks:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
```

Run the same canonical image build on one supported macOS host and one
supported Linux host and record the host OS, image ID, and successful startup:

```bash
mise run docker:build
```

The startup lane remains the owner of the official image and public routes; the
identity lane remains the owner of UI login. Both run unchanged rather than
gaining test-only filters. Do not add `codegen:check` or Bifrost lanes.

## Material Stop Conditions

- Packaging the existing UI requires a new public auth contract, tenant or
  permission decision, public listener, or required deployment setting.
- Supporting both build hosts would require a repository-wide
  cross-compilation framework rather than a target-native image build.
- The existing login cannot work through the official route boundary without
  changing its externally observable auth behavior.
- Implementation begins onboarding Bifrost, generated HTTP contracts, or live
  page data to make the plumbing test pass.

## Authority Links

- [Approved specification, revision 1](../spec.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../AGENTS.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Protocol doctrine](../../../../architecture/wyrd-design.md)
- [Spec-driven development](../../../../changes/spec-driven-development.md)
- [Implementation execution](../../../../changes/implementation-execution.md)
- [Testing workflows](../../../../TESTING.md)
