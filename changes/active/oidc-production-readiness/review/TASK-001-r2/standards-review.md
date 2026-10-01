# Repository Standards Review — TASK-001 r2

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Range reviewed: the complete `base..candidate` diff (66 files, including Rust contracts, auth/server/SQL/crypto/runtime code, a migration, tests, generated TypeScript error codes, docs, fixtures, and `mise.toml`)
- Candidate identity at review start and completion: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- CodeGraph: not used because the repository has no `.codegraph/` directory.

This review did not use the conclusions of the prior TASK-001 review reports and does not judge task acceptance.

## Authority coverage

| Changed surface | Applicable authority read | Coverage |
|---|---|---|
| Repository-wide structure, documentation, dependencies, verification | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/implementation-execution.md` | Complete for changed surfaces |
| Public auth contracts and stable errors | `architecture/wyrd-design.md` (doctrine, client model, runtime identity, auth); `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md` | Complete |
| Human federation, secrets, SSRF, tenant identity, production composition | `architecture/wyrd-security-posture.md` (principal lifecycle, delegation/federation, source/SSRF defense, tenant isolation, cryptography/secret handling) | Complete |
| Rust ownership, async, SQL boundaries, docs | `architecture/references/languages/rust-core.md`; `AGENTS.md` §§4–6, 9, 15–16 | Complete |
| Tenant SQL, migration, RLS, concurrency | `architecture/agent-rules.md`; `architecture/references/architecture/patterns.md` storage pattern; `AGENTS.md` §15 | Complete |
| HTTP/OpenAPI and TypeScript generated error projection | `AGENTS.md` §§8–9, 11; `architecture/references/languages/errors.md`; `architecture/references/languages/typescript-guide.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| Real-server journeys, SQL migration tests, fixtures, mise selection | `AGENTS.md` §11; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` | Complete |
| Docs changes | `AGENTS.md` §§11–12; applicable authentication/security authorities above | Complete |
| Bifrost, PyO3, Python, UI, MCP | No materially changed implementation surface | Not applicable |

## Rule results

| Rule | Evidence | Result |
|---|---|---|
| Durable contracts belong in `wyrd-spec`; durable orchestration stays in server owners | `wyrd-spec/src/auth/human_connection.rs` owns typed wire shapes; `wyrd-auth::connections::HumanConnections` owns the lifecycle; server handlers are adapters; `wyrd-sql` owns statements/migration | PASS |
| Stateful workflows use a cohesive concrete owner | `HumanConnections` owns Postgres, keyring, screened HTTP, and callback origin; `SealedSecretRewrap` owns cross-tenant rewrap | PASS |
| `wyrd-spec` remains IO-, async-, database-, and PyO3-free | New contract module contains only validation and wire/domain values | PASS |
| Tenant SQL uses `TenantConn`, relies on RLS, and callees do not commit | New tenant query functions take `&mut TenantConn<'_>`; migration enables and forces RLS; transaction lifecycle remains in `HumanConnections` | PASS |
| Cross-tenant secret maintenance uses explicit operator authority | Rewrap queries take `&OperatorPool`, are annotated as cross-tenant, and use compare-and-swap | PASS |
| Raw `PgPool` is banned from library struct fields/signatures | The materially modified `PgIssuerResolver` still stores `Arc<PgPool>` and accepts it in `new` | **FAIL — STD-R2-001** |
| Public HTTP request/response bodies are typed structs | Five new handlers use typed bodies; `put_candidate` uses `Json<Value>` while advertising `ConnectionInput` in OpenAPI | **FAIL — STD-R2-002** |
| New/materially modified Rust items and fallible functions have complete rustdoc | Most new modules are well documented, but touched `pg_resolvers` items omit required item/field/variant docs and `# Errors` | **FAIL — STD-R2-003** |
| All `use` statements are at module top, except the two stated narrow exceptions | New proxy test imports `wiremock` from inside the test function | **FAIL — STD-R2-004** |
| File-mounted secrets require restrictive permissions | New retained-key file loader accepts any readable file without checking restrictive permissions | **FAIL — STD-R2-005** |
| Public errors use the derive-backed catalog and project to generated TypeScript codes | Four new `WyrdError` variants carry `#[wyrd_error(...)]`; generated `error-codes.ts` contains them and declares itself generated | PASS |
| Server handlers are instrumented, derive tenant from authenticated caller, and expose typed problem responses | All six handlers use `#[tracing::instrument(skip_all,...)]`, take `Caller`, use its tenant, and return the central `WyrdErrorResponse` | PASS, subject to STD-R2-002 |
| Authorization decisions use the canonical audit path and fail closed where required | Handlers use the existing audit owner; connection mutations append through the tenant transaction; the probe's two permission evaluations have focused failure tests | PASS |
| Tenant-controlled provider URLs are resolved, screened, pinned, redirect-disabled, and proxy-free | Provider requests reuse `ScreenedHttp`; `.no_proxy()` closes the ambient proxy bypass; a focused child-process test proves it | PASS |
| Secrets use redacted wrappers and are not exposed by views | Inputs use `SecretBearer`/`SecretString`; views omit ciphertext/key ids; keyring/debug implementations redact material; OpenAPI journey checks view properties | PASS, subject to STD-R2-005 |
| Async exists only at real IO/composition boundaries | Added async methods await SQL, HTTP, process, or server IO; pure parsing/validation remains synchronous | PASS |
| Public capability has a real client/server journey and supporting integration/unit proof | `identity_e2e` adds tenant administration, rotation, replica/session cutoff journeys; OpenAPI and migration integration tests cover their seams; mise exact-selection guard prevents zero-test success | PASS |
| Generated artifacts are source-derived and checked | TypeScript error union is marked generated; candidate evidence records `codegen:check` and served OpenAPI verification | PASS (recorded evidence; not independently rerun) |
| No gate weakening, unjustified `allow`, or production unwrap/expect was added | Executed `check:unwrap-audit` and `check:clippy-allow-audit`; both passed. The changed identity task removes test `--all-features`, which matches the repository test-feature rule rather than weakening selection | PASS |
| Applicable boundary/tenant checks pass | Independently executed `check:tenant-isolation`, `check:client-tier`, `check:from-pools-allowlist`, and `check:no-legacy-server-vocab`; all exited 0 | PASS, but existing checks do not cover STD-R2-001/002/003/004/005 |
| Documentation and verification evidence are present | Authentication/SSO docs and task evidence describe connection configuration, rotation, focused journeys, SQL, codegen, docs, format, and lint results | PASS as recorded evidence |

