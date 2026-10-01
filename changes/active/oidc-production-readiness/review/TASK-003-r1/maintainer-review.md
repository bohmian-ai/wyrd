# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `4e0ca8d2ecc2940724861cf6884b68f9adf65464`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 5 at `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`

The candidate remained checked out at the stated commit during this review. I reviewed the complete base-to-candidate diff and did not modify reviewed source.

## Authorities Read

- `AGENTS.md`, especially ownership, struct-centered Rust, domain types, documentation, UI/client projection, and test-tier rules
- `architecture/agent-rules.md`, especially Rust documentation, tenant SQL boundaries, and test placement
- `architecture/references/languages/maintainer-style.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/README.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/languages/typescript-guide.md`
- `architecture/references/languages/testing-workflows.md`
- Applicable identity, tenant, UI, and surface sections of `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and `architecture/wyrd-doctrine.mdx`

## Changed-Surface Coverage

| Surface | Symbols and consumers inspected | Maintainer assessment |
|---|---|---|
| Callback contract and generated declarations | `CallbackQuery`, its deserialization test, both checked-in JSON schemas, and the callback route consumer | The relaxed provider-parameter behavior is documented, focused, and generated schemas match the source. |
| Browser-session auth owner | All of `BrowserSessions`, `CreatedBrowserSession`, `BrowserSessionView`, `BrowserSessionAuthority`, `CurrentSession`, renewal/insert/crypto helpers, `HumanConnections::redeem_completion`, and callers in the BFF handlers and boot | One cohesive stateful owner with discoverable operations. Public and private Rust items are generally substantively documented. Finding `MAINT-002` covers the incomplete tenant projection. |
| SQL persistence owner | Browser-session migration; `BrowserSessionMode`, `SessionLifetime`, write/locked row types and every query; login-completion return change; `WyrdPostgres` tenant resolvers and all callers; migration assertions | Tenant transactions and query ownership are easy to follow. Finding `MAINT-003` covers a dead lifetime shape that complicates the insert contract. |
| Server BFF transport and composition | `BffChannel`, router/middleware, every request/response DTO and handler, `ServerAuth`, route mounting, production boot construction, configuration loading/parser/tests | The private channel is locally typed and handlers are thin projections over `BrowserSessions`. Finding `MAINT-001` covers misplaced rustdoc in the modified configuration module. |
| SvelteKit production-session owner | Every type and method in `server-sessions.ts`; its callers in hooks, root actions, login, completion, layout, settings, existing change actions, and `App.Locals` | `ServerSessions` is a meaningful owner matching the existing `LocalSessions` boundary, not a utility wrapper. Cookie, channel, CSRF, tenant switch, logout, and API-call flows remain findable through that owner. Finding `MAINT-002` covers the fabricated `TenantContext` value. |
| UI routes and components | Root entry/load/actions, tenant login load/actions/page, completion route, tenant layout, settings load/actions/page, `Shell`, `TenantChooser`, and their tests | Route files keep request parsing beside their actions and delegate session behavior to the owner. Settings wire shapes and form names are internally consistent. |
| Existing UI authorization consumer | `WyrdClient::can` and every changed permission check; `CardsInventory` wildcard test | The helper replaces repeated string membership checks at one owner and has focused proof for all wildcard forms. |
| Journey and fixture infrastructure | `identity_ui_e2e.rs`, `production-auth.integration.test.ts`, `WyrdTestServerBuilder::with_bff_service_key` and composition, Vite integration exclusion, and identity mise routing/filter selection | The Rust host owns process lifecycle; the TypeScript browser helper keeps HTTP journey mechanics local. Test names and lane selectors agree. |
| Miscellaneous changed tests and generated files | Session, shell, tenant-routing, card permission, and migration tests; generated callback schemas; task evidence | Tests use caller-observable assertions and declarations are in parity. No new package or dependency was added. |

## Material Findings

### MAINT-001 — Modified configuration helper has another function's rustdoc

- **Location:** `crates/wyrd/wyrd-server/src/config.rs:3505-3514`
- **Governing principle:** `AGENTS.md` required Rust documentation and `architecture/agent-rules.md` require every materially modified Rust item to carry substantive rustdoc describing its actual operation; maintainer style requires documentation to clarify the contract rather than misdescribe it.
- **Evidence:** `/// Read an environment variable, returning None ...` immediately precedes `parse_bff_service_key_hashes`, so it becomes that parser's first rustdoc sentence. The actual `env_opt` function at line 3531 no longer has that documentation. The parser therefore claims two unrelated responsibilities, while the environment helper has none.
- **Concrete maintenance cost:** generated documentation and source navigation lie about which function reads the environment, and a maintainer editing either helper cannot rely on the nearest contract. This is a changed-source documentation defect, not a formatting preference.
- **Smallest testable correction:** move the two-line environment-variable rustdoc directly above `env_opt` and leave only the BFF hash parsing contract above `parse_bff_service_key_hashes`. Run the existing rustdoc/lint gate and the focused parser test.
- **Nearby pattern:** the surrounding configuration helpers each place one operation-specific rustdoc block immediately above their owning function.

