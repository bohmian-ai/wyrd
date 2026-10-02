# Repository standards review — TASK-004 R2

## Review Findings

### Critical

None.

### Important

- **STD-R2-001 — stale revision-8 behavior remains in materially changed Rust and SDK documentation.** `crates/shared/wyrd-client/src/config.rs:172-175`, `sdks/wyrd-sdk-python/src/client.rs:42-46`, and `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1076-1083` still say an unselected saved login can be "ambiguous," although approved spec revision 8 and `SavedLogins::select` now choose the newest matching-server login when `tenant` is absent (`crates/shared/wyrd-client/src/saved_login.rs:194-229`). `sdks/wyrd-sdk-python/src/testing.rs:800-802`, `sdks/wyrd-sdk-ts/native-testing/src/lib.rs:587-589`, and its generated declaration `sdks/wyrd-sdk-ts/testing/index.d.ts:215-217` still describe the removed custom "CLI handoff," although the shared helper actually drives the RFC 8628 device-code grant (`crates/wyrd/wyrd-testing/src/human_login.rs:151-180,219-240`). This violates `AGENTS.md` §16 and `architecture/agent-rules.md`' hard requirement that every materially changed Rust item, test helper, and generated declaration accurately explain its current workflow. It can mislead maintainers and SDK consumers into preserving two behaviors the binding authority explicitly deleted. Replace the ambiguity language with the newest-login default and explicit-selector mismatch behavior; replace "CLI handoff" with the RFC 8628 device login; then regenerate the TypeScript test declaration through its owner and run the documentation/codegen/type declaration checks. Do not hand-edit `testing/index.d.ts`.

### Suggestions

None.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `362878494ed80ca5c5533a4364f744bf92dd1e06`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revisions 8 and 9
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Prior review: `changes/active/oidc-production-readiness/review/TASK-004-r1/verdict.md`
- Binding human direction: `changes/active/oidc-production-readiness/review/TASK-004-r1/human-direction-FIND-TASK-004-4.md`, including every addendum
- Explicit exclusion: spec revision 10 REQ-021 and its form-encoded RFC 6749 wire-format work belong to TASK-008 and were not assessed as a TASK-004 violation.

The candidate remained `362878494ed80ca5c5533a4364f744bf92dd1e06` throughout this review. `.codegraph/` is absent, so the review used the cumulative Git diff, `rg`, and direct source/caller inspection.

## Authority coverage

