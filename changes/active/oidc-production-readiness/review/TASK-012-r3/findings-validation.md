# TASK-012 round-3 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Round-1 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Round-2 remediation: `changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`

The candidate remained the named commit before and after validation. This pass
read the complete cumulative diff, the latest documentation-only remediation
diff, both prior verdicts and validated ledgers, and every required round-3
discovery report. It independently inspected the cited source, full owner
bodies, producers, callers, sibling consumers, tests, and recorded proof.

No `followup-review.md` was required: the discovery reports materially agree,
their proposed finding union is empty, and they expose no conflicting claim,
unreviewed reachable path, or common source left unresolved by the two prior
remediations.

## Proposal validation

| Discovery proposal or conclusion | Disposition | Independent source validation |
|---|---|---|
| Behavior and invariant reviewers: no proposed findings; `FIND-TASK-012-1` through `-4` closed | **CONFIRMED** | Device and refresh are reachable only through the installed `oauth2` client; the private form owner has only RFC 8693 and RFC 7523 grant producers plus the separately locked RFC 7009 revocation call. `HttpConfig::validate` produces the one normalized origin consumed by the token client, authenticated HTTP client, and saved-login key. The R2 documentation and schema descriptions match those owners, and TASK-012 records all seven exact selectors as one-selected/one-passed. |
| Repository-standards reviewer: no material repository-rule finding | **CONFIRMED** | The cumulative change keeps shared OAuth behavior in `wyrd-client`, retains cohesive dependency-owning structs, adds no language-specific token implementation or Cargo feature, keeps async at IO boundaries, preserves typed/redacted secrets and stable error projection, and contains substantive rustdoc for every new or materially modified Rust item. Generated schema copies and goldens agree. |
| Maintainer reviewer: no material finding | **CONFIRMED** | `AuthHttp`, `TokenExchange`, `AuthMiddleware`, `HttpTransport`, `SavedLogins`, and `LoginFlow` remain the discoverable owners of their workflows. The removed broad exchange entry point is not replaced by another abstraction; the retained conversion and error helpers are narrow and stateless. No placement, naming, structure, or wording preference has a behavioral, security, tenancy, durability, or public-contract consequence. |
| System-resilience reviewer: no material finding | **CONFIRMED** | Redirect refusal, finite request deadlines, device cancellation, refresh uncertain completion, saved-login locking and atomic replacement, pending-mint single-flight, browser fallback, and local-first best-effort logout retain bounded failure and restart behavior. No retry journal, backup token, lease, recovery service, or availability mechanism was introduced. |
| Security reviewer: no material finding | **CONFIRMED** | Every secret-bearing OAuth request uses the same redirect-disabled client and validated, userinfo-free origin. OAuth refusal and diagnostic paths disclose no submitted credential or raw URL spelling. The private form path has no device or refresh producer. The RFC 7009 exception remains exactly the lead-approved single form POST. |
| Durability reviewer: no material finding | **CONFIRMED** | `SavedLogins::renew` retains the configuration-directory lock across reread, refresh, and atomic persistence; waiting processes reuse the saved successor. `remove` shares that lock, preventing a late renewal from recreating a deleted record. Crash-after-rotation behavior remains the explicitly approved login-again consequence rather than motivating new state. |
| Tenancy reviewer: no material finding | **CONFIRMED** | Saved-login selection first scopes by canonical origin, then selects the named tenant or newest same-origin record. Explicit self-identifying credentials cannot be retargeted by the tenant selector. Every first-class SDK reaches the same Rust resolver and renewal owner, while the server remains the authority for tenant and permission identity. |
| Platform reviewer: no material finding; `PLAT-001` remains rejected | **CONFIRMED** | Wyrd calls `webbrowser::open` directly, keeps the printed URL and `--no-browser`, and has no per-platform launcher, shell-escaping layer, browser-command option, WSL branch, or alternate dependency. The accepted `cargo check -p webbrowser --target x86_64-pc-windows-msvc` is the locked Windows proof and was not reopened. |

The independent validation found no unique discovery claim to retain, revise,
or reject. Agreement was treated only as corroboration; the conclusion rests
on the source traces below.

## Producer, caller, and sibling-consumer trace

### Grant ownership and redirect boundary

`TokenExchange::new` validates the supplied deployment URL once, retains the
resulting `HttpsOrigin`, constructs the RFC 8628 and RFC 6749 endpoints from
that origin, and builds one `reqwest` client with
`reqwest::redirect::Policy::none`. `device_authorization`,
`device_access_token`, and `refresh` use the `oauth2::BasicClient` through
`AuthHttp`.

The former public `TokenExchange::exchange(&TokenRequest)` is absent. The
private `grant` method is called in production only by `platform_session` and
`AuthMiddleware::post_token_request`. Their complete producer set constructs
RFC 8693 `TokenExchange` or RFC 7523 `JwtBearer` requests. Repository search
found no `TokenRequest::DeviceCode` or `TokenRequest::RefreshToken` producer in
the changed client, CLI, or testing surfaces. Device and refresh sibling
callers in CLI login, CLI refresh, saved-login renewal, and the human-login
harness use the typed `oauth2` methods.

`revoke_refresh_token` is a separate typed operation that creates exactly the
RFC 7009 `token` and `token_type_hint=refresh_token` form and sends it once
through the same redirect-free client. This is the locked conventional
exception for loopback HTTP and is not a surviving broad grant escape hatch.

