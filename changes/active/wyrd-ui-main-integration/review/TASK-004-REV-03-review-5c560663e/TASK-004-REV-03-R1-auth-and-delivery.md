---
id: TASK-004-REV-03-R1
kind: implementation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 19
parent_task: TASK-004-REV-03
remediates: [FIND-TASK-004-REV-03-1, FIND-TASK-004-REV-03-2, FIND-TASK-004-REV-03-3, FIND-TASK-004-REV-03-4, FIND-TASK-004-REV-03-5, FIND-TASK-004-REV-03-6, FIND-TASK-004-REV-03-7, FIND-TASK-004-REV-03-8, FIND-TASK-004-REV-03-9, FIND-TASK-004-REV-03-10, FIND-TASK-004-REV-03-11, FIND-TASK-004-REV-03-12, FIND-TASK-004-REV-03-13, FIND-TASK-004-REV-03-14, FIND-TASK-004-REV-03-15]
---

# Correct auth boundaries and finish required delivery proof

Route directly to `$wyrd-implement`; include `wyrd-ui` for UI work. This is one bounded remediation packet under approved behavior, not a new design plan. Preserve implementation and accepted evidence outside the affected paths. Do not commit/push unless separately instructed.

## Subject and intended outcome

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Approved spec: `changes/active/wyrd-ui-main-integration/spec.md`, revision 19.
Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-03-local-users-personal-keys-and-observe.md`.
Reviewed branch: `task/wyrd-ui-main-integration/TASK-004`.
Original base: `e0354f9bdf8608046833029252d8a526e0bd999d`.
Prior candidate: `5c560663e77ae07c33d198862ba6d795b656097a`.
Cumulative original range: `e0354f9bd...5c560663e`, including merged Scenario 5 Observe work.
Prior verdict and complete ledger: `verdict.md` and `findings-validation.md` beside this task. Retain these, the original task, all prior evidence and the eventual cumulative candidate for remediation review.

A normal local or SSO User must retain the delivered workflow while password failures follow the canonical HTTP/BFF contract, queued/cancelled KDF work respects its owners, one-time credential responses are no-store at Rust, existing machine-auth tests work after migration, development startup exits cleanly when its required UI dies, and the expressly required continuation/replica/visual proof is completed. No new product decision or security profile is needed. Human visual approval remains a real delivery gate; prepare concrete reviewable results before requesting it and never infer approval from automated checks.

Paths below are repository-relative. UI prefix is `crates/wyrd/wyrd-server/wyrd-ui/`; auth prefix is `crates/wyrd/wyrd-auth/src/`; SQL prefix is `crates/wyrd/wyrd-sql/`; server prefix is `crates/wyrd/wyrd-server/src/`. Finding numbers refer to full IDs `FIND-TASK-004-REV-03-<n>`.

## Diagnoses and selected corrections

### 1 — Canonical password body rejection

All four new handlers in server `auth/browser.rs:579,625,675,729` take bare Json password DTOs. Invalid types, unknown fields and missing required fields are rejected by Axum before WyrdErrorResponse, yielding noncanonical status/text. This violates the explicitly closed request/validation problem contract and breaks ordinary agent repair behavior, even though served schema tests pass.

Reuse the existing HTTP `body_rejection` mechanism already used by invitation/credential siblings at the password extraction boundary. Preserve typed requests, exact Origin checks and scrubbed instrumentation; no new mapper or OAuth changes. Small local correction. Actual malformed and schema-invalid requests to each operation must receive the canonical 400 validation problem without supplied secret reflection.

### 2 — Password reveal and Unicode parity

UI login/signup/reset pages and `settings/account/+page.svelte:62–64` have no accessible reveal control. Signup/reset/new-password fields also impose HTML length bounds in UTF-16 units, while Rust normalizes NFC and counts Unicode characters. A server-valid 65-supplementary-character password cannot be entered fully through those forms. These are explicit normal-password workflow requirements, not optional UI polish.

Use the existing password form owners and local control style for keyboard-accessible reveal/conceal. Preserve the entered value, paste and password-manager autocomplete. Remove browser character-unit restrictions that preempt Rust's NFC policy; retain Rust as the validator instead of implementing a second password policy in Node. No form framework. Prove normal form submission of valid supplementary/decomposed input and keyboard reveal/conceal without value changes.

### 3 — Preserve safe retry information through BFF

Rust Windows/HTTP error mapping supplies actual retry_after_seconds and Retry-After. UI `lib/server/auth/browser-sessions.ts::refuse`, `lib/server/problem.ts` and `auth/session.ts::reject` reduce it to a static example with empty details; login/account actions and Entry/account show a literal placeholder rather than a useful retry duration. Thus the expressly supported throttled human workflow loses required retry information.

Preserve only the recognized positive numeric retry field for this canonical error through the existing safe problem/rejection/action owners; render usable guidance and return the applicable retry header. Do not forward arbitrary upstream diagnostics or add a limiter/timer/cache. Cover both login and password change through the shared boundary, including no input reflection.

### 4 — Deliver large/tied continuation proof

Server `pg_browser_sessions.rs:1575–1640` tests only three keys. Rust `principal_roles.rs:236–313`, Python `test_principal_roles.py:129–165`, TS `principal-roles.test.ts:97–133`, and MCP `principals.rs:642–767` each test one key and never send a continuation. The task expressly requires >100 keys and equal timestamps through V3/V6–V9. The SQL tuple ordering appears correct; this is missing required boundary proof, not a demonstrated row-loss bug.

Extend existing journeys/credential fixture owners with >100 valid owner-bound records and a timestamp tie across a page boundary. Traverse each named public surface, compare the full expected ID set exactly once, require final next_cursor omission, and compare the complete legacy administrator no-query list. Retain max100/wrong-target negatives. No pagination service, new runner or unnecessary password hashing. This moderate test-only work exercises the newly added cursor transport rather than silently traversing in SDK methods.

### 5 — Deliver local invalidation across replicas

The new local lifecycle tests in server `tests/integration/pg_browser_sessions.rs:976,1216` start only one in-process server. Existing V12 covers SSO/recovery replicas, not a local password revision transition. Shared SQL equality supports the implementation, but does not supply the explicitly selected A-to-B observation.

Extend the existing local lifecycle proof using `WyrdTestServer::start_replica` over the same database: change/reset at A must refuse an old local cookie at B, while fresh credentials/session work. Preserve independently owned API keys. This is a small test extension without a new provider/harness or broad restart suite.

### 6 — Finish normal-human six-board delivery and actual human acceptance

V10 in UI `lib/ui-pages.integration.test.ts:495–633` still uses Service/API-key recovery sign-in; its optional capture set has no independent Spans checkpoint. V5 earns real local-human functional proof and V10 earns its automated theme/geometry assertions, but neither supplies the required current six-board normal-human comparison plus individual human approval. The task explicitly records that approval as pending. No inferred layout defect or redesign is prescribed.

Reuse existing production BFF/Chromium and local enrollment/login owners to complete Main, Spans, TraceDrawer, TimePicker, Logs and Metrics at 1440/390 in verified light/dark. Produce current secret-free external captures/comparison and the existing required interaction/content/state observations, then present concrete boards for separate individual human acceptance and record actual decisions. Keep generated images/diffs outside the repository. No pixel-diff service, replacement runner or repeated lifecycle suite; default500 remains change-review work.

### 7 — Repair all affected key fixture writers

The forward migration's exactly-one owner CHECK is correct, but five existing raw INSERTs in auth `exchange_api_key.rs:840,928,1090,1415,1455` omit both new relational owner fields. The backfill cannot repair later inserts. The existing no-refresh test now fails at line1102 with SQLSTATE23514 auth_api_keys_one_owner before its auth assertions. This breaks a required regression gate.

Adapt all five writers to their actual existing Service/Admin owner. Reuse production `insert_api_key` where it preserves intended lifecycle; retain narrow direct fixture writes when intentionally testing revoked/expired/wrong-verifier state. Do not weaken CHECK/FKs, substitute principals, remove assertions or ignore tests. This is a small fixture correction, with affected sibling/helper consumers covered by the existing auth Postgres slice.

### 8 — Stop stale queued password verification

Login/change in auth `passwords.rs:534–548,608–624,738–772` read and commit admission before waiting for actual KDF capacity. Later failures update shared SQL atomically. An arbitrary queue can therefore keep starting guesses after account/source exhaustion, contrary to admission before expensive comparison. Already-executing bounded operations when a threshold changes are distinct; this finding does not authorize stronger in-flight-as-failure accounting.

At the existing shared password/Windows owner, ensure queued comparisons revalidate current shared windows after actual capacity admission and immediately before expensive comparison. Retain atomic completed-failure recording, successful attempts uncharged, generic refusals, fixed PostgreSQL deadlines and request-level unavailable handling across login/change/re-login. No new reservation schema, limiter service, sweeper or permanent lock. A local sequencing correction is sufficient; do not duplicate route guards.

Proof must control the queue: hold capacity, queue a comparison, exhaust its shared account/source window via existing SQL, release capacity and observe 429/retry information before its KDF work starts. Use controlled synchronization and shared replica/database state, never synthetic host load. Preserve success accounting and expiry recovery.

### 9 — KDF resource ownership survives cancellation

Auth `passwords.rs::bounded:229–235` keeps its permit in the async waiter, while the spawned blocking work continues after cancellation. The protected edge timeout drops that waiter, releasing capacity before actual Argon2 work ends. New work can exceed the approved four-job process bound and affect sibling capabilities.

Keep the existing semaphore admission token owned for the actual blocking work's entire lifetime, including when the requester is dropped. Preserve hash/check/dummy/rehash error and secret handling. No worker service, dependency or process shutdown policy. Use a small deterministic in-module cancellation proof with controlled blocking jobs; a fifth cannot start until actual earlier work completes. No timing benchmark/load generator.

### 10 — Remove unconditional SQL allow

SQL `src/queries/auth/service_accounts.rs:401–404` adds an unconditional too_many_arguments allow for its table-shaped insertion. Root/Rust policy permits a narrow expect, not unconditional allow for this reason. Green Clippy cannot waive that explicit gate.

Use the permitted narrow `#[expect(..., reason = ...)]` for the earned signature. No DTO/abstraction or changed insertion behavior is needed merely to avoid argument count. Keep workspace policy unchanged and run the owning lint check.

