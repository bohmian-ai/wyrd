# Repository standards review

Overall result: **FAIL**. Proposed findings REP-1–REP-3 require independent validation.

Subject: `/home/thorrester/Documents/GitHub/wyrd`; base `e0354f9bdf8608046833029252d8a526e0bd999d`; candidate `5c560663e77ae07c33d198862ba6d795b656097a`. Reviewed the cumulative `git diff base...candidate`, including Observe work. This is the first full review; no acceptance was carried forward. HEAD remained the candidate at the final source inspection. Only this report was written.

Scope is repository standards across the complete changed surface, not a task-acceptance or abstraction-preference review. CodeGraph exploration preceded source location and inspection. The root and every applicable nested AGENTS file were read independently; the inventory contains only the root, SQL, Python, TypeScript and docs AGENTS files. The approved revision-19 spec and original task establish context, but implementation completion claims are not verification.

## Authority and surface coverage

Every row also applies root `AGENTS.md`, `changes/spec-driven-development.md`, and `agent_docs/maintainer-style.md`.

| Changed surface | Additional authority read | Source and consumer coverage |
|---|---|---|
| Pure auth contracts, error variants, runtime permissions | `agent_docs/{rust,errors,agent-surfaces}.md`; architecture README; `wyrd-design.md` Client model and Human sign-in; security API keys, browser sessions, permission and tenant binding sections | `wyrd-spec/src/auth/{browser,tenant_principals,mod}.rs`, `error.rs`, runtime `permission.rs`; schema roots, HTTP handlers, MCP descriptors and SDK projections |
| Auth domain, local password policy and session provenance | Rust/error guides; security credential/session, cryptography and audit sections | Entire new `passwords.rs`, changed `BrowserSessions`, `TenantGrant`/`TenantTokenIssuer`, existing shared issuance/permission resolution and their callers |
| SQL migration and queries | SQL AGENTS; security Postgres tenant binding and credential lifecycle | Entire new migration and `local_passwords.rs`; changes in `users`, `service_accounts`, `api_keys`, `browser_sessions`, auth re-exports; production signatures and fields, transaction callers, migration sentinel test |
| Server HTTP, middleware, MCP | Rust/error/agent-surface guides; security authorization, sessions and audit | New account operations; changed principal routes, password handlers/router, recent-login extension, credential mapper, error mapper, MCP registration/dispatch; protected edge and request-id middleware checked for rejection conversion |
| Operator setup, test harness, development lifecycle | Rust guide; operations README and deployment/configuration/browser-session/database ownership sections; `TESTING.md` | `main.rs` setup/password/Postgres helpers, `WyrdTestServer` new production-route bootstrap/helper, `dev-server.sh`, changed `dev:full` task and lifecycle browser test |
| Shared Rust client and Rust journeys | Rust/error guides; Client model; testing lane map | Principals handle and re-exports, profile/page methods, new Rust personal-key journey and existing lane selectors |
| Python binding, stubs and journey | Python AGENTS; Rust/error guides; Client model | Principals wrapper, test-server wrapper, authored auth/testing stubs, public-import integration test; existing registered Principals class and package export mechanism retained |
| TypeScript binding, declarations and journey | TS AGENTS; Rust/error/agent-surface guides; Client model | N-API production/testing methods, generated `.d.ts`/`.d.cts`, wrapper exports/methods and integration journey; wire type/error generator roots |
| UI server auth/forms and login/signup/reset/account | UI skill and SvelteKit/testing references; brand DESIGN; architecture session/public credential contracts | Hooks, browser-session forwarding, form/problem owner, all changed route server modules and pages, shared Entry, identity-menu link; transient secret action and ordinary load traced separately |
| Observe rendering/state and proof | UI skill/references; brand DESIGN, palette and component contracts | Chart, TraceWaterfall, plot palette, Observe host/layout/page, URL points state and server loading, component/unit/browser tests |
| Generated assets, docs and tooling | Docs AGENTS (only generated style tokens touched); agent-surface guide; UI palette generation contract | Schema example generator, TS generator, problem projections, error codes, theme projections in UI/docs/skill, palette generator assertions, Cargo manifests/lock, architecture amendments |

## Rule results

PASS below is source compliance in this audit; it does not claim a newly executed build or test.

