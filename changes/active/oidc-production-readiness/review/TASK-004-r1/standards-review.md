# Repository standards review — TASK-004 R1

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None. This review found no material repository-rule violation in the immutable
base-to-candidate range.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 7
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Binding human directions: TASK-003 R1 issuer binding, TASK-003 R2 real
  connection test, and TASK-003 R5 per-login refresh-chain logout scope
- Candidate was still `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
  when this report was completed.

## Authority coverage

| Changed surface | Applicable authority read and applied | Result and source evidence |
|---|---|---|
| Shared Rust client credential selection, renewal, filesystem store, and transport | `AGENTS.md` §§2–6, 9, 15–16; `architecture/wyrd-design.md` client model/runtime identity; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | PASS. The single owner is `SavedLogins`/`SavedLoginSource` in `wyrd-client`; `ClientConfig::resolve_credential` composes that source at the documented precedence point (`crates/shared/wyrd-client/src/config.rs:174-212`). Secret values remain `SecretBearer`, debug projections are redacted, blocking filesystem work is routed through the existing credential-source blocking boundary, and language SDKs do not duplicate renewal. |
| Pure wire contracts and public error catalog | `AGENTS.md` §§2–4, 7–9; `wyrd-design.md`; `patterns.md`; `rust-core.md`; `errors.md` | PASS. CLI handoff/revocation payloads remain typed, synchronous, IO-free `wyrd-spec` values. The new cross-surface client error is a derive-backed `WyrdError` catalog variant and is projected into Python/TypeScript declarations; no PyO3, SQL, runtime, or server dependency entered `wyrd-spec`. |
| Server HTTP adapters and auth owner | `AGENTS.md` §§5–6, 9, 16; `agent-rules.md`; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | PASS. Routes use typed bodies/results, generated OpenAPI registration, structured Wyrd errors, and scrubbed tracing (`crates/wyrd/wyrd-server/src/auth/cli_login.rs:72-85,119-136,160-170,194-203`). Durable behavior remains on the dependency-owning `CliLogins` service, not in handlers. Anonymous begin/claim/cancel/revoke paths do not accept a client-asserted effective tenant; tenant selection is resolved to server-owned state before tenant SQL. |
| Tenant SQL and migration | `AGENTS.md` §§3, 9, 15; `architecture/agent-rules.md` SQL rules; `wyrd-security-posture.md` tenant/data isolation; `patterns.md` storage pattern; `rust-core.md` Postgres boundaries | PASS. Production query signatures accept only `&mut TenantConn<'_>` (`crates/wyrd/wyrd-sql/src/queries/auth/cli_handoffs.rs:79-97,104-111,122-131,141-150`), do not commit or roll back, and rely on forced RLS rather than parallel tenant predicates. The migration carries tenant identity, forced RLS, a five-minute database-enforced bound, and Postgres-owned timestamps. No raw `PgPool`, naked connection, or caller-owned SQL transaction was added to production signatures or fields. |
| CLI | `AGENTS.md` §§3, 5–6, 9, 16; `wyrd-design.md` public/client model; `wyrd-security-posture.md`; `patterns.md`; `rust-core.md`; `errors.md` | PASS. The CLI owns only interaction/orchestration, delegates handoff and token operations to the shared client/server owners, never prints credentials, performs blocking local-store operations off the async worker, and documents the tombstone-before-revoke partial-progress behavior. No second credential store or provider trust path was added. |
| Python PyO3/package/stubs/tests | `AGENTS.md` §§7–8, 11, 16; `pyo3-boundaries.md`; `python-api-and-stubs.md`; `testing-workflows.md`; `errors.md` | PASS. New bindings live in `sdks/wyrd-sdk-python`, use explicit `#[pyo3(signature = ...)]` constructors named `__new__`, detach blocking/network operations through the shared runtime, and delegate to `wyrd-client`. Public stubs include the tenant selector. Test harness code remains behind the `testing` feature (`sdks/wyrd-sdk-python/src/lib.rs:19-20,136-141`; `Cargo.toml:21-34`) and the identity journey is kept out of the ordinary integration lane that lacks Keycloak. |
| TypeScript/N-API/package/declarations/tests | `AGENTS.md` §§2–3, 8–9, 11, 16; `wyrd-design.md` client model; `typescript-guide.md`; `testing-workflows.md`; `errors.md` | PASS. The native layer remains a thin projection of `wyrd-client`; exported wrapper options are precise and add `readonly tenant?: string` consistently (for example `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1076-1098`). Test-only human-login helpers remain in `native-testing`; production code does not import the test harness. Generated N-API declarations match the binding changes in the reviewed diff. |
| Rust/Python/TypeScript journey and test helpers | `AGENTS.md` §11 and §16; `agent-rules.md` test placement/runtime ownership; `testing-workflows.md`; language references | PASS. Rust-only logic stays in Rust tests, Python lifetime behavior stays in Python, and Node lifetime behavior stays in TypeScript. Real client-to-server journeys exist on all three first-class SDK surfaces, while provider-dependent selectors are routed through the identity lane. Added external Rust tests earn their placement by starting a server/compiled CLI or crossing multiple crates/processes. Materially changed Rust test helpers and test functions carry substantive rustdoc. |
| Tooling and generated artifacts (`mise.toml`, API docs input, `.pyi`, `.d.ts`, error-code union) | `AGENTS.md` §§8, 11–12, 16; `agent-rules.md` generated-artifact/gate integrity rules; `implementation-execution.md`; `testing-workflows.md` | PASS. Identity selectors prove exactly one named test before execution. Provider-dependent Python/TypeScript journeys are excluded only from lanes that do not provision Keycloak and are added to the provider-backed identity aggregate. Public contract changes are represented in generated surfaces and the task records successful codegen/type/N-API checks. The production-wheel gate is analyzed separately below. |
| Approved spec/task workflow and binding human decisions | `spec-driven-development.md`; `implementation-execution.md`; approved spec revision 7; TASK-004; three supplied human-direction records | PASS for repository standards. The implementation packet names exact focused selectors and repository-managed lanes, retains the conditional issuer rule and real provider sign-in owner from TASK-003, and the new revoke path explicitly scopes logout to one refresh chain (`crates/wyrd/wyrd-server/src/auth/cli_login.rs:173-203`). Task acceptance itself is outside this repository-standards role. |

