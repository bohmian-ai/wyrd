# TASK-012 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Candidate identity was rechecked during this review and remained unchanged.
- `.codegraph/` is absent, so the repository's CodeGraph routing rule does not apply.

## Authority coverage

| Changed surface | Applicable authority read | Coverage and result |
|---|---|---|
| Workspace dependency declarations, lockfile, and `workspace-hack` | `AGENTS.md` §§1–4, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/references/languages/implementation-execution.md`; `mise.toml` | `oauth2 = 5.0.0` remains default-feature-free and is owned by `wyrd-client`; `webbrowser = 1.2.4` is confined to `wyrd-cli`; no Cargo feature was added; the lockfile and Hakari changes match the dependency move. Recorded `check:workspace-hack` and lint results pass. PASS. |
| `wyrd-client/src/auth.rs` OAuth transport, grant flows, revocation, and public error projection | `AGENTS.md` §§2–6, 9, 12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Doctrine #18/#20, Client model, Runtime identity, Error catalog; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md`; specification REQ-011/012/021 and INV-005 | The shared client remains the sole SDK-facing owner; `TokenExchange` is a cohesive dependency-owning struct; every network async method awaits real IO; secrets retain redacted types; remote cleartext validation and redirect refusal stay centralized; OAuth and problem responses project to the existing `WyrdError` catalog. Rust documentation is incomplete on newly introduced items and changed stateful async operations. FAIL; see REPO-001. |
| Saved-login refresh, CLI login/refresh/logout, and browser launch | `AGENTS.md` §§2–6, 9, 16; `architecture/wyrd-design.md` Client model and Runtime identity; `architecture/wyrd-security-posture.md` Access and refresh tokens / Cryptography and secret handling; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md`; task locked decisions | `SavedLogins::renew` still performs refresh under the existing file lock; CLI operations delegate OAuth to `wyrd-client`; the browser launch is the standard `webbrowser` dependency with no configurable command or command interpreter. RFC 7009 remains the lead-approved one-form-POST exception through the redirect-free client and was not reopened. PASS, subject to REPO-001's documentation closure for the shared async owner. |
| `wyrd-testing` human-login helper, shared transport fixture, and CLI journey | `AGENTS.md` §§11–12, 16; `architecture/agent-rules.md` test placement/runtime rules; `architecture/references/languages/testing-workflows.md`; `TESTING.md`; `architecture/references/languages/maintainer-style.md` | The real-server CLI journey remains in its external journey target; the provider-backed helper remains in `wyrd-testing`; the transport mock now emits the RFC 6749 `expires_in` field and does not weaken assertions. No test was ignored, allowed, or moved down a tier. PASS for placement and integrity. |
| Rust/Python/TypeScript SDK consumers and generated declarations | `AGENTS.md` §§2–3, 8–9, 11; `architecture/wyrd-design.md` Client model; `architecture/wyrd-doctrine.mdx` Public surfaces; `architecture/references/architecture/patterns.md`; `architecture/references/languages/testing-workflows.md` | No language-specific token implementation or generated artifact changed. All SDKs continue to consume `wyrd-client`; recorded filtered Rust/Python/TypeScript identity journeys, `codegen:check`, and `ts:napi:check` pass. PASS. |
| Task packet and verification evidence | `AGENTS.md` §11; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md`; `architecture/references/languages/testing-workflows.md`; task Verification and Evidence section | Filtered identity commands and scoped family/boundary/codegen/lint results are recorded. The report names multiple Rust tests without recording their required exact `mise exec -- cargo nextest ... -E 'test(=...)'` commands/results. FAIL; see REPO-002. |
| SQL capabilities, PyO3, server handlers, schemas/OpenAPI, migrations, and durable database state | `AGENTS.md`; `architecture/agent-rules.md`; applicable router entries | No production SQL signature/field, PyO3 boundary, server handler, wire schema, migration, or persistent database owner changed. The raw-`PgPool` capability audit is therefore not applicable to this candidate. N/A. |

