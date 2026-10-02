# TASK-009 R3 Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`
- Binding withdrawals: `FIND-TASK-009-5` and `FIND-TASK-009-14`; neither is reopened here.
- Scope: repository-rule compliance only. Task acceptance and Ponytail validation belong to their independent reviewers.

The candidate object remained the stated commit throughout this review. `.codegraph/` is absent, so navigation used commit-qualified source, repository search, and direct caller inspection.

## Authority coverage

| Changed surface | Applicable authority | Coverage |
|---|---|---|
| Workspace dependency, lockfile, and generated Hakari feature union | `AGENTS.md` §§1, 4, 11–12, 15–16; `architecture/agent-rules.md` Cargo-feature rule; `architecture/references/languages/testing-workflows.md`; TASK-009's approved `openidconnect = 4.0.1` decision | Reviewed workspace/crate manifests, lockfile edges, `workspace-hack`, dependency placement, and recorded verification. |
| `wyrd-auth-oidc` relying party, screened transport, provider cache, verification, and local errors | `AGENTS.md` §§3–6, 9–10, 12, 16; `architecture/agent-rules.md` SSRF, async, owner, errors, and rustdoc rules; `architecture/wyrd-security-posture.md` federation, SSRF, and cryptography sections; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `architecture/references/languages/errors.md` | Reviewed the complete module, exports, cache/discovery callers, verifier configuration, tests, error projections, and transport routing. |
| `wyrd-auth-verify` workload verifier after removal of human ID-token verification | `AGENTS.md` §§3–6, 10, 12, 16; `architecture/agent-rules.md` owner and rustdoc rules; `architecture/wyrd-security-posture.md` external federation; `architecture/references/languages/rust-core.md` | Reviewed all corrected owner documentation and production callers; only RFC 7523 workload issuance remains. |
| Tenant login, callback, connection testing, role mapping, issuance, and audit | `AGENTS.md` §§3, 5–6, 9–12, 16; `architecture/agent-rules.md` tenancy, audit, async, SSRF, SQL, and rustdoc rules; `architecture/wyrd-security-posture.md` delegation/federation; `architecture/references/architecture/patterns.md` | Reviewed changed auth owners, callback/login paths, fixture composition, SQL capability types, failure mapping, and focused/served tests. |
| Platform login contract, process-owned relying party, server state, boot, configuration route, and callback | Same server/security authorities; `architecture/wyrd-design.md` runtime identity; `architecture/operations/deployment-and-release.md`; `architecture/references/languages/errors.md` | Reviewed `PlatformLogin`, `ServerAuth`, boot composition, typed handlers, full discovery during configuration, cache reuse, platform SQL callers, and served journey coverage. |
| Workload issuer administration and boot discovery | `AGENTS.md` §§3, 5–6, 9–12, 16; `architecture/agent-rules.md` SQL, SSRF, async, and test-placement rules; `architecture/references/architecture/patterns.md` | Confirmed workload setup retains its distinct metadata-only path and workload verification remains on `ExternalVerifier`/`JwksCache`. |
| Platform contract and durable SQL schema/query layer | `AGENTS.md` §§3–6, 9, 11–12, 15–16; `architecture/agent-rules.md` SQL capability and transaction rules; `architecture/operations/deployment-and-release.md` migration contract; `architecture/references/architecture/patterns.md` | Reviewed the typed request/view/row, migration, queries, production fields and signatures, callers, and SQL tests. The binding lead direction withdrawing `FIND-TASK-009-14` controls the unreleased-column contraction. |
| Rust unit tests, Postgres integration tests, real-server journeys, fixtures, and nextest configuration | `AGENTS.md` §11 and §16; `architecture/agent-rules.md` test placement, exact selectors, managed environment, and host-load rules; `architecture/references/languages/spec-driven-development.md`; `architecture/references/languages/testing-workflows.md` | Reviewed changed tests/helpers, rustdoc, exact-selector evidence, the reused `postgres-fixtures` nextest group, and the recorded lane results. |
| Workflow skills, their Claude discovery mirrors, and task verification text | `AGENTS.md` §§11, 14–16; `architecture/references/languages/spec-driven-development.md`; human verification direction in `518026d54` | Reviewed all four canonical/mirror pairs byte-for-byte and the task edits. They consistently place narrow write-set proof at task review and full journey/every-language proof at change review. |
| Public/generated surfaces | `AGENTS.md` §§8–9, 11–12; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/errors.md` | Reviewed the Rust wire owner and served routes. No Python, TypeScript, UI, `.pyi`, declaration, schema-golden, or other generated artifact is changed in the cumulative range; recorded `codegen:check` passed. |

`architecture/bifrost-design.md` and the analytical-domain references do not apply: the candidate changes no Bifrost ingest, query, storage, or maintenance behavior.

## Applicable rule results

| Rule | Source evidence | Result |
|---|---|---|
| Use the approved vetted relying-party library in its narrow owner | Workspace pins `openidconnect = "=4.0.1"`; `wyrd-auth-oidc::RelyingParty` owns discovery, PKCE authorization, redemption, and human ID-token verification. | PASS |
| Follow standard OIDC audience behavior without a Wyrd-specific option or check | `id_token_verifier` no longer installs the trust-all additional-audience callback; the library default is retained, with the existing `azp` check. Focused and served refusal tests cover the corrected behavior. | PASS |
| Do not enable `oauth2`'s reqwest feature or introduce a second unscreened transport | `openidconnect` keeps default features off; all relying-party HTTP uses the existing `ScreenedHttp` adapter. | PASS |
| Screen, resolve, pin, bound, and refuse redirects for provider URLs | `ScreenedHttp` remains the single transport for discovery, JWKS, and token requests, preserving DNS pinning, network policy, response bounds, no proxy, and no redirects. | PASS |
| Stateful workflows and caches have one concrete owner | `HumanConnections` owns tenant relying-party state; one boot-built `PlatformLogin` owns the platform `RelyingParty`; platform configuration borrows that same capability before persistence. No second cache or invalidation service was added. | PASS |
| SQL capabilities and transaction ownership follow repository boundaries | Production platform reads use `OperatorPool`; writes receiving an open authorization decision use `TenantConn`; no raw production `PgPool`, caller-supplied transaction, or callee commit/rollback was added. Raw pool construction/borrows found in the reviewed paths are test-only fixture code. | PASS |
| Server behavior, plane separation, and canonical audit stay with existing owners | Tenant/platform selection, principal resolution, issuance, role mapping, and audit remain in the current auth/server owners. No client-side durable behavior, parallel identity store, audit path, or cross-plane fallback was introduced. | PASS |
| Secrets and provider credentials remain sealed/redacted | Client secrets, codes, PKCE verifiers, and tokens retain secret-bearing types or sealed storage; public request/view types do not expose stored secrets. The binding withdrawal for `FIND-TASK-009-5` permits conventional provider error diagnostics and is not reopened. | PASS |
| Public failures use the existing Wyrd error catalog; crate-local failures remain local | `RelyingPartyError` remains local and `discovery_error` projects it through existing `WyrdError` variants at the HTTP boundary. No parallel public error taxonomy was added. | PASS |
| Pure work remains synchronous; async is limited to IO/cache composition | Discovery, HTTP, SQL, cache and handler paths await IO; claim checks, issuer comparison, URL conversion, and error classification remain synchronous. | PASS |
| Human audience is represented once in the public/runtime contract | The platform request, view, row, and verifier consume `client_id`; workload `expected_audience` remains on the separate RFC 7523 contract. The unreleased platform column drop is retained exactly as directed by the lead withdrawal of `FIND-TASK-009-14`. | PASS |
| New/materially modified Rust items and test helpers have accurate durable rustdoc | The former human/platform claims on `ExternalVerifier`, `ServerAuth`, and `AuthHandles` are corrected to workload-only ownership; dead callback verifier wiring is deleted; new accessors, helpers, and modified tests carry intent-focused docs and required error/panic sections. | PASS |
| Tests use the owning tier and repository-managed environment | Unit behavior stays inline; Postgres and real-server behavior stays in `pg_tests`/external journey targets. Exact named commands use `mise exec --` and Postgres-backed lanes use repository wrappers. | PASS |
| Test concurrency uses an established native mechanism rather than a custom harness | `platform_admin_e2e` joins the existing nextest `postgres-fixtures` group used for per-test `WyrdTestServer` binaries; no sleep, retry, raised load, new runner, or task-specific concurrency implementation was added. | PASS |
| Verification is proportional to the task write set | The remediation records exact focused tests plus affected owner, formatting, lint, codegen, boundary, and workspace-hack lanes. Under human direction `518026d54`, full user-journey and every-language sweeps belong to final change review; their absence from a task round is not a gap. | PASS |
| Canonical skills and mirrors remain synchronized | Each changed `.agents/skills/<name>/SKILL.md` is byte-identical to its `.claude/skills/<name>/SKILL.md` mirror. The edits consistently encode the human verification direction. | PASS |
| No generated artifact was hand-edited and no unrelated language layer was implied as covered | The cumulative diff changes no Python/TypeScript/UI source or generated stub/schema golden; recorded `codegen:check` passed. | PASS |
| No nonstandard mechanism, check, file, setting, option, or compatibility surface was introduced | The candidate uses the selected OIDC library default, existing screened transport, Moka cache, typed SQL capabilities, the process-owned login owner, existing nextest group, and existing tests. No custom probe, audience option, cache protocol, compatibility service, or permanent check was added. | PASS |

## Material findings

None.

## Prior standards-finding closure

| Prior finding | Current source evidence | Result |
|---|---|---|
| `REPO-R2-TASK-009-1` / validated `FIND-TASK-009-12` | `ExternalClaims`, `VerifiedExternalIdentity`, `ExternalVerifier`, `AuthHandles`, and `ServerAuth` now describe workload RFC 7523 verification only. The callback fixture no longer constructs `PgIssuerResolver`, `JwksCache`, or `ExternalVerifier`; production `jwt_bearer` remains the human-independent consumer. | CLOSED |

`FIND-TASK-009-5` and `FIND-TASK-009-14` remain **WITHDRAWN — MUST NOT REOPEN**.

## Verification assessment

The remediation record reports all changed named tests passing with exact zero-selection-safe selectors, including the relying-party audience refusal, callback target, served platform configuration/cache journey, workload metadata-only paths, and the tenant refusal journey. It also records successful `fmt`, `lints`, `codegen:check`, client/PyO3/unwrap/workspace-hack boundaries, and affected shared, principals, and Wyrd owner lanes. Static review additionally confirmed a clean cumulative `git diff --check`, synchronized workflow-skill mirrors, no generated/language-surface diff, and typed SQL production signatures.

The record also contains broader historical lanes, but they are not required to establish this remediation round. Per the binding human verification rule in `518026d54`, final unfiltered user journeys and every-language sweeps run once at change review, not as a task-review prerequisite.

## Overall result

**PASS**

Repository authority coverage is complete, the prior standards finding is source-closed, and no material repository-rule violation remains.