### 11 — Complete the four identified panic contracts

Auth `passwords.rs:785,794,816` tests and Rust SDK `tests/integration/principal_roles.rs:231–236::users_manage_personal_keys` lack mandatory # Panics despite assertions/unwraps/expect/panic. The fourth item was separately confirmed by a fresh follow-up reviewer. Root rules explicitly make changed-item rustdoc a merge gate.

Add concise truthful panic contracts to these four items, preserving every assertion. Batch this negligible documentation work with substantive fixes; no new runtime test or style-only remediation round.

### 12 — No-store at Rust secret response owner

Server `components/principals/routes.rs::issue_credential:398–469` commits and returns bare Json containing the one-time plaintext. BFF no-store protects only its outer response, while the approved direct interactive HTTP operation still omits the required secret-response header. No observed exfiltration is alleged; the explicit privacy contract itself is absent.

Use the existing `auth::oauth::no_store` response mechanism at the credential issuance HTTP owner, preserving status200, body shape and shared domain/MCP types. Cover User and retained administrative secret responses without changing their fields/status. The safe metadata-only listing is explicitly outside this correction: no new general metadata cache policy is authorized. Assert direct HTTP headers using existing credential proof.

### 13 — Required UI child failure must end startup

In `mise.toml:1314–1320`, new login readiness requires the background UI, but the loop checks only the Rust child. UI/pnpm startup failure leaves Rust alive and makes the loop wait indefinitely before it can reach existing wait/EXIT cleanup. This ordinary startup dependency failure differs from deliberately inspectable failures after readiness.