## Rule results

| Repository rule | Source evidence | Result |
|---|---|---|
| Shared client logic belongs in `crates/shared/wyrd-client`; language SDKs must not duplicate transport or durable credential behavior. | `crates/shared/wyrd-client/src/auth.rs:160-439`; no `sdks/*` source changed; recorded `check:client-tier`, `check:cli-client-tier`, and `check:sdk-client-tier` pass. | PASS |
| Specialized dependencies stay in the narrowest owner; features must be earned. | Workspace `oauth2` uses `default-features = false`; `crates/shared/wyrd-client/Cargo.toml:32` owns it; `crates/wyrd/wyrd-cli/Cargo.toml:48` alone owns `webbrowser`; no feature was added. | PASS |
| Stateful IO workflows use a cohesive concrete owner and async only for awaited IO. | `TokenExchange` owns `AuthHttp` and the configured `BasicClient`; its device, refresh, form, and revocation methods directly await HTTP IO. Pure conversions and error mapping remain synchronous helpers at `auth.rs:441-554`. | PASS |
| Secret-bearing calls must not follow redirects; secrets use redacted types and are not logged. | `TokenExchange::new` installs `reqwest::redirect::Policy::none()` at `auth.rs:227-230`; bodies use `SecretBearer` and only `expose()` at form-construction boundaries; `Debug` omits the client and tokens. Redirect regression test is at `auth.rs:1759-1802`. | PASS |
| Public failures crossing CLI/SDK boundaries use stable Wyrd errors; OAuth wire errors are the approved endpoint exception. | `refused`, `refusal`, and `oauth_error` at `auth.rs:477-554` map RFC responses and problem JSON to catalog variants; CLI uses `AuthError::into_wyrd`. | PASS |
| Every new or materially modified Rust item, including private fields and associated types, has substantive rustdoc; async stateful operations document cancellation/partial progress when relevant. | `AuthHttp`'s tuple field and `AsyncHttpClient::{Error, Future}` have no item docs at `auth.rs:131,151-152`; `call` only restates delegation at `auth.rs:154-156`; the materially changed stateful network operations at `auth.rs:313-412` do not consistently record cancellation/uncertain-response behavior, notably refresh rotation and revocation. | FAIL (REPO-001) |
| Tests remain in the owning runtime/tier and may not be weakened to clear a failure. | Rust-only helpers/tests remain Rust; the CLI journey drives a compiled binary and real server; the mock correction replaces nonstandard `expires_at` with RFC `expires_in`; no ignores/allows/assertion deletions entered the diff. | PASS |
| Every specifically named Rust test in a task or implementation report has and runs an exact focused `mise exec -- cargo nextest` command; a family lane does not replace that evidence. | Task lines 200-206 name seven Rust tests/test targets; lines 209-213 record only `test:shared`, CLI-lib, codegen, and checks. The task itself reiterates the exact-selector requirement at lines 160-161. | FAIL (REPO-002) |
| Use only the narrowest write-set verification at task review; full journeys/aggregates are reserved for change review. | This review started no Cargo or mise process. The recorded evidence uses filtered identity journeys and scoped owner/check lanes. No unfiltered journey or aggregate is required here. | PASS |
| Windows launcher proof follows the locked lead decision. | Task evidence lines 230-233 records `cargo check -p webbrowser --target x86_64-pc-windows-msvc` and explains why the full CLI target cannot link on this host; all custom per-OS launcher code is deleted. | PASS |

## Material repository-rule findings

### REPO-001 — New OAuth adapter items and stateful async operations do not meet the mandatory rustdoc contract

