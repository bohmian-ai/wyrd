# TASK-009 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa2`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Candidate was still `HEAD` when this report was completed.
- Scope: repository-rule compliance only. Task acceptance and other reviewers' conclusions were not reviewed here.

## Authority coverage

| Changed surface | Applicable authority | Coverage |
|---|---|---|
| Workspace dependency, lockfile, and generated Hakari feature union | `AGENTS.md` §§1, 4, 11, 12, 15–16; `architecture/agent-rules.md` Cargo-feature rule; `architecture/references/languages/testing-workflows.md`; task's approved `openidconnect = 4.0.1` decision | Reviewed workspace and crate manifests, lockfile, `workspace-hack`, `mise.toml`, Cargo feature tree, and recorded verification. |
| `wyrd-auth-oidc` relying party, screened HTTP adapter, metadata/JWKS cache, and crate-local errors | `AGENTS.md` §§3–6, 9–10, 12, 15–16; `architecture/agent-rules.md` SSRF, async, struct-owner, error, and rustdoc rules; `architecture/wyrd-security-posture.md` Delegation and federation + Source credentials and SSRF defense; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md`; OIDC Core, Discovery, RFC 7636, RFC 9207 as selected by the approved task | Reviewed all new production code and its in-module tests, manifest features, exports, and callers. |
| Tenant login, callback, candidate testing, connection lifecycle, and human-session issuance | `AGENTS.md` §§3, 5–6, 9–12, 16; `architecture/agent-rules.md` tenancy, audit, struct-owner, async, SSRF, and rustdoc rules; `architecture/wyrd-design.md` Runtime identity; `architecture/wyrd-security-posture.md` Delegation and federation; `architecture/references/architecture/patterns.md` Server/Audit/External network patterns | Reviewed `login.rs`, `callback.rs`, `connections.rs`, errors, constructors, SQL capability types, audit paths, and relevant tests. |
| Platform login and server composition | Same server/security/identity authorities, plus `architecture/references/languages/rust-core.md` shared-client/cache lifecycle rules | Reviewed `PlatformLogin`, its server construction on begin/callback, platform identity handler changes, and platform tests. |
| Workload issuer boot and administrative discovery consumers | `AGENTS.md` §§3, 5–6, 9–12, 16; `architecture/agent-rules.md` SSRF/tenant/async rules; `architecture/references/architecture/patterns.md` Server and External network patterns; `architecture/wyrd-security-posture.md` external federation | Reviewed boot seeding, admin/platform connection creation, their discovery callers, tests, and `TenantConn`/`OperatorPool` use. |
| `wyrd-auth-verify` workload path | `AGENTS.md` ownership/error/testing rules; `architecture/wyrd-security-posture.md` external federation; approved non-goal retaining workload RFC 7523 on `ExternalVerifier` | Reviewed removal of the human-ID-token method and preservation of workload verification and coverage. |
| Server, CLI, shared, and first-class-language identity journeys and fixtures | `AGENTS.md` §11; `architecture/agent-rules.md` test placement, exact commands, and journey rule; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/implementation-execution.md` | Reviewed changed fixtures/tests, task verification contract, and recorded lane evidence. |
| Active task evidence | `architecture/references/languages/spec-driven-development.md` task/evidence contract; `architecture/references/languages/implementation-execution.md` focused verification and completion standard | Reviewed the evidence added at candidate lines 201–265. |

`architecture/wyrd-doctrine.mdx` was also checked for public-surface and service-boundary drift. Bifrost, PyO3, Python source, TypeScript source, schemas, storage, and analytical authorities are not changed by this range.

## Rule results