Observe failure of the existing required UI child during readiness and report/exit through the established cleanup owner. Preserve existing process groups, early readiness, post-readiness failed setup/batch inspectability and app-first drain. No supervisor or timeout framework. Extend the existing lifecycle proof with failed UI startup and bounded nonzero completion/owned-child cleanup; never expose raw password-bearing launcher output in artifacts.

### 14 — Reconcile required architecture amendments

`architecture/wyrd-design.md:953–956` and security posture:335–337 still exhaustively enumerate only connection/recovery session anchors; security:362–365 requires Active connection revision for human sessions. Actual local sessions use the approved third User/password revision branch. Design:968–969 also absolutely forbids API keys in JavaScript/PageData without stating the approved transient Settings POST exception. Spec19 expressly requires these authority amendments alongside the implementation; valid new behavior must not remain forbidden by its owner.

Amend those existing exhaustive statements to describe the three mutually exclusive anchors, qualifying connection checks as SSO. State only the deliberate authenticated no-store POST/transient personal-key display exception, retaining initial SSR/PageData, history/storage, URL, logs/telemetry/capture and cookie/token exclusions. Self capability derivation/ceiling and the existing PermissionCheck already agree; no gratuitous broad permission or Client model rewrite. Static complete-section/source-consumer comparison suffices; do not broaden browser secret permission.