| Changed surface | Applicable authority read and applied | Result and evidence |
|---|---|---|
| Approved spec, task, prior review packet, and human direction | `AGENTS.md`; `architecture/references/languages/spec-driven-development.md`; spec revisions 8 and 9; TASK-004; R1 verdict and human-direction addenda | **FAIL only for STD-R2-001.** Revision 8 correctly replaces the custom handoff and bespoke renewal state in implementation, but the changed Rust/SDK docs cited above retain the superseded vocabulary and ambiguity behavior. R1 artifacts remain preserved as immutable review evidence. |
| Shared Rust client credential precedence, saved-login selection/renewal, one credential file, filesystem safety, and transport | `AGENTS.md` §§2–6, 9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | **PASS except documentation in STD-R2-001.** `CredentialsFile` owns the one `credentials.toml`, its blocking directory lock, ownership/mode checks, atomic replacement, and preservation of unrelated TOML (`credentials_file.rs:1-11,65-267`). `SavedLogins` owns selection, save/remove, and refresh under that lock (`saved_login.rs:151-379`); no `RefreshPending`, generation, tombstone, lock deadline, per-login version, separate login store, or separate token cache remains. `TokenExchange::new` reuses the existing HTTP transport policy for secret-bearing routes. |
| Pure auth contracts and public error catalog | `AGENTS.md` §§2–4, 7–9, 16; `wyrd-design.md`; `patterns.md`; `rust-core.md`; `errors.md` | **PASS.** `crates/wyrd-spec/src/auth/device.rs` contains synchronous typed serde/schema contracts only; `wyrd-spec` remains IO-, async-, server-, SQL-, and PyO3-free. Device and saved-login failures use the derive-backed public Wyrd error catalog, with generated error projections present. |
| Device authorization, OIDC callback binding, refresh revocation, server routes, and audit | `AGENTS.md` §§5–6, 9, 16; `architecture/agent-rules.md` audit rules; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | **PASS.** Stateful behavior is on `CliLogins`; handlers are typed, scrub secret-bearing requests with `skip_all`/`skip(request)`, and delegate to the owner. Device redemption appends the allowed audit in the same tenant transaction (`cli_logins.rs:310-397`); logout revocation and its audit commit together (`cli_logins.rs:400-452`). The shared auth-route governor remains the sole admission mechanism. REQ-021 JSON-versus-form behavior is intentionally excluded. |
| Tenant SQL, migration, persistent device state, and transaction ownership | `AGENTS.md` §§3, 9, 16; `architecture/agent-rules.md` SQL capability rules; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md` | **PASS.** All added production query signatures take `&mut TenantConn<'_>` (`device_authorizations.rs:75-193`), none commits or rolls back, and forced RLS is the tenant boundary. No raw `PgPool`, `Pool<Postgres>`, naked connection, or caller-supplied transaction entered production signatures or fields; raw pools found by the static scan are confined to external PostgreSQL tests. PostgreSQL owns expiry/poll timestamps, and the migration enforces tenant RLS and the ten-minute bound. |
| CLI device flow and browser launch | `AGENTS.md` §§3, 5–6, 9, 16; `wyrd-design.md`; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | **PASS.** `LoginFlow` owns device authorization, browser opening, polling, and save orchestration. Polling follows the standard `authorization_pending` interval and five-second `slow_down` increment; filesystem work uses `spawn_blocking`; status/output contain no token. Logout deletes locally before best-effort server revocation and emits the required warning on failure. |
| Rust, Python/PyO3, and TypeScript/N-API SDK projection | `AGENTS.md` §§2–3, 7–9, 11, 16; `pyo3-boundaries.md`; `python-api-and-stubs.md`; `typescript-guide.md`; `testing-workflows.md`; `errors.md` | **FAIL only for STD-R2-001.** Runtime bindings stay thin over `wyrd-client`, constructors consistently accept and forward `tenant`, PyO3 constructors are named `__new__` with explicit signatures, and no foreign-runtime object is held across an await. Generated `.pyi`, `.d.ts`, `.d.cts`, and error-code projections are present. The public/source documentation cited in STD-R2-001 is inaccurate, and the generated test declaration faithfully propagates one stale source comment. |
| First-class journeys, test helpers, and runtime ownership | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement/runtime rules; `testing-workflows.md`; Python and TypeScript language references | **FAIL only for STD-R2-001.** CLI, concurrent Rust client, Rust SDK, Python, and TypeScript real-server journeys remain wired into the provider-backed identity lane. Python lifetime behavior stays in Python and Node lifetime behavior in TypeScript; external Rust tests earn their placement through live server/process boundaries. The Python and TypeScript test-helper rustdoc incorrectly names the removed handoff. |
| Manifests, dependency cost, features, lockfile, workspace-hack, and platform scope | `AGENTS.md` §§1, 3–4, 11–12; `architecture/agent-rules.md` feature rules; `rust-core.md`; `testing-workflows.md` | **PASS.** `toml_edit` is used for preserving existing credential-file structure, `rustix` is an already workspace-managed dependency scoped to Unix ownership checks, and added Tokio features are exercised by runtime bridging, signals, and polling. No new Cargo feature, client-tier SQL/cloud/data dependency, wildcard version, or per-crate profile entered the diff; workspace-hack and lockfile changes match the manifest cone. |
| Generated schemas, OpenAPI proof, Python/TypeScript declarations, API error docs, and public documentation | `AGENTS.md` §§8, 11–12, 16; `architecture/agent-rules.md` generated-artifact rule; `python-api-and-stubs.md`; `typescript-guide.md`; `testing-workflows.md`; `errors.md` | **FAIL only for STD-R2-001.** Contract schemas, served-OpenAPI assertions, error catalog/docs, Python stubs, N-API declarations, and TypeScript error codes are represented in the cumulative diff and the implementation record reports their gates green. Green generation proves parity, not semantic accuracy: `testing/index.d.ts` reproduces stale source rustdoc. |
| `mise.toml` identity routing and boundary gates | `AGENTS.md` §11–12; `architecture/agent-rules.md`; `implementation-execution.md`; `testing-workflows.md` | **PASS.** The identity wrapper provisions the environment and selects the CLI, Rust SDK, concurrent client, Python, and TypeScript journeys by exact target. Provider-dependent tests remain out of ordinary no-Keycloak lanes rather than being disabled. The production-wheel boundary check remains isolated from the testing-feature development install. |
| Docs and plan/skill collateral in the cumulative range | `AGENTS.md` §§12, 14, 16; `spec-driven-development.md`; mirrored skill ownership | **PASS except STD-R2-001.** The `.agents` and `.claude` copies of each changed Wyrd skill are byte-identical, revision 9 removes the proprietary live-provider qualification contract in favor of standards plus Keycloak/Dex journeys, and public auth/client docs describe the newest-login default. Historical R1 reports correctly remain historical. |

## Rule-by-rule results