## Material findings

### STD-R2-001 — Raw pool remains in a materially modified library owner

- Violated rule: `architecture/agent-rules.md` permits no raw `sqlx::PgPool` in library struct fields or function signatures; production pool use must go through the owning Wyrd connection boundary. `AGENTS.md` §5 also requires explicit dependency-owning handles to follow the repository composition pattern.
- Location: `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:64-73`, with direct acquisition at `:95` and the corresponding `trusted_issuer` path.
- Evidence: this candidate materially changes `PgIssuerResolver` from a single sealing key to `SealingKeyring`, but the touched owner still stores `pool: Arc<PgPool>`, its constructor accepts `Arc<PgPool>`, and methods call `TenantConn::acquire(&self.pool, ...)`. The adjacent new `HumanConnections` owner already demonstrates the sanctioned `WyrdPostgres` composition.
- Consequence: the production resolver keeps a library-owned raw pool path outside the repository's connection owner, so future pool/role/tenant acquisition invariants can drift independently and the explicit raw-pool boundary remains violated.
- Testable correction: make the materially modified resolver own/reuse the existing `WyrdPostgres` runtime handle and acquire tenant connections through that owner; update its construction sites. Prove the resolver tests and auth/server compile lanes, then statically confirm no raw pool remains in the resolver field or constructor.

### STD-R2-002 — Candidate PUT route uses an untyped JSON request body

- Violated rule: `AGENTS.md` §9 and `architecture/references/architecture/patterns.md` Server Pattern require public request bodies to be typed structs and the OpenAPI/wire contract to be the actual boundary.
- Location: `crates/wyrd/wyrd-server/src/components/admin/identity.rs:151-181` and `:182-187`.
- Evidence: OpenAPI declares `request_body = ConnectionInput`, but the actual Axum extractor is `Json<Value>` and only later calls `ConnectionInput::from_json`.
- Consequence: the served schema is not the handler's compile-time request shape; extraction/validation semantics can diverge from `ConnectionInput` without a compiler-visible boundary, which is precisely the untyped public-body path the rule prohibits.
- Testable correction: use a typed boundary that still preserves the required stable `UNSUPPORTED_CLIENT_AUTH` refusal for `PrivateKeyJwt` (for example, a typed boundary conversion rather than a handler-level `Value`). Keep the existing served OpenAPI assertions and add/retain HTTP proof for both a valid request and the stable unsupported-auth error.