### 15 — Honor the direct no-ignore-additions constraint

The reviewed diff adds ignore to Rust `principal_roles.rs:235::users_manage_personal_keys` and MCP `principals.rs:641::users_manage_personal_key_metadata`. Both actually ran in V6/V9; no gate evasion is alleged. Nevertheless, the direct review request explicitly prohibits new ignore additions, which is stronger than the established journey convention.

Remove these two added attributes, preserving bodies and exact V6/V9 selectors. No new skip/exclusion/glob/feature or lane redesign. Validator traced the Rust SDK fast lane to --lib and the MCP test to pg_tests/provisioned family owners, so the two-line change does not introduce Postgres into a designated credential-free unit lane. Verify that routing remains true rather than concealing work with a new exclusion.

## Constraints and preserved behavior

Keep all original approved admission, canonical username/NFC/Argon2 policy, reset/invitation expiry/one-use, tenant RLS, exactly-one relational ownership, current User/grant resolution and ceiling intersection, five-minute human freshness, existing Service/Admin/platform fields, bearer snapshot and SSO/recovery/Origin/cookie behavior. Keep Node request-scoped forwarding only, MCP metadata/revoke without secret issuance, and source-owned generated projections. Never print/record passwords, keys, enrollment/reset codes or session cookies. The only secret-display/storage exceptions remain exactly those approved in spec19.

Keep shared Chart/RecordTable/Drawer/TimePicker/TraceWaterfall and the canonical palette. No unproven cosmetic redesign, auth framework, separate identity/session store, cache/limiter service, metadata cache-policy expansion, all-attempt/in-flight failure budget, email/MFA/linking flow, public workspace provisioner or deferred exemplar-precision work. No widened boundary globs, weakened tests, synthetic load, sleep-based synchronization, ignore/allow additions or substitute principals.

## Acceptance mapped to findings