| Rule | Evidence | Result |
|---|---|---|
| Use the approved vetted relying-party library and do not add provider-specific behavior | Workspace pins `openidconnect = "=4.0.1"`; the new owner calls library discovery, PKCE, code exchange, and ID-token verification; no provider-name branch was added. | PASS |
| Keep the dependency in its narrow owner and do not enable `oauth2`'s `reqwest` feature | Only `wyrd-auth-oidc` consumes the workspace dependency; `default-features = false`; `cargo tree --locked -e features -i oauth2` shows only `openidconnect -> oauth2` and no enabled `oauth2` feature. | PASS |
| Every provider fetch is screened, DNS-pinned, bounded, proxy-free, and redirect-disabled | `ScreenedHttp` implements `AsyncHttpClient`; every library request routes through `client_for` and `read_bounded_body`; token redirect tests assert that the second origin is untouched. | PASS |
| Stateful IO workflows have a concrete owner and long-lived dependencies/caches are composed through owner state | `HumanConnections` correctly owns a long-lived `RelyingParty`; platform and admin paths rebuild it on request paths. See `REPO-TASK-009-1`. | FAIL |
| Tenant SQL uses `TenantConn`; cross-tenant platform work uses `OperatorPool`; callees accepting `&mut TenantConn` do not commit | Changed tenant paths acquire `TenantConn`; `PlatformLogin` retains `OperatorPool`; no raw pool was introduced into production library signatures and no changed `&mut TenantConn` callee commits. | PASS |
| Authorization decisions retain the canonical transactional audit behavior | Callback/candidate/platform changes reuse the existing audit owners; no second writer, audit sink, or unaudited permission decision was added. | PASS |
| Secrets use secret-bearing types and do not reach `Debug`, public errors, or response details | Codes, PKCE verifiers, provider client secrets, and sessions remain `SecretString`/sealed; custom `Debug` implementations omit secret/cache state; provider causes are logged server-side and mapped to stable `WyrdError` variants. | PASS |
| Public errors use the stable Wyrd catalog; crate-local errors use `thiserror` | `RelyingPartyError`/`ProviderHttpError` remain crate-local and map at the server boundary through existing `WyrdError` variants. No parallel public error catalog was added. | PASS |
| Async is restricted to actual IO/composition; pure validation remains synchronous | Discovery, token exchange, SQL, and HTTP operations are async; claim checks, error classification, URL conversion, and role mapping are synchronous. | PASS |
| New/materially modified Rust items have accurate rustdoc, including private helpers | The insertion in `connections.rs` attached the old `not_tested_reason` description to `require_usable_jwks` and left `not_tested_reason` undocumented. See `REPO-TASK-009-2`. | FAIL |
| New user-facing behavior has real-server journey coverage | The task records the unfiltered identity lane across server/UI/CLI/Rust/client/Python/TypeScript plus supporting shared, SDK, CLI, principals, Python, TypeScript, and Wyrd lanes. | PASS |
| Every specifically named changed test is run and recorded with its exact focused selector | The evidence names multiple new unit and journey tests but records only aggregate lanes, not their required exact selectors/listing commands. See `REPO-TASK-009-3`. | FAIL |
| Generated artifacts are regenerated by their owner and verified | Hakari generated only `workspace-hack` feature-union entries and lockfile edges; `check:workspace-hack` subsequently passed. No generated schema/stub was hand-edited. | PASS |
| No unapproved compatibility, provider-specific, hosted-only, SAML, or SCIM mechanism entered the diff | None found. The changed CLI device-secret helper is a direct replacement for a deleted shared random helper and uses the standard existing primitives. | PASS |
| Human standing direction: do not retain or require a mechanism/check/file/setting/option absent from standards and comparable projects | The approved library transport/cache and RFC 9207 check are grounded in the task/research/standards; this review requires no novel security mechanism or setting. Findings below ask only for repository-native ownership, documentation, and mandated proof. | PASS |

## Material findings

### REPO-TASK-009-1 — Per-request construction defeats the new relying-party cache and violates server composition rules

