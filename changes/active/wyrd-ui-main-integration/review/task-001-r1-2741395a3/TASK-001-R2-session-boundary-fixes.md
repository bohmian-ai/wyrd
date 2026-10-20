---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 10
parent_task: TASK-001
remediates: [FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9]
depends_on: []
---

# TASK-001-R2 — Preserve browser-session authority and failure semantics

## Subject and authority

Approved [spec revision 10](../../spec.md), original
[TASK-001](../../tasks/TASK-001-distribution-login.md), retained
[TASK-002](../../tasks/TASK-002-remove-old-ui.md) workflows, and
[TASK-001-R1](../tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md)
remain the cumulative contract. This task implements only the independently
validated corrections in [findings-validation.md](findings-validation.md) and
[verdict.md](verdict.md). Read those reports before editing.

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Original base: `8651b96fd0c049ab5e63b0606c98326af1fc8848`.
Reviewed candidate / R2 starting subject:
`2741395a33d5e1e5ee5d7022916627917254a663`.
Prior reviewed candidate: `843484f6cceba1f48139349f66794966221c5d37`.
The new review is cumulative, not just R1's final commit. Preserve both prior
reviews and the R1 execution evidence. Re-review the next immutable cumulative
candidate against the original task, revision 10, R1 and this R2 task.

FIND-TASK-001-1's shared installer admission correction is closed in source;
do not redesign it. FIND-TASK-001-2 has a written official nginx/provider test
but still lacks successful Linux execution. REQ-033 is the caller-approved
server-owned session architecture, not a demand for confidential Node OAuth,
encrypted credential cookies, new deployment secrets or infrastructure.

## Outcome

The native public browser endpoints receive their session authority, metadata
never turns refused/unavailable current authority into signed-in success,
callback backend failures retain their error classification, and logout cannot
mutate a different tenant's session under a relabeled cookie. The container
journey cleans up on failure, uses the existing fixture transport, and records
real nginx/provider execution when a Linux runner is available. Batch only the
finite mandatory Rust documentation gaps with those behavior fixes.

## Diagnosis and decision-complete corrections