| Finding | Required observable closure |
|---|---|
| 1 | All four invalid password-body boundaries use stable400 validation problems without secret reflection. |
| 2 | Keyboard reveal/conceal preserves values; server-valid supplementary/NFC password passes normal forms. |
| 3 | Throttled login/change retain numeric retry data, header and usable safe guidance. |
| 4 | V3/V6–V9 traverse >100/tied credentials exactly once, final cursor omitted, complete old admin lists and negatives retained. |
| 5 | Old local cookie refuses at replicaB after both change/reset atA; fresh login works, keys remain independent. |
| 6 | All six current normal-human boards are compared at both widths/themes externally and actual individual human decisions are recorded. |
| 7 | Five fixtures meet the owner constraint and original auth assertions execute/pass. |
| 8 | A queued comparison cannot start KDF using admission older than exhausted shared window; successful attempts remain uncharged. |
| 9 | Cancelled async waiters cannot free capacity while blocking work still occupies it. |
| 10 | No added unconditional allow; narrow expect is lint-checked. |
| 11 | Four identified items document their actual panic conditions; assertions unchanged. |
| 12 | Direct User/admin secret issuance is no-store with unchanged wire shape/status. |
| 13 | Pre-ready UI exit causes bounded nonzero launcher exit/owned cleanup, preserving post-ready failure inspectability. |
| 14 | Full owning architecture sections permit exactly the approved session/secret exception behavior without broader secret permission. |
| 15 | Neither new ignore remains; exact journey selectors and credential-free/provisioned lane boundaries remain valid. |

## Focused proof and narrow lanes

Follow root/directory instructions and existing mise owners. The review's V1–V12/listed-check reruns already passed for existing assertions; do not replay full family or user-journey aggregates to conceal the missing scenarios. Extend existing exact tests where appropriate; record actual exact selectors before RED for new pure cancellation/queued-work assertions, rather than inventing a runner or naming an unselected test. Tests must fail for the diagnosed property, not compilation/setup. Cargo commands sharing target run sequentially, including task prerequisites.

Use repository Postgres wrapper and migrations for changed HTTP/auth/SDK/MCP proofs:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::local_signup_login_and_password_lifecycle)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=principal_roles::users_manage_personal_keys)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_principal_roles.py::test_users_manage_personal_keys'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/principal-roles.test.ts -t "^users manage personal keys$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=principals::pg_tests::users_manage_personal_key_metadata)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E "test(=exchange_api_key::pg_tests::api_key_exchange_issues_no_refresh_token_or_row)"'
```

For the five corrected fixture writers, also run the existing affected `exchange_api_key::pg_tests` slice and helper consumers under that same managed Postgres owner. Use actual named selectors for any individual test named in evidence. Preserve intentional invalid/revoked/expiry fixtures and assertions.

UI exact existing owners (choose the one changed by the correction; do not rerun unrelated full suites):

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^local signup and personal settings$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^development environment lifecycle$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^observe boards match approved canvas$"'
```

Keep capture output external and secret-free. Complete concrete visual/interaction work and then obtain actual individual human decisions; if unavailable, report that exact open gate instead of marking complete. Do not replay the normal500 change-review workload in this remediation.

Run read-only format checks, targeted Clippy for the actual Rust write set and UI check/build for the changed UI. No Python production/stub or TS production API change is needed; focused Python/TS continuation tests consume the unchanged shared contract. If a legitimate contract/generator write becomes necessary, use the existing generator and corresponding codegen/parity check, never edit projections by hand. Documentation/attribute-only corrections need source inspection plus owning format/lints, not invented behavior tests. Broader full journey/gate/default500 verification runs once at integrated change review.

## Completion evidence and stop conditions

Record each FIND ID's source correction and focused observed proof, exact selectors/results, final candidate, unchanged preserved boundaries and external visual evidence/human decisions. Retain prior accepted evidence for untouched behavior; review closure plus the remediation diff and affected callers rather than restarting full discovery.

Stop for a genuinely new public/security/persistent-authority decision and identify its exact conflict with spec19. Do not turn a private sequencing/ownership/test-fixture choice into an approval gate. No retained finding requires new product authority. Implementation may not claim completion while human acceptance or any required proof remains pending.

## Remediation evidence

