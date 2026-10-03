# TASK-012 round 2 repository-standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`

The candidate remained at the named commit throughout this review. The review
covers the complete base-to-candidate range and uses the latest remediation diff
only to locate the owners changed for round 2.

## Authority coverage

| Changed surface | Applicable authority | Coverage |
|---|---|---|
| `wyrd-client` OAuth owner (`auth.rs`) | `AGENTS.md` §§3–6, 9, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` client model and runtime identity; `architecture/wyrd-security-posture.md` access/refresh tokens and secret handling; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/maintainer-style.md` | Inspected the complete changed owner, adapter, device/refresh/revocation/form paths, middleware callers, errors, secret projections, async/cancellation contracts, tests, and public API shape. |
| Shared HTTP target and transport (`transport/config.rs`, `transport/http.rs`, `saved_login.rs`) | Same Rust/client authorities; security posture URL/credential rules; `AGENTS.md` client ownership and documentation rules | Inspected normalized-origin production and consumer paths, same-origin enforcement, saved-login selection, diagnostics, changed tests, and rustdoc. |
| CLI login and refresh (`wyrd-cli/src/auth/*`) | `AGENTS.md` §§3–6, 9, 11, 16; Wyrd design/doctrine public-surface and shared-client rules; security posture credentials; maintainer style | Inspected browser launch, `--no-browser`, device polling, refresh dispatch, local-first logout consumer path, secret exposure, and the removed private launcher/poller. |
| Journey and test support (`wyrd-cli/tests/cli_login_journey.rs`, `wyrd-testing/src/human_login.rs`, `wyrd-client/tests/transport/http.rs`) | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement/documentation rules; `architecture/references/languages/testing-workflows.md`; `architecture/references/languages/spec-driven-development.md` test-command precision | Inspected the changed journey, fixture helper, external-test justification, exact selectors, and recorded narrow evidence. |
| Workspace/crate manifests, lockfile, workspace-hack | `AGENTS.md` §§1, 2, 4, 15; `architecture/agent-rules.md` feature-cost rule; client ownership authorities | Inspected versions, default features, dependency placement, lockfile resolution, and workspace-hack update. |
| Active task and prior review/remediation artifacts | `AGENTS.md` §§11, 14; `architecture/references/languages/spec-driven-development.md` | Inspected original and remediation task authority, exact-selector records, verification scope, and candidate identities. |

`architecture/bifrost-design.md`, Vala domain references, PyO3, Python-stub,
TypeScript, SQL-tenancy, server-handler, OpenAPI, and MCP authorities do not
govern this write set: no corresponding production, binding, schema, SQL,
server, or generated surface changed.

## Per-rule results

| Rule | Result | Source evidence |
|---|---|---|
| Shared client ownership; no language-specific OAuth implementation | PASS | OAuth transport and lifecycle remain on `TokenExchange`/`AuthMiddleware` in `crates/shared/wyrd-client/src/auth.rs`; CLI and `wyrd-testing` call that owner; no `sdks/*` source changed. |
| Struct-centered Rust workflows | PASS | `TokenExchange` owns the normalized origin, redirect-free client, and configured `oauth2::BasicClient`; `LoginFlow` owns CLI orchestration; saved-login locking remains on `SavedLogins`. New free functions are narrow conversions/error mappings. |
| Async only at IO/composition boundaries | PASS | Changed async functions await HTTP, OAuth polling, subprocess/signal, or file-operation composition. URL normalization and response conversion remain synchronous. |
| Use vetted, conventional OAuth/browser libraries without parallel grant machinery | PASS | Device and refresh use `oauth2 5.0.0`; the public arbitrary `TokenExchange::exchange` is gone; the private form owner is reachable only from RFC 8693/RFC 7523 producers, plus the lead-approved one-form-POST RFC 7009 exception. `webbrowser 1.2.4` owns browser launch. No WSL mechanism or configurable launcher was added. |
| Redirect and secret handling | PASS | `AuthHttp` is built with `reqwest::redirect::Policy::none()`; `HttpConfig::validate` refuses userinfo without echoing it; `TokenExchange` and `HttpTransport` debug only normalized `HttpsOrigin`; refresh/device values remain secret wrappers. |
| One parsed effective target shared by auth and HTTP transport | PASS | `HttpConfig::validate` returns `HttpsOrigin`; `TokenExchange::new`, `HttpTransport::new`, and `canonical_origin` consume it. `authenticated_url` reuses `HttpsOrigin::of_url` for absolute URLs. |
| Public and private Rust documentation, including fallible functions, tests, and changed methods | **FAIL** | See `REPO-R2-001`. Several cumulative changed items still have missing required sections or no rustdoc, and the public `HttpConfig` rustdoc describes behavior the candidate no longer has. |
| Crate-local errors, stable public projections, and no production `unwrap()`/unjustified `allow` | PASS | OAuth transport failures remain `WyrdClientError`/`AuthError`; server OAuth refusals preserve existing mapping. The cumulative production diff adds no `unwrap()` or `#[allow]`. |
| Dependency ownership and feature economy | PASS | `oauth2 = =5.0.0` has `default-features = false` and is owned by `wyrd-client`; `webbrowser = =1.2.4` is owned by `wyrd-cli`; no new Cargo feature or profile block was added; workspace-hack and lockfile were updated. |
| Test placement and tiering | PASS | Auth/config tests remain inline; the CLI journey starts a real server and therefore earns an external integration target; the existing transport integration target crosses the shared-client/HTTP seam. No test was ignored or weakened to bypass a gate. |
| Exact named-test commands and narrow remediation verification | PASS | TASK-012 lines 237–248 record all seven required exact selectors, each as one selected/one passed. The R1 evidence records the filtered CLI journey and only write-set static/boundary lanes; no aggregate is required by this review. |
| Generated artifacts and public language declarations | N/A | No generated schema, Python stub/package, TypeScript declaration/package, OpenAPI, or MCP catalog source changed. |
| No legacy vocabulary, compatibility route, task/agent note in production, or speculative check/setting | PASS | The production diff adds none. The remediation adds no parser, compatibility path, setting, launcher, WSL branch, permanent source check, or extra protocol layer. |