| Repository rule | Result | Evidence |
|---|---|---|
| Durable behavior stays server-owned; shared client behavior is implemented once in `wyrd-client` | PASS | Device lifecycle and revocation live in `wyrd-auth`/`wyrd-sql`; selection, renewal, credential-file persistence, and exchange live in `wyrd-client`; SDK bindings delegate. |
| `wyrd-spec` remains foundational, IO-free, async-free, and PyO3-free | PASS | The new device module is contract-only and no forbidden dependency was added. |
| Struct-centered ownership and narrow async boundaries | PASS | `CliLogins`, `CredentialsFile`, `SavedLogins`, `SavedLoginSource`, and `LoginFlow` own cohesive stateful workflows. Pure parsing/normalization helpers stay synchronous; async operations await SQL, HTTP, signals, or timers. |
| Secrets are typed, redacted, and excluded from logs/output/URLs | PASS | Device/refresh/access values use `SecretBearer`/`SecretString`; route instrumentation skips bodies; CLI output prints only code, URL, tenant/origin, and expiry; the device code is absent from verification URLs. |
| Public errors use the derive-backed catalog | PASS | New public client/device variants originate in `wyrd_spec::error::WyrdError` and project through central boundary mappers. |
| Production SQL uses only `TenantConn`/`OperatorPool` capability types | PASS | Added device query functions use `&mut TenantConn<'_>` exclusively; no production raw-pool propagation was found. |
| Tenant query callees do not own transaction commit/rollback | PASS | Device query helpers operate through the caller's `TenantConn`; `CliLogins` owns commits at workflow boundaries. |
| Audit decisions use the canonical transactional path | PASS | Successful redemption and refresh-chain logout call `append_auth_audit` before the same transaction commits; no second audit sink was added. |
| Python and TypeScript bindings remain thin and generated declarations match source | PASS for mechanics; FAIL for documentation | Constructor/signature parity is present and recorded gates are green. STD-R2-001 identifies inaccurate source prose that generation reproduces rather than detects. |
| User-facing behavior has real journeys in each first-class SDK runtime | PASS | Rust/Python/TypeScript saved-user-auth journeys plus CLI and concurrent-client journeys are wired to `test:identity:journey`. |
| Tests stay in the correct tier/runtime and no gate is bypassed | PASS | Provider-backed tests are selected by the identity provisioning wrapper; no new `#[ignore]`, broad exclusion, weakened assertion, or production `#[allow]` was added to clear the change. |
| New/materially changed Rust items, fields, helpers, and tests have accurate substantive rustdoc | **FAIL** | STD-R2-001: materially changed client and test-helper docs describe deleted revision-8 behavior. |
| Dependency and feature cost stays in the narrowest owner | PASS | `toml_edit` is client-owned, `rustix` is Unix-only, and no specialized server/data dependency moved into the client tier. |
| Standard/conventional mechanisms are used without extra bespoke state | PASS | RFC 8628 replaces the custom CLI handoff; conventional lock/reread/refresh/save replaces the bespoke pending/generation/tombstone/deadline model; the newest-login default replaces custom ambiguity. No unapproved extra mechanism remains in implementation. |

## Open Questions

None affecting the repository-standards result.

## Verification Notes

Independently performed during this review:

- `git diff --check 06f134dc14164c040c0e5014d21de29c240f4116..362878494ed80ca5c5533a4364f744bf92dd1e06` — PASS.
- Static scan of all changed Rust production signatures and fields for raw `PgPool`, `Pool<Postgres>`, `PgConnection`, and caller-supplied SQL transactions — no violation; matches were test-only.
- Static scan of the changed auth/client/SDK sources for superseded `RefreshPending`, `LoggedOut`, custom lock timeout, generation, ambiguous selection, and CLI-handoff vocabulary — only the documentation cases in STD-R2-001 remain relevant.
- Direct source trace from Python/TypeScript `save_human_login` through `HumanSso::save_login` to `HumanSso::cli_login` confirms the helper uses device authorization, verification-page approval, provider sign-in, and device-code redemption rather than a custom handoff.
- Byte comparison of the three changed `.agents`/`.claude` skill mirror pairs — PASS.
- Candidate identity rechecked after inspection — unchanged at `362878494ed80ca5c5533a4364f744bf92dd1e06`.

The implementation record reports `fmt`, `lints`, Python format/lints/typecheck, `codegen:check`, client/PyO3/production-wheel/workspace boundaries, docs, served OpenAPI, shared/SDK/CLI/Wyrd tests, every focused identity target, the unfiltered identity aggregate, and Python/TypeScript integration and declaration lanes passing. They were available as evidence and were not re-executed concurrently with the other independent reviewers. Those green gates do not reject semantically stale prose when generated output agrees with its source, so they do not close STD-R2-001.

## Overall result

**FAIL**

The implementation satisfies the reviewed repository boundaries, but `AGENTS.md` makes accurate rustdoc for materially changed items a hard merge requirement. STD-R2-001 is a bounded documentation correction at existing source owners plus regeneration; it requires no new mechanism, option, setting, or architecture.