## Rule-by-rule results

| Repository rule | Result | Evidence |
|---|---|---|
| Durable server behavior remains Rust/server-owned; SDKs share `wyrd-client` | PASS | Renewal/store/selection are implemented once in `crates/shared/wyrd-client`; Python and N-API constructors only pass `tenant` into that owner. Handoff and revoke mutations remain `wyrd-auth`/`wyrd-sql` behavior behind server routes. |
| `wyrd-spec` remains foundational, IO-free, async-free, and PyO3-free | PASS | The added `auth/cli_handoff.rs` contains only typed serde/schema contracts; no forbidden dependency entered its manifest. |
| Struct-centered Rust ownership and narrow async boundaries | PASS | `SavedLogins`, `SavedLoginSource`, `CliLogins`, and `LoginFlow` own their stateful workflows. Free functions are conversions/error constructors/platform browser launch helpers. Async methods await network, SQL, server, signal, or timer IO; parsing and selection remain synchronous. |
| Secrets are typed and redacted | PASS | Handoff verifier, access token, and refresh token use `SecretBearer`; saved-login `Debug` uses redacted secret wrappers; route tracing skips request/proof/token bodies; CLI output prints only origin, tenant, principal, status, and expiry. |
| Stable public errors come from the derive-backed catalog | PASS | `ClientSavedLoginUnusable` was added to `wyrd_spec::error::WyrdError` and projected through the existing central client/Python/TypeScript mappers; no parallel problem-json implementation was introduced. |
| Tenant SQL uses `TenantConn`, RLS, caller-owned transactions, and Postgres coordination time | PASS | New production query signatures and migration satisfy each rule; no query callee commits/rolls back. The only raw privileged pool use added is in Postgres test setup. |
| Public HTTP handlers use typed contracts, scrubbed tracing, stable errors, and OpenAPI registration | PASS | All four added handlers satisfy those shapes and are mounted through the existing auth route owner; `pg_openapi_contract.rs` was extended. |
| PyO3 remains at the Python SDK boundary and the production wheel excludes test-tier native behavior | PASS | Feature gating and the independently executed wheel check are detailed below. |
| Python and TypeScript generated declarations match the public surfaces | PASS | Tenant arguments and the new stable error code appear consistently across binding source, wrappers, `.pyi`, `.d.ts`/`.d.cts`, and the TypeScript literal error-code union. Task evidence reports `codegen:check`, `py:typecheck`, `ts:typecheck`, and `ts:napi:check` green. |
| User-facing capabilities have real first-class-language journeys | PASS | Candidate adds Rust, Python, and TypeScript saved-user-auth journeys plus CLI and concurrent-client journeys; `test:identity:journey:inner` includes all five saved-login targets in addition to existing server/UI identity journeys. |
| Tests remain in the correct runtime/tier and credential-requiring cases stay out of fast lanes | PASS | Python test is marked `identity`; ordinary Python integration selects `integration and not identity`. The TypeScript saved-login test is excluded from ordinary integration and run by the identity target. Rust provider-backed journeys are ignored outside the identity wrapper. |
| No gate was disabled, weakened, hidden by a broad glob, `#[ignore]`, or unjustified `#[allow]` | PASS | Existing non-identity lanes retain their prior tests; the newly provider-dependent tests are added to the provisioning lane rather than silently skipped. Added Clippy allowances carry the required immediate justification and apply only to foreign-runtime boundary shapes. `git diff --check` is clean. |
| New/materially modified Rust items and helpers have substantive rustdoc | PASS | Reviewed new owner modules, fields, methods, private helpers, test helpers, tests, error conditions, and relevant partial-progress/retry semantics. No placeholder or missing material item was found. |
| Dependency cost stays narrow and features are earned | PASS | The only new dependency edge is Unix-only `rustix` in `wyrd-client` for effective-UID ownership checks. Tokio features added to `wyrd-client`/CLI are used by runtime-handle renewal, `spawn_blocking`, signal handling, and polling. No new Cargo feature or broadly propagated server/data dependency was added. |