| Applicable rule | Evidence and result |
|---|---|
| Contracts stay in pure `wyrd-spec`; Rust owns durable behavior | PASS. New requests/profile/pagination/error catalog live in spec; hashing, sessions, grants, code redemption and credential storage remain Rust/server owned. No IO/async/PyO3/heavy data dependency was added to spec. |
| Required tier boundaries and narrow dependencies | PASS. Argon2 and the declared Unicode normalizer are added only to the auth owner; no client SQL/cloud/data engine dependency, owner Python feature, wildcard version, per-crate profile or new runtime was added. |
| Stateful capability has its owning struct; free work can be caller-owned | PASS. Session lifecycle extends `BrowserSessions`, issuance extends `TenantTokenIssuer`, SDK calls extend `Principals`, BFF forwarding extends `BrowserSessions`. Account route helpers and SQL functions operate on caller-owned request/transaction state. No new single-implementation trait or utility struct. |
| Actual SQL connection capability types | PASS. Production additions in password auth, account HTTP, setup and query functions use `WyrdPostgres`, `TenantConn` or `OperatorPool`; no added production `PgPool`, bare connection or caller-supplied SQLx transaction signature/field. This was checked directly, not inferred from tenant-isolation tooling. |
| Caller transaction ownership; RLS and tenant-qualified relationships | PASS. Query functions borrow `TenantConn`; workflows own commit. New tables enable and force RLS with tenant `USING`/`WITH CHECK` and restricted operator policies. User-key ownership uses tenant-qualified foreign keys and exactly-one owner checks. No new general RLS bypass or manual tenant filter was introduced. |
| PostgreSQL coordination time and forward migration | PASS. Invitation/reset/session expiry and password windows derive from `statement_timestamp()`; new migration is appended, not an edited old migration. Existing-key backfill preserves identity and verifier columns. |
| Stable derive-backed public errors and typed handlers | FAIL: REP-3. Error variants themselves have derive metadata and generated projections, and principal handlers normalize extraction rejection. New password handlers bypass that boundary on invalid JSON. |
| Instrumentation and scrubbing | PASS. Password handlers use `skip_all`; new account handlers skip caller/state/request appropriately. Secret request fields use `SecretBearer`, password work uses `SecretString`, and BFF errors normalize through safe problems. No raw secret was printed or recorded by this review. |
| Audit decisions remain nonblocking on the existing process outbox | PASS. Principal account operations reuse `audit::authorize`; successful session creation reuses `stage_login`. No new durable audit service or awaited audit-write dependency was introduced. |
| Changed-item rustdoc, including private/test items | FAIL: REP-2. New and changed production items/fields have intent documentation and fallible methods have error sections; the three new password unit tests omit required panic documentation despite explicit assertions/unwraps. |
| No gate circumvention or unconditional local allow | FAIL: REP-1. One new unconditional `#[allow]` was added. No existing test was removed/ignored to clear a failure. New ignored Rust/MCP journeys match the documented journey split and are included by the existing `--run-ignored=all` lanes. |
| No sleep-based test synchronization or weakened proof | PASS. New UI tests use condition polling/events. Existing development readiness polling remains lifecycle polling. Metrics lifecycle assertion now explicitly opens its reader-controlled recorded-points inspector, preserving a positive real-row assertion; the task records the stale-assertion diagnosis. Palette tests add roles/pairs without reducing an existing contrast minimum. |
| Cross-language projection parity and foreign runtime boundaries | PASS. Shared handle methods are projected by thin PyO3 and N-API methods, authored Python stubs, generated native declarations and typed TS wrapper methods. New tests import public clients and exercise real server metadata/revoke paths. No Python/Node lifetime behavior was moved into a Rust emulation. |
| Generated artifact ownership | PASS in source. Palette/generator roots and error/example sources accompany generated projections; new profile/page wire roots are added in both schema and TS generators. No new committed runtime JSON Schema. Full regenerated equality remains an orchestrator check. |
| Agent-first HTTP/MCP/public shape | PASS except REP-3. Profile is typed, read-only MCP discovery is added unscoped, metadata pagination stays on the existing route/tool, and no secret issuance MCP tool was added. Mutations retain the existing configured permission/scope machinery. |
| UI server/client split and tenant routing | PASS. The BFF forwards to Rust owners, derives personal target identity from session/profile, keeps page loading server-side, and exposes a created key only in the approved transient action response. No Node durable password/grant/session service was introduced. |
| UI tokens, shared components, themes and accessibility | PASS for material changes inspected. Observe consumes palette-generated tokens and existing Drawer/Chart/RecordTable/FacetRail; status retains words/glyphs, interactive forms have labels and password autocomplete, chart keyboard interactions and series noncolor channels remain. Existing copied Entry geometry is not reported as a new supported-use regression. Browser tests cover both modes and widths in source; this review did not conduct rendered acceptance. |
| Docs and tooling scope | PASS. Docs-site change is a generated token projection, with no new authored guide to evaluate. Architecture additions use Wyrd owners/nouns. Tooling retains configured identity and introduces no source compatibility alias, OS-specific behavior or dependency gate widening. |

## Proposed findings

### REP-1 — VIOLATION: unconditional lint suppression on the new SQL insertion