| Finding | Why the candidate falls short | Selected minimal correction and preserved boundary |
|---|---|---|
| FIND-TASK-001-3 — native session-cookie forwarding | `wyrd-ui/src/lib/server/upstream.ts:69–75` keeps only `wyrd_login` for all `/auth/*`. The hook therefore drops tenant cookies from public `/auth/browser/session` and `/logout`. Rust sees no session: enumeration refuses and logout can clear a browser cookie without revoking its durable row. Node's internal calls take a different path, hiding this in the happy-path journey. | Extend the existing configured-upstream forwarder to convey the Wyrd session authority needed by the browser endpoints, reusing existing session-cookie projection where sufficient. Preserve login binding, trusted public-origin/header replacement, method/body/query, separate Set-Cookie, no redirect following, and Rust's enumeration/selection/refusal ownership. Do not forward arbitrary unrelated application cookies, introduce another transport or move auth into Node. |
| FIND-TASK-001-4 — stale metadata after failed/current-missing enumeration | `BrowserSessions.metadata` maps any non-OK result to an empty list and reinserts its earlier selected session; the tenant layout calls it after hook resolution. A 503/401 or 200 containing only a live sibling can therefore render stale signed-in metadata and hide the server's current answer. | At the existing metadata consumer, relay cookies and propagate refusal/unavailability with the existing safe mapping. Build the selected projection only from the currently accepted enumeration; if selected tenant disappeared, follow existing unauthenticated behavior rather than reinsert it or choose a sibling. Keep live per-tenant choices independent and cookies intact on outage. No cache, retry, extra authority query or session endpoint. |
| FIND-TASK-001-5 — callback backend errors become ordinary sign-in refusal | Browser completion preserves a failure in `LoginCompletion::Refused`, but `auth/browser.rs::login_refused` discards its unavailable/server classification when the next tenant directory lookup succeeds, redirecting to `login=failed`. A transient/session-write failure is enough; total database failure is not required. | Preserve existing unavailable/server Wyrd errors at the browser response adapter; retain ordinary invalid identity/state/provider-denial redirects. Preserve login-cookie clearing, spent transaction/replay behavior, fixed redirect targets, audit and device/provider-test siblings. Reuse the error catalog's classification rather than adding a parallel mapping or retrying consumed login state. |
| FIND-TASK-001-6 — failure aborts fixture cleanup | `test-startup-identity.sh` runs a nonzero `(exit "$status")` in its EXIT trap under inherited errexit before calling startup-common cleanup. Failed nextest or interruption can leave app/Postgres, ports, volumes/network/image/temp data and omit failure logs. | Ensure the existing shared cleanup executes unconditionally after best-effort forwarder removal and receives/preserves the original status. Keep its existing no-argument EXIT callers working, failure diagnostics, resource ownership and success=0/failure status. Do not add another lifecycle controller, process manager or deployment wrapper. |
| FIND-TASK-001-7 — prohibited new test-only setting | `WYRD_UI_NGINX_JOURNEY` is a new serialized-fixture environment variable used solely between the container script and its exact Rust test, despite INV-004. The same owner already has `WYRD_UI_JOURNEY` fixture transport. | Use the established fixture transport for the nginx fixture selected by the existing exact test. Remove the extra name/producer/reader; preserve payload, real public origin, image identity and every assertion. No selector flag, compatibility alias or generic fixture/config framework. Source inspection is enough for removal; do not add a permanent name-ban check. |
| FIND-TASK-001-8 — logout tenant label/value mismatch | `auth/browser.rs::logout` selects by cookie label/request tenant; `wyrd-auth::BrowserSessions::logout` routes revocation from the value's embedded tenant. A valid Beta value relabeled as Acme can revoke Beta when the request selects Acme. RLS correctly scopes the wrong selected mutation and cannot fix that earlier mismatch. | Enforce agreement between requested tenant and cookie's authoritative tenant at the lifecycle mutation boundary before revocation, reusing existing tenant resolution. Mismatch must revoke nothing. Preserve idempotent cleanup for absent/invalid/already-revoked sessions, unavailable-store handling and normal session-only logout. Do not require full live principal/connection permission just to log out, add manual tenant SQL predicates or duplicate checks across readers. |
| FIND-TASK-001-9 — finite mandatory rustdoc | Three new secret-redacting Debug methods and two test modules lack required item docs. Existing provider/transaction docs cover much of cancellation, but the browser creation/revocation and HTTP delivery boundary lacks committed-but-undelivered/retry consequences. | Documentation only in the two new browser owner/adapter files. Explain each formatter's output/redaction and inherited formatter error, document the two test modules, and add/link one accurate account of committed state outliving delivery, fresh-session retry and idempotent logout. Credit existing pre-commit/transaction/callback docs; do not duplicate them or add runtime idempotency/retry/reconciliation machinery. |

These corrections stay within approved REQ-004/006/033, INV-002/004/007 and
AC-003/004/012. FIND-TASK-001-8 is a selected-tenant contract defect, not a claim
that a caller can revoke a session value it does not possess. Metadata failure
is not a claim that Rust API writes bypass permission checks. Missing nginx
execution is not evidence that the deployment necessarily fails.

## Existing owners and affected consumers

- Native hook → `forwardAuth` → Rust browser session/logout endpoints; direct
  public requests and existing Node internal session forwarding are distinct.
  Extend `upstream.test.ts` and the existing production browser driver, not a
  replacement gateway or mock IdP.
- Hook selected-session read → tenant layout → `BrowserSessions.metadata`;
  sibling read/logout already preserve refusal. The authoritative enumeration
  remains the existing Rust `session/live_view` owner.
- Existing `AuthorizationCodeExchange` → `LoginCompletion::Refused` → browser
  callback response adapter; retain the error carried by that path rather
  than change provider verification or transaction consumption.
