# TASK-009 R2 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- `FIND-TASK-009-5` is withdrawn by `review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md` and was not reopened.
- Scope: repository-rule compliance only. Task acceptance and Ponytail validation belong to their independent reviewers.

The candidate remained at the stated commit before this report was written.

## Authority coverage

| Changed surface | Applicable authority | Coverage |
|---|---|---|
| Workspace dependency, lockfile, and generated Hakari feature union | `AGENTS.md` §§1, 4, 11–12, 15–16; `architecture/agent-rules.md` Cargo-feature rule; `architecture/references/languages/testing-workflows.md`; approved `openidconnect = 4.0.1` decision | Reviewed workspace/crate manifests, lockfile edges, `workspace-hack`, feature graph, and recorded verification. |
| `wyrd-auth-oidc` relying party, screened transport adapter, provider cache, discovery-only workload metadata read, and crate-local errors | `AGENTS.md` §§3–6, 9–10, 12, 16; `architecture/agent-rules.md` SSRF, async, owner, errors, and rustdoc rules; `architecture/wyrd-security-posture.md` federation and SSRF sections; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Reviewed the full production module, exports, callers, tests, transport routing, cache ownership, and error mapping. |
| `wyrd-auth-verify` workload verifier after removal of the human ID-token path | `AGENTS.md` §§3–6, 10, 12, 16; `architecture/agent-rules.md` ownership and rustdoc rules; `architecture/wyrd-security-posture.md` external federation; `architecture/references/languages/rust-core.md` | Reviewed the removed method/tests, remaining callers, workload ownership, and adjacent public documentation. |
| Tenant login, callback, connection testing, and session issuance | `AGENTS.md` §§3, 5–6, 9–12, 16; `architecture/agent-rules.md` tenancy, audit, async, SSRF, and rustdoc rules; `architecture/wyrd-security-posture.md` delegation/federation; `architecture/references/architecture/patterns.md` | Reviewed the changed auth owners, callback/login flows, connection helpers, SQL capabilities, error boundary, and tests. |
| Platform login contract, server state, boot composition, routes, and session issuance | Same server/security authorities; `architecture/wyrd-design.md` runtime identity; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | Reviewed the typed callback/configuration contracts, process-owned `PlatformLogin`, server/test composition, handler use, OpenAPI-facing route types, and platform journey. |
| Workload issuer administration and boot discovery | `AGENTS.md` §§3, 5–6, 9–12, 16; `architecture/agent-rules.md` SQL, SSRF, async, and test-placement rules; `architecture/references/architecture/patterns.md` external-network/server patterns | Reviewed the typed discovery-document read, admin/boot callers, retry owner, `TenantConn`/`OperatorPool` use, and Postgres-backed tests. |
| Platform persistence migration and query layer | `AGENTS.md` §§3–6, 9, 11–12, 16; `architecture/agent-rules.md` SQL capability and transaction rules; `architecture/references/architecture/patterns.md` storage pattern | Reviewed the column removal, row/query/upsert parity, capability types, callers, and migration/query tests. |
| Rust unit, Postgres integration, real-server identity/platform journeys, CLI journey adjustment, and fixtures | `AGENTS.md` §11; `architecture/agent-rules.md` test placement, exact selector, and journey rules; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/testing-workflows.md` | Reviewed changed tests and fixtures, exact-selector evidence, broad lane evidence, and the production-shaped server composition used by the journeys. |
| Public contract and active change/review evidence | `AGENTS.md` §§8–9, 11–12; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/errors.md` | Reviewed the removed duplicate platform audience input/view/storage field, new RFC 9207 callback field, generated-contract evidence, remediation evidence, and withdrawal direction. |

`architecture/wyrd-doctrine.mdx` was checked for public-surface and service-boundary drift. Bifrost, PyO3, Python source, TypeScript source, storage engines, and analytical-domain authorities are not materially changed by this range.

## Applicable rule results