### STD-R2-003 — Hard-blocking rustdoc omissions remain in touched resolver code

- Violated rule: `AGENTS.md` §16 and `architecture/references/languages/rust-core.md` Documentation require rustdoc on every new or materially modified item, including private fields and enum variants; every fallible function requires `# Errors`. Missing documentation is explicitly a hard blocker.
- Location: `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:65-66` (touched fields), `:246-264` (materially changed/exposed `IssuerDecodeError` variants), `:357-365` (`open_secret` lacks `# Errors`; modified `decode_secret` has no rustdoc or `# Errors`).
- Evidence: these items changed as part of the keyring and human-connection decoder work but lack the required item-level documentation. Compilation and Clippy do not enforce this repository-specific rule.
- Consequence: the exact failure and secret-decoding invariants of a security-sensitive boundary are undocumented, and the candidate fails a repository-declared `BLOCK_BEFORE_MERGE` condition despite green executable checks.
- Testable correction: add substantive rustdoc to each touched field, variant, and helper, including `# Errors` for the fallible helpers; verify with diff inspection plus the normal format/lint lane. Do not add docs to unrelated untouched items.

### STD-R2-004 — New test hides dependencies in function-local imports

- Violated rule: `architecture/agent-rules.md` requires all `use` statements at module top; the test-module exception permits `use super::*`, not arbitrary function-local imports.
- Location: `crates/shared/wyrd-auth-oidc/src/screening.rs:310-312`.
- Evidence: `an_ambient_proxy_cannot_observe_a_screened_request` imports `wiremock::matchers` and `wiremock` inside the function.
- Consequence: the test module's dependency surface is not visible in its top-level import block and directly violates the repository's mandatory module-shape rule.
- Testable correction: move the two imports to the `#[cfg(test)] mod tests` import block and run the focused screening test plus format/lints.

### STD-R2-005 — Retained sealing-key files are accepted without restrictive permissions

- Violated rule: `architecture/wyrd-security-posture.md` §Cryptography and secret handling requires file-mounted secrets to have restrictive permissions and atomic replacement.
- Location: `crates/wyrd/wyrd-server/src/config.rs:3436-3472`; operator-facing documentation at `docs/src/content/docs/self-hosting/authentication.svx:69-76` names the file inputs without a permission requirement.
- Evidence: `load_sealing_retained_keys` calls `std::fs::read_to_string` on an arbitrary configured path and performs no metadata/mode check. The new retained keys are decryption-capable production key material, not public identifiers.
- Consequence: a world/group-readable retained-key file is accepted at boot, weakening the repository's explicit secret-file boundary during the rotation window.
- Testable correction: reuse one narrow secret-file loading/permission-validation path for the sealing key family (and the existing signing-key file where the same authority applies), reject permissive modes before reading/using the key, and document atomic replacement. Add a focused config test proving a restrictive file is accepted and a permissive file is refused on supported deployment platforms.

## Verification evidence and limits

Independently executed, all exit 0:

1. `git diff --check base..candidate`
2. `mise run check:tenant-isolation`
3. `mise run check:unwrap-audit`
4. `mise run check:clippy-allow-audit`
5. `mise run check:client-tier`
6. `mise run check:from-pools-allowlist`
7. `mise run check:no-legacy-server-vocab`

The candidate task records successful focused identity journeys, full identity and platform journeys, the migration preflight and SQL lane, served OpenAPI tests, `codegen:check`, docs, format, and workspace lints. I inspected their source and mise selection mechanics but did not rerun Cargo-, container-, identity-provider-, docs-build-, or codegen-heavy lanes in this Wave 1 review. No claimed result from a prior review report was used. `check:from-pools-allowlist` emitted an `rg: python/: No such file or directory` diagnostic but exited 0; its documented scope is `from_pools` construction sites and it does not cover the raw `PgPool` violation in STD-R2-001.

## Overall result

**FAIL**

The candidate generally follows the Wyrd owners, tenancy model, security composition, stable-error path, and journey-test architecture, but five material repository-rule violations remain. STD-R2-003 is independently a repository-declared hard blocker.