- Browser logout HTTP adapter → `wyrd-auth::BrowserSessions::logout` → existing
  tenant session revoke query. Recovery and metadata already demonstrate
  route/value tenant binding. SQL remains caller-transaction-owned and RLS.
- `test-startup-identity.sh` and `startup-common.sh` already own the application
  fixture; `with-identity-providers.sh` owns providers, not app cleanup. Reuse
  both lifetimes without merging them into a new controller.
- Mandatory docs are finite: auth `browser_sessions.rs` Debug methods/tests,
  `begin`, `recover` and `logout` owner semantics; server `auth/browser.rs`
  tests and HTTP creation/logout delivery account. Existing linked methods
  may share the account rather than repeat it.

## Ordered behavioral proof

Work RED → GREEN → REFACTOR for one behavior at a time. Preserve all previously
passing relevant cases. Record actual selected/running tests and exact commands;
missing historical RED logs are not permission to manufacture evidence or
weaken a check. New titles below are **planned additions**, not existing tests.

### 1. Native public metadata and logout retain session authority

**Behavior.** Sign in at the real native UI origin, read its public
`/auth/browser/session`, then logout at that origin and prove the saved cookie
is durably refused. Multiple-tenant enumeration and selected logout preserve
other sessions. Unrelated cookies remain unforwarded.

**RED.** Extend the existing forwarding unit owner and production native
journey. The current forwarder must fail because it drops session cookies,
not because fixtures are missing. Run:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/upstream.test.ts
mise run test:identity:ui
```

The focused leaf owns managed provider setup and invokes the existing exact
Rust journey; when naming it in execution evidence also run/record its inner
command within that same provider lifecycle:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'
```

**GREEN.** Correct only the existing forwarding projection and prove the
actual Rust endpoint receives and revokes the right session authority.

**REFACTOR.** Reuse existing cookie projection/error/redirect handling; no
second auth path, session owner or generic cookie-forwarding framework.

### 2. Metadata requires the current accepted enumeration

**Behavior.** An initial valid selected read followed by enumeration 503,
401, or 200 containing only a sibling must not return stale selected metadata.
Valid live tenant choices remain independent; outage preserves cookies and
invalid-session deletion is still relayed.

**RED.** Add a **planned** focused case titled
`metadata requires current live enumeration` in `browser-sessions.test.ts` and
exercise those three answers using its existing fetch/cookie fixture:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts -t '^metadata requires current live enumeration'
```

**GREEN.** Preserve the authoritative failure or require the selected live
view before constructing page metadata; existing safe refusal handles errors.

**REFACTOR.** Remove the stale success fallback without another cache,
authority read or server change. Use the existing route/layout test seam to
show refusal reaches the page if unit projection alone does not establish it.

### 3. Callback unavailable remains unavailable

**Behavior.** A browser completion carrying an existing unavailable/server
error after spent state, while a later directory read could succeed, returns
the existing appropriate error rather than a credentials-refused redirect.
Ordinary provider/identity refusals keep their current redirect behavior;
transaction cookie clearing and sibling login flows remain correct.

**RED.** Add a **planned** `callback_failures_preserve_unavailable_classification` case to the
existing `auth::browser::pg_tests` response-adapter owner, with healthy tenant
lookup and explicit `AuthVerifyUnavailable`, `DiscoveryUnavailable` and genuine
invalid-authority completion cases. Require no-store, no session cookie and
transaction-cookie clearing on unavailable responses. Run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --lib -E "test(=auth::browser::pg_tests::callback_failures_preserve_unavailable_classification)"'
```

**GREEN.** Correct classification at the response adapter, preserving the
error catalog and transaction/cookie/redirect behavior.

**REFACTOR.** Reuse generated error classification, not another error profile,
provider verifier, login retry or Node error authority.

### 4. Logout cannot mutate another tenant under a relabeled cookie