| Rule | Source evidence | Result |
|---|---|---|
| Use the approved vetted relying-party library and keep the dependency in its narrow owner | `openidconnect = "=4.0.1"` is workspace-pinned and consumed by `wyrd-auth-oidc`; discovery, authorization, PKCE, redemption, and ID-token verification use its types and APIs. | PASS |
| Do not enable `oauth2`'s `reqwest` feature or add a second unscreened relying-party transport | `openidconnect` has `default-features = false`; the inspected feature graph shows no enabled `oauth2` feature; `AsyncHttpClient` routes through `ScreenedHttp`. | PASS |
| Screen, resolve, pin, bound, and refuse redirects for provider URLs | `ScreenedHttp::send` obtains its client through `client_for`, preserves the screened address, reads through `read_bounded_body`, and returns redirects without following them; changed negative tests cover discovery/JWKS screening and token redirects. | PASS |
| Stateful workflows and caches have one concrete long-lived owner; handlers do not rebuild them | `HumanConnections` owns the tenant relying party; `PlatformLogin` is built once in production boot and the test server, stored in `ServerAuth`, and borrowed by both platform handlers. Workload setup performs a direct typed metadata read and does not create a second cache. | PASS |
| SQL capabilities and transaction ownership follow repository boundaries | Changed tenant work accepts `TenantConn`; platform reads use `OperatorPool`; no raw production `PgPool` signature or callee commit/rollback was added. Test-only pool construction remains in Postgres test modules. | PASS |
| Server durable behavior, plane separation, and canonical audit remain with their existing owners | Tenant/platform login selection, platform authorization, principal resolution, issuance, and audit stay in the existing server/auth owners; no parallel audit path or cross-plane fallback was added. | PASS |
| Secrets and provider credentials stay out of public payloads and ordinary diagnostics | Client secrets and PKCE verifiers remain secret-bearing/sealed values; response types omit secrets; relying-party `Debug` omits provider/cache state. The human withdrawal direction permits conventional standard OAuth provider error text and adds no new mechanism. | PASS |
| Public failures use Wyrd's existing stable catalog; local failures use `thiserror` | `RelyingPartyError` and `ProviderHttpError` remain crate-local and are projected through existing Wyrd error variants at server boundaries; OAuth protocol exceptions remain within the approved spec. | PASS |
| Pure work remains synchronous and async functions directly await IO or compose IO | HTTP, SQL, cache, and server operations are async; URL conversion, response-issuer comparison, claim checks, and error classification are synchronous. | PASS |
| Human ID-token audience is represented once and public contracts are source-generated | `CodeRedemption` derives audience from `client_id`; platform request/view/row/query/migration no longer expose or persist a second human audience; workload `expected_audience` remains on its separate RFC 7523 contract. Recorded `codegen:check` and principals integration pass. | PASS |
| New/materially modified Rust items have accurate durable rustdoc and no ephemeral task/commit references | The implementation removed all human-login/platform use of `ExternalVerifier`, but materially affected public docs still claim those consumers and one doc cites old implementation commits. See `REPO-R2-TASK-009-1`. | **FAIL** |
| User-visible behavior is covered at the journey tier, with supporting focused tests | The candidate records real-server tenant/platform journeys and supporting relying-party/Postgres tests for cache sharing, RFC 9207, discovery-only workload setup, audience derivation, and refusal paths. | PASS |
| Every specifically named test has exact zero-selection-safe evidence | The remediation evidence records exact `nextest` selectors and the required identity target listing/filter workflow, each selecting and passing one test. | PASS |
| Generated artifacts and dependency unions are not hand-maintained out of band | The recorded `codegen:check` and `check:workspace-hack` pass; the Hakari changes match the new dependency feature union. | PASS |
| No unrequested compatibility route, provider branch, setting, check, health probe, lock, retry policy, or test harness was introduced | None found. The remediation reuses Moka single-flight, the existing screened transport, current server state, existing SQL capabilities, and repository-native tests. | PASS |
| Human standing direction: mechanisms must match standards or comparable widely used projects | The candidate uses standard OIDC/OAuth library behavior, standard library constructors, ordinary process-local Moka coalescing, and existing repository mechanisms. This report requires only correction of inaccurate source documentation, not a novel mechanism, option, or check. | PASS |

## Material findings

### REPO-R2-TASK-009-1 — Removed human-verification ownership remains advertised in public rustdoc

- **Governing rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require every materially modified Rust item to carry accurate durable rustdoc explaining its real workflow role; code must not preserve references to implementation commits, tasks, plans, or agents. `architecture/references/languages/rust-core.md` repeats that documentation must let a maintainer safely identify ownership and consumers.
- **Locations:**
  - `crates/shared/wyrd-auth-verify/src/lib.rs:149-176`
  - `crates/shared/wyrd-auth-verify/src/lib.rs:371-377`
  - `crates/wyrd/wyrd-server/src/components/auth/state.rs:22-24`
  - `crates/wyrd/wyrd-server/src/boot/auth.rs:37-55`
- **Evidence:** The cumulative candidate deletes `ExternalVerifier::verify_id_token_against` and routes tenant and platform human ID tokens through `RelyingParty`; the only production consumer of `ExternalVerifier` is now the workload `jwt-bearer` flow (`wyrd-auth/src/jwt_bearer.rs`). Nevertheless, `ExternalClaims` says the platform control plane uses it directly, `VerifiedExternalIdentity` describes “commit 06” human construction, `ExternalVerifier` says it serves OIDC login and platform federated login, and both server handle docs still say it verifies human OIDC ID tokens. These are not merely old wording around unchanged behavior: they describe the ownership boundary this task materially moved.
- **Consequence:** A maintainer following the public owner and server-state documentation is directed to the workload verifier for human-login changes, obscuring the new `RelyingParty` boundary and inviting the deleted duplicate human verification path to be reintroduced. The embedded implementation-commit references also violate the repository's durable-code rule.
- **Smallest testable correction:** Update only these existing rustdocs to state that `ExternalVerifier` and its server handles verify trusted workload RFC 7523 assertions for `jwt-bearer`; remove the obsolete platform/human and implementation-commit claims. Keep human verification documented on `RelyingParty`/`HumanConnections`/`PlatformLogin`. Add no check, option, alias, abstraction, or new documentation file. Verify by source inspection and the existing Rust format/lint lane.

## Verification assessment

The durable remediation evidence records successful exact selectors for every named Rust test and successful broad lanes for formatting, lints, code generation, docs, workspace-hack, client/PyO3/unwrap boundaries, shared/Wyrd/SDK/CLI suites, principals integration, unfiltered identity journeys, and Python/TypeScript consumer lanes. I independently inspected the final source, complete base-to-candidate diff, dependency feature graph, caller set, and `git diff --check`; I did not start a competing Cargo verification process in the shared checkout.

The recorded lanes are credible for executable behavior, but green lints do not override the repository's explicit semantic-rustdoc requirement. No new permanent documentation check is warranted or permitted by the standing direction; the source-local correction is sufficient.

## Overall result

**FAIL**

One bounded repository-rule regression remains: public owner documentation still assigns human OIDC verification to the workload-only `ExternalVerifier`. All prior repository-standard findings from R1 are source-closed, and no nonstandard mechanism, setting, check, file, or option is required.