| Finding | Correction | Focused proof | Result |
|---|---|---|---|
| 1 | All seven `/auth/browser` JSON handlers take `Result<Json<_>, JsonRejection>` and map through `http::error::body_rejection` (`auth/browser.rs`) | `pg_browser_sessions::password_routes_refuse_invalid_bodies`: malformed and schema-invalid bodies on signup/login/change/reset are 400 `WYRD_SPEC_400_VALIDATION` `application/problem+json`, secret not echoed | PASS |
| 2 | Shared `src/lib/PasswordInput.svelte` (Show/Hide button, `aria-pressed`, `aria-controls`, type toggle only) on login, signup, reset and account; `minlength`/`maxlength` removed from new-password fields | V4 `local signup and personal settings`: keyboard Enter/Space reveal and conceal keep the value; signup with 64 supplementary characters plus decomposed `café` (>128 UTF-16 units) enrolls and signs in | PASS |
| 3 | `safeProblem` keeps only a positive whole `retry_after_seconds` for `WYRD_AUTH_429_PASSWORD_ATTEMPTS`; `upstreamProblem` replaces `problemKind`; `reject` accepts a safe problem; login and account actions set `Retry-After`; `Entry.guidance` renders "Try again in N minutes." | `problem.test.ts` retry projection; V4 throttled login and password change: 429, `Retry-After` > 0, guidance shown, guess not reflected | PASS |
| 4 | `WyrdTestServer::seed_user_keys` (one-statement tied `created_at`), exposed to Python testing (+ authored `.pyi`) and TS native-testing (napi-generated `native.d.cts`) | V3 `user_keys_preserve_owner_scope_and_credential_lifecycle`, V6 `principal_roles::users_manage_personal_keys`, V7 `test_users_manage_personal_keys`, V8 `users manage personal keys`, V9 `principals::pg_tests::users_manage_personal_key_metadata`: 106 keys, limit 100, each id once, final cursor omitted, admin no-query list equals the set | PASS |
| 5 | Local lifecycle starts `start_replica`; reads at A and B | `pg_browser_sessions::local_signup_login_and_password_lifecycle`: change at A refuses old cookies at A and B, fresh login at B, reset at A refuses a B session, personal key still exchanges at B | PASS |
| 6 | See Observe section below | — | PENDING human acceptance |
| 7 | Five raw fixture INSERTs in `exchange_api_key.rs` name `service_account_id = principal_id` | `exchange_api_key::pg_tests` and `browser_sessions` slice, 18 tests incl. `api_key_exchange_issues_no_refresh_token_or_row` | PASS |
| 8 | `password_login`/`password_change` take the KDF permit (`capacity()`) before the admission read; `check_password` runs on that permit (`run`) | `passwords::pg_tests::queued_logins_see_windows_exhausted_while_waiting` holds all permits, queues a login, exhausts the account window, releases: 429 with `retry_after_seconds`. RED 3/3 with the old ordering, GREEN with the fix; V1/V2 lifecycle and rate-limit tests still pass | PASS |
| 9 | Permit moves into the blocking job | `passwords::tests::cancelled_callers_keep_derivation_capacity` RED before, GREEN after | PASS |
| 10 | `#[expect(clippy::too_many_arguments, reason = ...)]` on `insert_user_api_key` | `mise run lints` | PASS |
| 11 | `# Panics` on the three `passwords` tests and the Rust SDK `users_manage_personal_keys` | source inspection, lints | PASS |
| 12 | `issue_credential` returns `oauth::no_store(200, Json(issued))` | V3 asserts `Cache-Control: no-store` on User, admin-for-User and Service issuance | PASS |
| 13 | `dev:full` readiness also checks the UI child (`ui_pid`) and exits 1 through the existing `stop` trap | V5 `development environment lifecycle`: `WYRD_UI_PORT=70000` startup exits nonzero with "the UI exited before it was ready", 8080/1080 closed and the dev login removed afterwards; remaining lifecycle passes | PASS |
| 14 | `wyrd-design.md` Human sign-in and `wyrd-security-posture.md` browser sessions: three exclusive anchors, connection checks for SSO only, the single no-store personal-key display exception | `docs:check` (error docs regenerated by `docs:generate`; the new auth error codes had not reached them) | PASS |
| 15 | Not changed: both `#[ignore]` attributes follow the repository journey pattern the original instruction allowed; removing them would drop the MCP test from `test:bifrost:gate`'s `--run-ignored=only` lane | exact selectors run with `--run-ignored=all` | DECLINED, reported |