### MAINT-002 — Production session projection fabricates a required tenant identifier

- **Location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:215-226`; producer path `crates/wyrd/wyrd-auth/src/browser_sessions.rs:315-356` and `crates/wyrd/wyrd-server/src/components/auth/bff.rs:227-271`
- **Governing principle:** `AGENTS.md` requires domain values instead of raw placeholders and server/client surfaces to project the server-owned contract; maintainer style requires types to describe the real value and invalid states not to be smuggled through a typed boundary.
- **Evidence:** `ServerSessions.context` must return the existing `TenantContext`, whose `Membership.tenantId` is a required string, but it assigns `tenantId: ''`. The Rust session owner already knows the authoritative `DataTenantId` returned by `current`, yet `BrowserSessionView` and the private `ReadResponse` drop it. The task's packet-local `SessionRead` shape explicitly includes `tenant_id`. Existing development owner `LocalSessions::authorize` carries a real `tenantId` and compares it when binding membership.
- **Concrete maintenance cost:** `TenantContext` claims a valid tenant identity while containing a sentinel. Existing consumers such as `WyrdClient` accept that type and use `context.tenant.tenantId` for tenant-keyed storage; their current production guards happen to reject before use, so a normal transport implementation or reordered check can silently operate under an empty tenant key. The type no longer protects a maintainer from mixing routing context with durable tenant identity.
- **Smallest testable correction:** carry the already-known `DataTenantId` through `BrowserSessionView` and the internal BFF read response, add it to the TypeScript `Read`/`ServerSession` projection, and populate `TenantContext.tenant.tenantId` with that value. Keep it out of browser page metadata. Add a focused session test asserting that `context(readSession).tenant.tenantId` equals the server-returned tenant id and the existing journey leak check still proves it is absent from page data.
- **Nearby pattern:** `LocalSessions::authorize` in `src/lib/server/auth/session.ts:126-137` returns a `TenantContext` containing the actual membership tenant id; `platform_sessions.rs` likewise carries `PrincipalId` rather than placeholders across its session view.

### MAINT-003 — `SessionLifetime::Until` is a zero-caller capability that complicates the storage contract

- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:126-134, 228-252`; callers `crates/wyrd/wyrd-auth/src/browser_sessions.rs:230-243, 292-305`
- **Governing principle:** maintainer style requires types and branches to represent real workflows; `AGENTS.md` abstraction rules prefer the concrete shape when there is one implementation. The Ponytail ladder rejects configuration and branches with no current caller.
- **Evidence:** repository-wide caller inspection finds no construction of `SessionLifetime::Until`. Both SSO and API-key session creation use `SessionLifetime::For`. The unused variant alone requires the insert query's two-way `COALESCE($13, statement_timestamp() + ($14 ...))`, two optional bind values, and documentation claiming an SSO workflow that the owner does not use.
- **Concrete maintenance cost:** maintainers must reason about two lifetime authorities and nullable parameter combinations even though only one can occur. The enum and SQL comments also suggest that changing the refresh expiry changes absolute session expiry, while the actual SSO owner applies its fixed 12-hour duration.
- **Smallest testable correction:** delete `SessionLifetime::Until`, make the write shape carry the one used fixed duration, and reduce the insert to the single PostgreSQL-clock duration expression. Preserve the separate producer-issued refresh expiry column. Existing SSO and API-key journeys plus the SQL integration lane prove the remaining path.
- **Nearby pattern:** the same query module already uses closed `BrowserSessionMode` variants only for the two modes that are actually constructed.

## Uncertain Preferences Kept Out of Findings

- `ServerSessions` is larger than the development `LocalSessions`, but its methods share one cookie/channel/session invariant and all callers discover the workflow through that owner. Splitting transport, cookies, and session actions now would add indirection without a demonstrated second owner.
- The tenant-slug pattern appears in the root route, `ServerSessions`, and the HTML `pattern` attribute. One copy is native browser validation and the server-side checks defend different trust boundaries; the small duplication has no demonstrated maintenance failure in this task.
- `wyrd.ts:256-263` contains two adjacent versions of the `observeAccess` doc comment. Deleting the stale shorter block would be cleaner, but it does not hide or alter the method contract enough to be a material acceptance finding.

## Verification Assessment

The task records successful focused UI selectors, both filtered and unfiltered identity journeys, the complete UI test/check commands, `fmt`, `lints`, `codegen:check`, `test:wyrd`, a focused callback test, and `git diff --check`. Per assignment, I did not run Cargo-backed lanes. The named test and lane wiring exist and match the recorded selectors. Those green results do not close the source-level documentation defect, fabricated typed value, or dead persistence branch above.

## Overall Result

**FAIL**

The changed surfaces and consumers were covered, but `MAINT-001`, `MAINT-002`, and `MAINT-003` are material maintainer defects requiring bounded corrections.