## Review findings

### Important

#### REPO-R2-001 — mandatory documentation remains incomplete and partly contradicts the implemented target/revocation behavior

- **Violated rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require
  substantive rustdoc for every new or materially modified Rust item, including
  private methods and tests; every fallible function needs `# Errors`, and
  remaining panic behavior needs `# Panics`. Documentation must explain the
  actual workflow and invariants.
- **Locations and evidence:**
  - `crates/shared/wyrd-client/src/auth.rs:637-643` materially changes
    `AuthMiddleware`'s `Debug::fmt` to print the normalized origin, but the
    changed method has no rustdoc explaining the redacted diagnostic boundary.
  - `crates/shared/wyrd-client/src/transport/http.rs:131-136` materially changes
    `HttpTransport::fmt` for the same normalized-origin behavior, but the
    changed method has no rustdoc.
  - `crates/shared/wyrd-client/src/transport/config.rs:352-359` adds the
    fallible `origin` test helper without the mandatory `# Errors` section.
    The materially changed tests at lines 361–410 use `expect_err`/`expect` but
    omit `# Panics`; `auth.rs:1809-1850` likewise materially changes the
    redirect test, which contains `expect`/`expect_err` and assertions but has
    no `# Panics` contract.
  - `crates/shared/wyrd-client/src/transport/config.rs:146-165` still documents
    `base_url` as a common route prefix to which ingest paths are appended. The
    changed `validate` implementation at lines 202–244 instead deliberately
    reduces it to an origin and drops every supplied path, query, and fragment.
    The public configuration rustdoc therefore contradicts its implemented
    contract.
- **Consequence:** the candidate does not satisfy the repository's explicit
  hard documentation gate. A maintainer reading the public config may believe
  a path prefix is preserved, while the implementation sends credentials and
  API calls to root routes; the changed debug and test items also remain below
  the required maintenance contract.
- **Testable correction:** update only these existing docs: document both
  changed `fmt` methods as emitting only the userinfo-free normalized origin;
  add the required `# Errors`/`# Panics` sections to the changed helper/tests;
  and make `HttpConfig`/`base_url` state that it names a deployment URL whose
  origin is retained while path/query/fragment are discarded. No new helper,
  check, test, option, or abstraction is warranted. Source inspection plus the
  existing narrow format and `wyrd-client`/`wyrd-cli` lint command is sufficient
  closure proof.

## Non-blocking note

- `Cargo.toml:101-103` says the `oauth2` dependency owns device, refresh, and
  revocation grants. That changed comment is stale: the locked RFC 7009 path is
  the single form POST through the redirect-free adapter because
  `oauth2 5.0` rejects loopback HTTP revocation URLs. Delete “and revocation”
  or otherwise describe only the dependency's actual ownership. This does not
  reopen the approved exception and does not justify a separate round.

## Verification assessment

The candidate records these successful narrow results: the filtered
`cli_device_login_journey`; the seven exact named selectors; `mise run fmt`;
the two-package all-target/all-feature Clippy command; `check:client-tier`;
`check:cli-client-tier`; `check:unwrap-audit`; and `git diff --check`. Earlier
cumulative evidence also records the focused Rust/Python/TypeScript saved-login
journeys and the required dependency/codegen boundaries. The accepted Windows
proof is `cargo check -p webbrowser --target x86_64-pc-windows-msvc`; this host
has no Windows C cross-compiler, so no full `wyrd-cli` Windows target is
required. PLAT-001 remains rejected.

This review independently ran `git diff --check` for the immutable range and it
passed. It did not start another Cargo or journey process in the shared
checkout. Full journey sweeps and aggregate gates are intentionally deferred to
change review under the human verification direction.

## Overall result

**FAIL**

`REPO-R2-001` is a bounded repository-rule violation. The implementation and
recorded verification otherwise conform to the applicable repository
authorities.
