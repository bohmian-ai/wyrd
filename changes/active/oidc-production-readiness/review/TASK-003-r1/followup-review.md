# Focused follow-up: tenant projection and chooser provenance

## Immutable subject and question

- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Approved authority: `SPEC-oidc-production-readiness` revision 5 at `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`
- Conflict reviewed: whether cookie-derived tenant choices violate the task when switching later revalidates them, and whether the empty production `TenantContext.tenant.tenantId` is a concrete defect or an unreachable placeholder.

The candidate remained the checked-out `HEAD` while this report was prepared. No Cargo command was run.

## Source paths inspected

- Session producer and private wire projection:
  - `crates/wyrd/wyrd-auth/src/browser_sessions.rs:69-86,315-356`
  - `crates/wyrd/wyrd-server/src/components/auth/bff.rs:220-272`
- SvelteKit session owner and request binding:
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:14-34,183-227,261-355`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/hooks.server.ts:10-31`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/app.d.ts:1-17`
- Browser-visible chooser path and switch action:
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/views.ts:12-18`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/+layout.server.ts:5-8`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/components/app/Shell.svelte:80-100`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/components/app/TenantChooser.svelte:1-24`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/routes/+page.server.ts:139-160`
- `TenantContext` consumers:
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/session.ts:8-27,119-137`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/wyrd.ts:43-370`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/changes/actions.ts:7-13,115-239`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/routes/t/[tenantKey]/settings/+page.server.ts:27-142`
- Proof currently present:
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/session.test.ts:83-183`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/components/app/Shell.test.ts:37-57`
  - `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:185-287`
  - `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:189-275`

## Claim resolution

### 1. Cookie-derived chooser entries

`ServerSessions.metadata` enumerates every incoming cookie name with the
`wyrd_session_` prefix, accepts a syntactically valid suffix, and emits it as a
`SessionMetadata.tenants` entry without calling `sessions/read`
(`server-sessions.ts:279-295`). The tenant layout returns that metadata
(`+layout.server.ts:5-8`), `Shell` opens `TenantChooser` when the array has more
than one entry (`Shell.svelte:84-88`), and `TenantChooser` renders each entry as
a selectable tenant button (`TenantChooser.svelte:16-23`). Therefore a request
with one valid current session plus `wyrd_session_victim=<arbitrary>` reaches a
signed-in page whose tenant chooser visibly includes `victim`. The forged
cookie need not be a valid server session to affect that display.

The forged suffix does **not** become effective tenant authority. The current
tenant was already bound by `hooks.server.ts:17-29`, which calls
`ServerSessions.read`; that method accepts the path tenant only when the
private server response returns the same `tenant_key`
(`server-sessions.ts:189-213`). A switch POST is authorized by the current
session and its CSRF token, then calls `read(target)`; an absent, forged,
expired, or cross-tenant target session is cleared/refused and the result is
the target login route (`server-sessions.ts:265-277`). Settings actions obtain
authority separately through `sessions/authority` and the ordinary Wyrd API
token (`server-sessions.ts:298-333`), not through chooser metadata.

These facts make the standards review's narrow authority conclusion true, but
they do not dispose of `BEH-003-02`. The original task separately and
explicitly prohibits "a tenant selector based on untrusted browser data"; it
does not limit that prohibition to selectors that successfully confer
authority. Here the rendered selector is directly based on request-cookie
names. `BEH-003-02` is therefore supported as a reachable task violation, with
the consequence narrowed to attacker-controlled displayed/routing state rather
than cross-tenant authorization. Existing tests prove forged current-session
refusal and server validation after a direct switch, but none loads a valid
current session alongside forged/expired sibling cookie hints and inspects the
rendered choices.

### 2. Empty `TenantContext.tenant.tenantId`

The task's packet-local typed contract requires `SessionRead` to contain
`tenant_id`. The server already has the authoritative `DataTenantId` on every
read: `BrowserSessions::read` receives it from `current` as `tenant`
(`browser_sessions.rs:320`) and uses it to verify the access token. It then
drops that value when constructing `BrowserSessionView`
(`browser_sessions.rs:329-356`). `ReadResponse` likewise has no `tenant_id`
(`bff.rs:227-244`), so the TypeScript `Read` and `ServerSession` cannot project
one (`server-sessions.ts:14-34`). `ServerSessions.context` nevertheless returns
the existing `TenantContext`, whose `Membership.tenantId` is required, by
fabricating `tenantId: ''` (`server-sessions.ts:215-226`). Hooks execute this
projection and construct a `WyrdClient` from it on every authenticated
production tenant request (`hooks.server.ts:17-29`). Thus the invalid value is
reachable; it is not dead code or a test-only shape.

The present candidate does prevent that empty identifier from selecting
tenant authority or reaching the mock tenant-keyed store. In production hooks
always construct `WyrdClient(context, false)`. Its changes, Observe, Cards, and
home paths reject at their existing non-mock boundary before calling any
`mockChanges` operation keyed by `context.tenant.tenantId`
(`wyrd.ts:58-65,137-155,267-271,345-370`). Production connection settings do
not use `WyrdClient` at all; they call `ServerSessions.api`, whose authority is
again resolved from the opaque server session (`settings/+page.server.ts:27-45`).
The layout also exposes only the server-returned tenant key and name. No current
source path was found where `''` causes a cross-tenant read, storage collision,
or browser-visible value.

That reachability limit narrows but does not reject `MAINT-002`: omitting
`tenant_id` fails the task's explicit `SessionRead` typed semantics, and every
authenticated production request constructs a domain value that falsely
claims to contain the required tenant identifier. The concrete defect is the
missing private-channel projection and invalid typed server-side context, not
a currently demonstrated authorization or storage failure. A focused closure
test must assert that a server session read projects the authoritative tenant
ID into `TenantContext`; the existing page-data leak assertion should continue
to show that this server-only ID is not added to browser metadata.

## New proposed findings

None. The traced evidence resolves the conflict by supporting the existing
`BEH-003-02` and `MAINT-002` claims with narrower consequences; it does not
identify another independent defect.

## Result

**RESOLVED**

- `BEH-003-02` remains a reachable violation of the task's explicit selector-provenance prohibition, even though later server validation correctly prevents it from becoming effective authority.
- `MAINT-002` remains a concrete missing-contract/invalid-domain-value defect. The empty value is produced on every authenticated production tenant request, while its currently dangerous storage and authority consumers are safely unreachable in this candidate.