- **Location:** `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:401–404`, `insert_user_api_key`.
- **Rule:** root AGENTS forbids circumventing a gate with `#[allow]`; `agent_docs/rust.md` requires a narrow `#[expect(lint, reason = ...)]` as the last resort and permits `#[allow]` only when a lint fires in some build configurations.
- **Evidence:** the candidate adds unconditional `#[allow(clippy::too_many_arguments, reason = ...)]` above a nine-argument SQL insert. Its explanation concerns table shape, not configuration-dependent lint behavior.
- **Consequence/materiality:** this bypasses an explicit repository lint gate and cannot detect when the allowance becomes unused. A green Clippy invocation cannot prove compliance while the rule is suppressed.
- **Smallest correction:** remove the suppression by using the existing owner pattern if readily applicable; otherwise use the permitted narrow `#[expect(..., reason = ...)]` with the existing justification. No credential or SQL behavior change is necessary.
- **Closure proof:** inspect the final attribute and run the owning lint lane; do not weaken workspace lint policy.

### REP-2 — VIOLATION: new password unit tests miss the mandatory panic contract

- **Locations:** `crates/wyrd/wyrd-auth/src/passwords.rs:785`, `:794`, `:816`: `usernames_canonicalize_or_refuse`, `new_passwords_follow_the_policy`, `verifiers_match_only_their_password`.
- **Rule:** root AGENTS requires rustdoc for every new/changed Rust item and `# Panics` when it can panic, explicitly blocking merge for missing rustdoc. The Rust guide explicitly includes private helpers and tests.
- **Evidence:** each test has a brief intent sentence but no `# Panics`; they contain `assert!`/`assert_eq!` and `unwrap` on tested results. The new integration tests in `pg_browser_sessions.rs` already demonstrate the compliant pattern: “Panics when … any expectation fails.”
- **Consequence/materiality:** this is the explicit changed-item documentation merge gate, not an optional prose improvement. Compiler/Clippy success does not cover documentation on private/test functions.
- **Smallest correction:** add concise panic documentation to these three tests in the same remediation as substantive fixes. Preserve all assertions. No new tests or abstractions.
- **Closure proof:** source inspection of these items and the ordinary format/lint checks. Do not create a separate documentation-only review round.

### REP-3 — INCORRECT: password request rejection escapes the public Wyrd error contract

- **Locations:** `crates/wyrd/wyrd-server/src/auth/browser.rs:579`, `:625`, `:675`, `:729` (all four new password handlers).
- **Rule:** root AGENTS server handlers require typed bodies and Wyrd errors; `agent_docs/errors.md` requires the shared public error projection. `wyrd-design.md` Human sign-in limits the OAuth error exception to OAuth endpoints and explicitly excludes browser-session operations.
- **Reachable path:** send a same-origin JSON request to `/auth/browser/password/login` with an unknown field or a required field of the wrong JSON type. The request is rejected by bare `Json<PasswordLoginRequest>` before handler code runs. Signup/change/reset have the same extractor shape.
- **Source proof:** each new contract denies unknown fields, while each handler takes bare `Json<T>`. Local installed Axum 0.8.9 `src/extract/rejection.rs` defines JSON data rejection as `422` with its own body (syntax failure `400`, missing JSON content type `415`). The shared protected edge maps Tower errors/panics, not extractor responses; request-id middleware only injects an instance into an already `application/problem+json` response. The existing `http/error.rs:98–108` documents and implements the required `Result<Json<T>, JsonRejection>` → `body_rejection` conversion, used by the new principal invitation handler.
- **Observable consequence/materiality:** a caller exercising the new closed request contract receives Axum prose instead of `WYRD_SPEC_400_VALIDATION` problem+json, losing the stable code/remediation/schema contract and predictable agent repair path. This is ordinary invalid request behavior at a newly added public trust boundary.
- **Smallest correction:** route JSON extraction failure through the existing `body_rejection` owner for all four password operations. Preserve same-origin checks and scrubbed instrumentation; do not create another error mapper or echo request content.
- **Closure proof:** a focused real-router negative test for malformed JSON and schema-invalid/unknown-field input asserts `application/problem+json`, the stable Wyrd code, and absence of supplied secret content; cover each changed extractor or an appropriately small table of the four routes.

## Verification limits

No Cargo, build, codegen or shared-target command was run by this reviewer, as requested by the orchestrator. Source checks included the full changed-file inventory, cumulative diffs, actual SQL capability signatures, added suppression/ignore/sleep scan, and changed Rust declaration documentation inspection. Available task verification is a claim pending orchestrator evidence; it is not independently rerun here. Rendered board approval remains outside this standards report.

No implementation was edited and no commit/push occurred. Risk remains the three proposed mandatory/public-contract violations until independently validated and closed.