- Classification: `VIOLATION`
- Violated authority: `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for every new or materially modified Rust item, explicitly including private fields and associated types; `architecture/references/languages/rust-core.md` requires async documentation to record cancellation and partial-progress behavior when applicable.
- Location: `crates/shared/wyrd-client/src/auth.rs:131`, `:139-156`, and `:313-412`.
- Evidence: the new `AuthHttp(Client)` tuple field and `AsyncHttpClient::Error` / `Future` associated types have no rustdoc. `AsyncHttpClient::call` says only that it hands the request to `send`, which is placeholder-level documentation. The new refresh rotation and materially changed revocation path document errors but not the uncertain-response boundary: cancellation can occur after the server rotates/revokes the refresh token but before the client observes the response. Device authorization and the shared send/form boundary likewise do not state their cancellation/partial-progress behavior.
- Consequence: this violates the repository's explicit `BLOCK_BEFORE_MERGE` documentation rule and leaves maintainers without the retry-safety information needed for credential-state operations. In particular, the refresh-token crash/replay behavior is an approved security invariant, not an optional prose improvement.
- Required testable correction: add substantive rustdoc to the new tuple field, both associated types, and the adapter call; document cancellation/partial-progress and retry implications on the changed async owner methods, especially refresh rotation and idempotent revocation. Preserve the current protocol and code shape; this is documentation closure only. Verify by source inspection plus the existing narrow Rust formatting/lint lanes—do not add a new permanent check or broaden verification.

### REPO-002 — Named Rust verification lacks the required exact focused command evidence

- Classification: `VIOLATION`
- Violated authority: `AGENTS.md` §11, `architecture/references/languages/spec-driven-development.md` “Test command precision,” and `architecture/references/languages/testing-workflows.md` require every specifically named Rust test in the task/report to carry and run an exact `mise exec -- cargo nextest` command with package, target, and exact expression. The task repeats this at lines 160-161.
- Location: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md:200-213`.
- Evidence: the implementation table names the transport target-validation tests, saved-login origin and permission tests, redirect test, and `pg_auth_e2e_against_fixture::pg_tests::wyrd_client_authenticates_via_wyrd_access_token_header`, but the recorded command list contains only the shared family lane and other broad owner/check lanes. No exact commands or exact-command results are recorded for those named Rust tests.
- Consequence: the available family-lane result is credible broad execution evidence, but it does not satisfy the repository's required nonzero exact-selection proof and cannot show that each named selector was valid rather than stale.
- Required testable correction: record and run exact focused commands for the specifically named Rust tests using their real package/target and `-E 'test(=...)'`; include the repository-managed Postgres wrapper for the `pg_auth_e2e_against_fixture` test. Run only those focused commands needed to close this evidence gap. Do not run or require an unfiltered identity journey, full journey sweep, `test:rust`, `gate`, or another aggregate.

## Non-blocking notes

- No SQL capability type was introduced, so there is no `PgPool`/`TenantConn`/`OperatorPool` finding.
- No Python, TypeScript, generated declaration, OpenAPI, or schema source changed. The recorded codegen and N-API declaration checks are sufficient for this task-level write set.
- The lead-approved RFC 7009 form POST and `webbrowser`-crate Windows target proof were treated as locked inputs and were not reopened.
- No nonstandard extra mechanism, check, file, setting, or option is required by either finding.

## Verification reviewed

Recorded passing evidence: five filtered identity journeys, `mise run test:shared` (732 tests), `mise exec -- cargo nextest run --locked -p wyrd-cli --lib` (60 tests), `mise run codegen:check`, `mise run ts:napi:check`, `mise run check:client-tier`, `mise run check:cli-client-tier`, `mise run check:sdk-client-tier`, `mise run check:workspace-hack`, `mise run check:unwrap-audit`, `mise run fmt`, `mise run lints`, `git diff --check`, and `cargo check -p webbrowser --target x86_64-pc-windows-msvc`.

No new verification command was run during this parallel review. REPO-002 identifies the only required focused proof missing from the recorded evidence. Full journey suites and repository aggregates are intentionally deferred to change review.

## Overall result

**FAIL**

The repository ownership, dependency, security, error, runtime, and test-placement rules are satisfied, but the explicit rustdoc hard gate and exact named-test command rule are not yet satisfied.