**Behavior.** With independent Acme and Beta sessions, send Beta's value under
the Acme label and select Acme logout. Neither durable session may be revoked
by that mismatch. Ordinary Acme logout still revokes only Acme; keys and Beta
remain usable. Absent/already-dead values retain idempotent cleanup.

**RED.** Extend the registered real HTTP/Postgres authority test:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable)"'
```

**GREEN.** Bind selected-route tenancy before the existing lifecycle owner
commits revocation. Preserve correct SQL RLS/digest behavior and unavailable
versus invalid handling.

**REFACTOR.** Keep one mutation authority; no repeated SQL filters, full grant
reauthorization solely for logout or changes to bearer revocation semantics.

### 5. Failed container proof always settles owned resources

**Behavior.** The existing EXIT handler executes both forwarder removal and
common cleanup on success, deliberate child failure and interruption, preserving
the original exit status and failure diagnostics. No app/Postgres/volume/network
resource is left behind by an early failing command.

**RED.** Execute a small assert-based shell probe of the EXIT handler with
stubbed external cleanup commands for failure status 7 and success 0. Confirm
its current errexit abort before common cleanup. Record the exact probe command
and observed status; no live Docker/image build is needed for this failure path.

**GREEN.** Correct the existing trap/cleanup invocation so status preservation
cannot prevent unconditional cleanup.

**REFACTOR.** Reuse the common cleanup owner, rather than a new fixture
controller or copied resource cleanup. Run shell syntax checks below.

## Static corrections, nginx closure and verification

FIND-TASK-001-7 and -9 do not need manufactured RED: remove the extra fixture
setting by reusing established transport, and document only the finite items/
consequences above. Verify source references and Rust documentation against
current AGENTS §16; do not add a permanent grep/name-ban check or a style-only
follow-up task. The formatter contract is inherited; its item docs need only
explain the selected redaction/output and propagated formatter errors.

After those fixes, execute the existing **focused** real-image/provider leaf on
a supported Linux runner:

```bash
mise run test:identity:ui:container
```

Its existing exact Rust selector, run within the same provider/image lifecycle,
is:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_nginx_journey)'
```

Record the immutable image/source identity, public origin, real provider return,
session/API use, refusal/logout and written restart assertions. This closes
FIND-TASK-001-2 only with actual execution; a registered ignored test or the
macOS image-build refusal is not PASS. Keep existing API-key/device/stream/
child-cleanup siblings. Do not copy provisioning, add selector variables,
bypass nginx or repeat the full identity suite.