- **Rule:** `AGENTS.md` §5 requires dependency-backed workflows to live on a dependency-owning concrete struct; `architecture/references/architecture/patterns.md` says handlers avoid constructing clients; `architecture/references/languages/rust-core.md` says to keep heavy shared dependencies in explicit state and avoid rebuilding clients or caches per request.
- **Locations:** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:652-669`, `crates/wyrd/wyrd-auth/src/platform_login.rs:111-122`, and `crates/wyrd/wyrd-server/src/components/admin/routes.rs:750-755`.
- **Evidence:** Both platform login handlers call `login_service`, which creates a new `PlatformLogin`; its constructor creates a new `RelyingParty` and empty `moka` cache. Begin and callback therefore never share the cache the new service claims to own. Administrative discovery also creates and drops a new `RelyingParty` inside the request path.
- **Consequence:** The cache has request lifetime on platform/admin paths, so ordinary begin-to-callback flow performs discovery/JWKS IO again and the server rebuilds a dependency-backed capability for each request. This contradicts the new owner's documented lifecycle and the repository's explicit composition rule.
- **Required correction:** Compose the existing `RelyingParty` capability at long-lived server/auth state and inject or clone that owner into platform login and administrative discovery, preserving the same screened transport and fresh `discover` semantics where a create operation requires them. Do not add a second cache type, setting, or alternative transport. Prove that separate platform begin/callback handler invocations reuse the same provider cache and that admin discovery still performs its required fresh screened fetch.

### REPO-TASK-009-2 — A changed private helper lost its required rustdoc

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require accurate rustdoc on every new or materially modified Rust item, including private helpers; missing documentation is `BLOCK_BEFORE_MERGE`.
- **Location:** `crates/wyrd/wyrd-auth/src/connections.rs:936-954`.
- **Evidence:** `/// A failed test check with a stable machine-readable reason.` now documents `require_usable_jwks`, although the sentence describes `not_tested_reason`; `not_tested_reason` itself has no rustdoc. The insertion created both the inaccurate combined comment and the undocumented helper.
- **Consequence:** A maintainer is told the JWKS validator is an error-construction helper and receives no documentation at the actual stable-reason constructor. Green lint/docs lanes do not waive the repository's explicit source requirement.
- **Required correction:** Give `require_usable_jwks` only its JWKS-validation documentation and restore the stable-reason description directly on `not_tested_reason`, including its workflow role. No new helper or check is needed.

### REPO-TASK-009-3 — The candidate does not record the mandatory exact focused test runs

- **Rule:** `AGENTS.md` §11, `architecture/agent-rules.md`, and `architecture/references/languages/spec-driven-development.md` require every specifically named Rust/Python/TypeScript test in a task artifact or implementation report to include and run its exact focused repository-native command. The task repeats that obligation at lines 162–168.
- **Location:** `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md:208-245`.
- **Evidence:** Lines 211–216 specifically name the new relying-party and identity-journey tests, while the lane record lists only aggregate `mise run ...` tasks. It records neither exact `mise exec -- cargo nextest ... -E 'test(=...)'` commands for the library tests nor the required `WYRD_IDENTITY_FILTER=<test>` single-run commands/listing proof for the named server journeys.
- **Consequence:** The aggregate lanes provide broad evidence but do not satisfy the repository's zero-selection-safe, exact-test proof requirement; a reviewer cannot establish from the durable record that each named negative/edge test was selected and executed.
- **Required correction:** Run and record the exact selectors for every specifically named new or changed test, using the task's identity setup wrapper and the explicit package/target/test expression for Rust library tests. Keep the already-recorded aggregate lanes; do not add a new harness or weaken the requirement to another aggregate.

## Verification assessment

The recorded broad lanes cover the changed Rust owners, identity journeys, language consumers, contracts, docs, formatting, lints, and relevant boundaries. I additionally inspected the candidate feature graph and confirmed that `oauth2` has no `reqwest` feature enabled.

The post-`9ef532660` reruns of `mise run lints` and `mise run check:workspace-hack` are sufficient **for that regeneration itself**. The commit changes only Hakari's generated feature-union dependencies and corresponding `Cargo.lock` edges; it changes no dependency version or production source. The earlier affected tests already compiled `openidconnect` with the crypto features it requests directly, while the rerun all-feature/all-target Clippy compiled the final unified feature graph and the Hakari check proved the generated file matches it. Repeating every behavioral lane solely because of that generated cache-unification update is not required by the repository's narrowest-complete-verification rule.

That conclusion does not close `REPO-TASK-009-3`: the exact focused commands were required before the Hakari regeneration as part of the task's own verification contract and are absent from the candidate evidence.

## Overall result

**FAIL**

Repository ownership/lifecycle, mandatory rustdoc, and exact-test-proof rules remain unsatisfied. No additional nonstandard hardening mechanism or setting is required to correct them.