## Mandatory boundary-check analysis: `check:py-wheel-no-testing`

**Result: PASS — commit `feac127a0` strengthens the check.**

The old task depended on `py:setup`, which installs the SDK with
`maturin develop --features testing`, and then ran `import wyrd.testing` in
that same development environment. Its own setup guaranteed that the forbidden
module was present, so it could not provide a green proof about the production
wheel.

The candidate keeps the setup dependency but no longer trusts that environment:

1. `uv run maturin build --out "$out"` builds from the normal
   `[tool.maturin].features` set (`python`, extension-module, ABI3), not the
   Cargo `testing` feature. `testing` remains a separate non-default feature
   whose `wyrd-testing`, tracing, and URL dependencies are optional
   (`sdks/wyrd-sdk-python/Cargo.toml:21-34`). Thus the artifact under test is
   the default production wheel rather than the editable testing build.
2. The produced wheel path is injected into `uv run --isolated --no-project`.
   That prevents dependency/project resolution from reusing the testing-enabled
   editable installation created by `py:setup`; the checked import comes from
   the explicitly supplied wheel (`mise.toml:1255-1260`).
3. `import wyrd` is a positive control (`mise.toml:1259`). With `set -e`, a
   failed build, absent/ambiguous wheel glob, failed installation, or broken
   production package exits nonzero before the negative assertion. Therefore a
   missing/broken wheel cannot masquerade as successful exclusion.
4. `import wyrd.testing` is the negative boundary assertion
   (`mise.toml:1260-1263`). If the default native module exposes the testing
   submodule, the public package import succeeds and the task explicitly exits
   1. The candidate native registration is still guarded by
   `#[cfg(feature = "testing")]` (`sdks/wyrd-sdk-python/src/lib.rs:19-20,136-141`),
   so the default wheel fails that import for the intended reason.
5. Failure behavior is conservative: shell/build/install/root-import failures
   fail the task; only absence of the test-tier public import passes it. The
   check neither removes the positive production import nor broadens an
   allowlist/glob to hide content.

I independently ran `mise run check:py-wheel-no-testing` on the immutable
candidate. It built the testing-enabled editable setup, then separately built
the default wheel in a temporary directory, installed/imported that wheel in
the isolated environment, rejected `wyrd.testing`, and exited 0. This directly
confirms the isolation and default-feature behavior instead of relying only on
the implementation record.

## Open Questions

None affecting repository-rule compliance.

## Verification Notes

Independently executed during this review:

- `git diff --check 06f134dc14164c040c0e5014d21de29c240f4116..7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84` — PASS.
- `mise run check:py-wheel-no-testing` — PASS (20.25 seconds), including
  successful positive `import wyrd` from the isolated default wheel.
- Static scan of every added/materially changed Rust production signature and
  field for forbidden raw SQL pool/connection/transaction propagation — no
  violation found.
- Static scan of added production Rust for `unwrap`, unjustified `allow`,
  function-scoped imports, and secret-bearing tracing/output — no violation
  found.

The implementation record reports the full identity aggregate and every
focused target, Rust family/SDK lanes, Python and TypeScript unit/integration/
typing/N-API lanes, code generation, client/PyO3/workspace boundaries, served
OpenAPI contract, formatting, and lint lanes as green. Those results were
available as review evidence but were not all re-executed by this independent
standards pass. That is a verification limit, not a standards finding: source
inspection found the recorded selectors and lane wiring intact, and the one
explicitly challenged boundary gate was rerun independently.

## Overall result

**PASS**