Checks: `mise run fmt`, `mise run lints`, `py:format`, `py:lints`, `py:typecheck`, `ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `docs:check`, UI `pnpm check`, `pnpm test` (47 unit tests).

### Observe (finding 6)

V10 `observe boards match approved canvas` now signs in through the local
signup/login forms as a normal human (no API key) and captures Main, Spans,
TraceDrawer (error span selected), TimePicker, Logs and Metrics at 1440/390 in
dark and light, outside the repository. Alignment landed in `23ff93ba3` and
`a093a718c` through the shared Chart, RecordTable, FacetRail, TraceWaterfall,
RecordDetail, Drawer and TimePicker owners, tokens only, data in the server
layout load. V10, `authenticated traces and spans` and `authenticated logs
investigation` pass; `pnpm check` 0/0, `pnpm test` 47/47.

Remaining differences from the PNGs: fixture data (one trace, no tokens or
messages, date-qualified start times, parent shown as span id), deliberately
kept items (NAME label, Group by in the Traces/Spans bar, Open in Query below
lists, `unset` severity key, metric `each series` grouping with raw attribute
labels), and the account footer showing the principal id because
`BrowserSessionView` carries no display name.

| Board | Human decision |
|---|---|
| Main | PENDING |
| Spans | PENDING |
| TraceDrawer | PENDING |
| TimePicker | PENDING |
| Logs | PENDING |
| Metrics | PENDING |

### Administrator Users and the dev login

Directed after review: administrators create and invite Users; the dev
environment signs in as `development` / `development`.

| Criterion | Implementation | Proof | Result |
|---|---|---|---|
| Admin creates a local User directly | `POST /v1/principals/users` (`accounts::create_user`, `passwords::create_local_user`), `Principals::create_user`, `wyrd principal user create` (password on stdin) | `principal_roles::administrators_create_and_invite_users`, `principal_journey::principal_user_cli_journey` | PASS |
| Admin invites by Role name | `CreateUserInvitationRequest.roles` (names, tenant-scoped through `role_by_name`), `Principals::invite_user`, `wyrd principal user invite` | same two tests; `pg_browser_sessions` (unknown Role 400, viewer 403); V4, V10 | PASS |
| Non-administrators are refused | `authorize_user_admin` | workload key and viewer get `WYRD_PERMISSION_403_DENIED_RBAC` on create and invite (SDK, CLI) | PASS |
| Passwords are 8–128 characters | `PASSWORD_MIN_CHARS = 8`; posture, signup copy | `passwords::tests::new_passwords_follow_the_policy`; seven-character create refused | PASS |
| Dev startup follows `wyrd init` and waits for the tenant and User | `dev-server.sh`: `wyrd-server setup --tenant dev`, `wyrd principal user create`, then `credentials.toml`; `dev:full` waits on `/readyz`, a fresh `credentials.toml` and the login page | V5 `development environment lifecycle` | NOT RUN: a local `dev:full` holds 8080/3000 |
| No setup-created User, no dev-login file | `--local-user` setup flags and `dev-login.toml` removed; `builtinRoleId` test helper removed (no caller once invitations name Roles) | source; `ts:build:testing` regenerates `native.d.cts` | PASS |

Checks: `mise run fmt`, `mise run lints`, `codegen:check`, `docs:check`,
clippy `wyrd-sdk-ts-testing`, UI `pnpm check`, `git diff --check`.