Run checks for this write set:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/browser-sessions.test.ts src/lib/server/upstream.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- bash -n scripts/server/test-startup-identity.sh scripts/server/startup-common.sh scripts/identity/with-identity-providers.sh
mise run fmt
mise run lints
mise run check:tenant-isolation
```

Use the existing managed identity/Postgres lifecycle for focused tests. Confirm
planned Rust names in `mise exec -- cargo nextest list`; if a source-backed
owner/name correction is necessary, record its exact equivalent selection.
Run Cargo-backed verification sequentially. No new public shape/schema or
SQL migration is expected; if an actual necessary edit affects generated or
served contracts, run that owner's codegen/OpenAPI checks rather than hand-edit
outputs. A new material contract is not an implementation convenience.

Inspect the final tracked/untracked diff and run `git diff --check`. Do not run
`mise run gate`, the full identity/user-journey suites or every language/platform
matrix for this remediation. Native signed/source-free bundle/digest evidence,
the full Debian amd64/arm64 + Rocky amd64 runtime matrix and human native/nginx
light/dark/mobile/focus inspection remain integrated acceptance gates. Do not
claim those from this focused leaf or Mac fixture tests. If Linux/human proof
is unavailable, finish all independent fixes/checks and explicitly retain the
unverified gate; do not mark task acceptance PASS.

## Acceptance and scope

| Finding | Focused closure |
|---|---|
| FIND-TASK-001-3 | Native public metadata succeeds with the real session cookie; public logout durably revokes it; unrelated cookies/other tenants remain protected |
| FIND-TASK-001-4 | 503/401/missing selected view cannot produce signed-in metadata; accepted independent live metadata and cookie/error behavior preserved |
| FIND-TASK-001-5 | Backend callback errors retain existing unavailable/server response; normal denial/correlation/clearing and CLI/provider-test behavior preserved |
| FIND-TASK-001-6 | Original failure status survives and all existing fixture cleanup/diagnostics run on both failing and successful exit |
| FIND-TASK-001-7 | No additional nginx-only fixture setting; existing exact test receives the same fixture through its established transport |
| FIND-TASK-001-8 | Relabeled cross-tenant value revokes neither independent session; selected normal logout, idempotency, store errors and key survival preserved |
| FIND-TASK-001-9 | Finite missing item/redaction/formatter/module docs and committed-but-undelivered/retry consequences documented using existing owner links, without runtime machinery |
| FIND-TASK-001-2 | Focused real-provider nginx journey actually passes on supported Linux with immutable image/source/public-origin evidence |

Expected write set: existing UI upstream/browser-session tests and production
browser driver, existing Rust browser callback/logout adapter and auth session
mutation owner/tests, existing container identity script/shared cleanup and
fixture transport reader, and finite docs in those Rust files. Paths guide
owner discovery, not helper layout. No installer rewrite, session-store redesign,
new secret/env/service/dependency/feature, broad OAuth/permission/audit change,
new UI component/domain client, compatibility route, weakened test, arbitrary
cookie forwarding or global cookie authentication is authorized.

Use `$wyrd-implement`; add the repo `wyrd-ui` skill for UI edits. Preserve all
uncommitted trace work. No commit, merge, push or deployment is implied.
If correctness requires a new public/persistent/security/tenancy/concurrency
or resource-ownership decision, stop with the exact conflict rather than choose
another architecture. The listed behavioral corrections themselves are already
within revision 10; private helpers/fixtures remain implementation-owned.

## Implementation evidence

Commits on `wyrd-ui-main-integration` after `2741395a3`: `cb20314a6` (FIND-3/4),
`71d210047` (FIND-5), `d3b9a71b9` (FIND-8), `33077ce74` (FIND-9), `4b3241b70`
(FIND-6/7), `1fa1b63cb` (FIND-3 journey), `6e9789fff`/`c082ae1e7` (fmt/clippy),
and `0340de10f` (activation expectation, diagnosis below). Preserved trace
edits in `crates/wyrd-spec/src/vala/trace/` remain uncommitted and untouched.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-3 | `wyrd-ui/src/lib/server/upstream.ts::forwardAuth` forwards `wyrd_login` on every `/auth/*` and `wyrd_session_*` only on `/auth/browser/*`; other cookies dropped | RED: new `public browser-session operations carry the Wyrd session cookies and nothing else` received `wyrd_login=abc`; GREEN `vitest run src/lib/server/upstream.test.ts` 9/9. Journey: `production multi-provider tenant switch` drives public `GET /auth/browser/session` (all, filtered) and `POST /auth/browser/logout` at the native origin; saved value then 401, sibling live — `mise run test:identity:ui` PASS (4/4 vitest, nextest `identity_ui_e2e::production_ui_bff_journey` 1 passed) | PASS |
| FIND-TASK-001-4 | `BrowserSessions.metadata` relays cookies, `refuse`s non-OK, rejects `unauthenticated` when the selected tenant is absent, and builds subject/expiry/tenants only from the current live set | RED: `-t '^metadata requires current live enumeration'` returned stale expiry; GREEN 1 passed (200 current, 503→502 upstream with cookies kept, 401 cleared, 200-sibling-only→401 with selected cleared and sibling kept) | PASS |
| FIND-TASK-001-5 | `auth/browser.rs::login_refused` answers any `5xx` catalog error as itself (`no-store`, login cookie cleared, no session cookie) before the directory read; refusals keep the `login=failed` redirect | RED: `auth::browser::pg_tests::callback_failures_preserve_unavailable_classification` got 303 for `WYRD_AUTH_503_VERIFY_UNAVAILABLE`; GREEN 1 passed (Verify/Discovery unavailable 503 + code; InvalidToken 303) | PASS |
| FIND-TASK-001-6 | `test-startup-identity.sh` trap: best-effort forwarder removal `\|\| true`, then `cleanup "$status"`; `startup-common.sh::cleanup` takes optional status (`${1:-$?}`), no-arg EXIT callers unchanged | Probe (scratchpad `exit-probe.sh`: stub `docker`, real trap line + real common prefix): RED exit 7 → no common cleanup, failing forwarder removal → status 1; GREEN all four cases (0, 7, 7+rm-fail, 0+rm-fail) keep status and run forwarder removal, common cleanup, failure logs. `bash -n` on the three scripts OK | PASS |
| FIND-TASK-001-7 | Script and `production_ui_nginx_journey` use `WYRD_UI_JOURNEY`; no other name | `git grep WYRD_UI_NGINX_JOURNEY -- ':!changes'` no match | PASS |
| FIND-TASK-001-8 | `wyrd-auth::BrowserSessions::logout(tenant_route_key, cookie)` resolves the route via `resolve_tenant_slug` and revokes nothing unless it equals the value's tenant; HTTP adapter passes `request.tenant` | RED: extended `pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable` — relabeled Beta under Acme logout left Beta 401; GREEN 1 passed (Beta and Acme survive, Acme label cleared, repeated logout 204, keys survive, outage 503) | PASS |
| FIND-TASK-001-9 | Rustdoc on the three `Debug::fmt` (redaction + formatter `# Errors`), both test modules, `begin`/`recover`/`logout` delivery consequences, and the server module's linked committed-but-undelivered account | Source inspection; `mise run fmt`, `mise run lints` PASS | PASS |
| FIND-TASK-001-2 | Unchanged nginx journey, now via `WYRD_UI_JOURNEY` | `mise run test:identity:ui:container` not run: this host is macOS arm64 and the image build refuses off Linux | UNVERIFIED |

Diagnosis (journey regression, first `test:identity:ui` run):
- **Symptom:** `production provider replacement settings` — `expect(activated.status).toBe(200)` received 401.
- **Evidence:** trace `tenant human connection activated … retired=Some(…) revision=3`; the full-page action renders `+layout.server.ts` → `browserSessions.metadata`, which Rust answers 401 because activation ended alice's session.
- **Cause:** the old metadata fallback reused the earlier identity after a non-OK answer; the 200 was FIND-4's defect. Production behavior is now correct.
- **Fix site:** the test expectation only (`toBe(401)`, comment added); the following session-ended, second-realm and owner-key assertions still prove activation. Independent read-only diagnostician confirmed cause and fix site; only other affected path is `deactivate`, whose assertion already tolerates it.

Commands (final source): `vitest run src/lib/server/auth/browser-sessions.test.ts src/lib/server/upstream.test.ts` 15/15; `pnpm check` 0 errors; UI build inside `test:identity:ui` OK; `scripts/postgres/with-test-postgres.sh -- … cargo nextest run --locked -p wyrd-server --lib -E 'test(=auth::browser::pg_tests::callback_failures_preserve_unavailable_classification) | test(/^auth::browser::/)'` 7 passed; `… --test integration -E 'test(=pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable)'` 1 passed; `WYRD_LOG=info mise run test:identity:ui` PASS; `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation` PASS; `git diff --check 2741395a3..HEAD` clean.

Limits: no journey-level RED was captured for FIND-3 (unit RED is real). FIND-2 Linux nginx execution, the Debian/Rocky matrix, native signed runtime, and human visual inspection remain unverified integrated gates. No new contract, migration, env var, dependency or schema; no unrelated file changed. Implementation is not acceptance: independent cumulative task review must pass.