### Deployment identity and authenticated consumers

`HttpConfig::validate` parses through `reqwest::Url` and delegates origin
normalization and userinfo refusal to `HttpsOrigin::of_url`. It rejects remote
cleartext and unsupported targets without repeating the submitted URL. Its
returned origin is consumed directly by `TokenExchange::new`,
`HttpTransport::new`, and `canonical_origin`; paths, queries, fragments, case,
and default-port variants therefore do not create sibling destination or
saved-login identities.

`HttpTransport::authenticated_url` compares absolute targets through the same
`HttpsOrigin` authority before attaching credentials. `TokenExchange`,
`AuthMiddleware`, and `HttpTransport` debug implementations expose only the
normalized origin and omit the raw spelling and client state. Direct CLI,
platform, explicit-credential, saved-login, and ordinary SDK construction all
converge on these shared owners rather than adding downstream target guards.

### Persistence, concurrency, and recovery consumers

`ClientConfig::resolve_credential` preserves explicit and environment
credential precedence, selects saved logins only under the canonical origin,
and refuses an unmatched named tenant. `SavedLoginSource` retains that origin
and tenant key. `SavedLogins::renew` acquires the stable directory lock,
rereads, reuses a successor already written by another process, or calls the
`oauth2` refresh method while retaining the lock; it installs the rotated pair
together before the existing atomic file replacement.

`AuthMiddleware` keeps its existing cache and pending-mint single-flight owner.
CLI logout removes the selected local record under the same lock before the
best-effort RFC 7009 call. These sibling consumers preserve the approved crash,
cancellation, restart, concurrent-renewal, and logout ordering without a
second store or recovery protocol.

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-012-1` | The public arbitrary-form exchange is deleted. The complete production caller set of private `grant` constructs only RFC 8693 and RFC 7523 requests; device and refresh use the installed `oauth2` methods. The CLI journey no longer drives a raw device form. | **CLOSED** |
| `FIND-TASK-012-2` | `HttpConfig::validate` returns the existing normalized, userinfo-free `HttpsOrigin`. `TokenExchange`, `HttpTransport`, authenticated absolute-URL checks, and saved-login identity consume that authority; diagnostics expose only that origin. | **CLOSED** |
| `FIND-TASK-012-3` | R1 supplied the adapter, method, field, associated-type, helper, cancellation, partial-progress, retry, and idempotency contracts. R2 supplies the remaining `Debug::fmt` diagnostic contracts, `origin` helper `# Errors`, three changed tests' `# Panics`, accurate public root-origin prose, and accurate device/refresh versus retained-form ownership wording. The generated schemas project the corrected public descriptions. | **CLOSED** |
| `FIND-TASK-012-4` | TASK-012 records exact `mise exec -- cargo nextest` commands for the six named library tests and the setup-wrapped Postgres integration test; each is recorded as selecting and passing exactly one test. | **CLOSED** |

## Ponytail validation

The delete/reuse/standard/native/installed/minimum ladder leaves the candidate
unchanged:

1. The broad custom device/refresh path was deleted; deleting the remaining
   private RFC 8693/RFC 7523 path would remove required behavior.
2. Existing `HttpConfig::validate`, `HttpsOrigin`, `SavedLogins`,
   `AuthMiddleware`, and `HttpTransport` owners are reused rather than
   duplicated.
3. Standard OAuth 2.0 behavior is supplied by `oauth2` for device and refresh;
   the retained standardized forms are only those not modeled by the library
   plus the locked RFC 7009 loopback-compatible request.
4. Native file locking and atomic replacement remain the persistence
   mechanisms; no new coordination layer exists.
5. The installed `webbrowser` dependency owns platform launch; Wyrd adds no
   shell or platform abstraction.
6. No correction is necessary. Additional validation, compatibility behavior,
   retry state, launcher logic, check, option, or test harness would be
   unearned complexity outside the approved task.

## Final deduplicated finding ledger

**Empty.** Every discovery report proposed no material finding, independent
source validation confirms that union, and `FIND-TASK-012-1` through
`FIND-TASK-012-4` are closed. No `FIND-TASK-012-5` is assigned.

`PLAT-001` remains rejected under the locked lead decision and is not converted
into a finding, verification limit, or optional remediation.

## Verification assessment

The cumulative task record contains the focused CLI, Rust, Python, TypeScript,
concurrent-renewal, and workload journey evidence, owner-level test lanes, and
all seven exact Rust selectors. R1 records the exact-selector closure. R2
records formatting, focused all-target/all-feature Clippy, `cargo doc`, four
schema-drift tests, and `git diff --check` for its documentation/schema write
set. Round-3 domain reviewers additionally recorded focused auth, origin,
tenancy, and Windows dependency checks.

This validation reran only `git diff --check` and performed source and range
inspection. It did not run or require a full journey suite, language sweep,
`test:shared`, `test:rust`, `gate`, or another aggregate, in accordance with
the human direction. The narrow recorded proof directly covers the cumulative
behavior and the R2 documentation-only write set; no missing verification
prevents a conclusion.

## Validation outcome

**PASS.** The independently validated final ledger is empty, all four prior
findings are closed with current-source evidence, the required reports are
complete, and the candidate preserves the locked RFC 7009, Windows proof, and
`PLAT-001` decisions without invented complexity.
