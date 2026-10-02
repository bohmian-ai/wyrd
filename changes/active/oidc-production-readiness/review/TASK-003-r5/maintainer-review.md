# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
- Approved specification: `SPEC-oidc-production-readiness`, revision 7
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation tasks:
  `TASK-003-r2/TASK-003-R2-production-ui-remediation.md`,
  `TASK-003-r3/TASK-003-R3-browser-renewal-and-rustdoc-remediation.md`, and
  `TASK-003-r4/TASK-003-R4-renewal-contract-boundaries.md`
- Human directions:
  `TASK-003-r1/human-direction-FIND-TASK-003-1.md` and
  `TASK-003-r2/human-direction-connection-test.md`

The human owner explicitly authorized the R3 and R4 rounds. The candidate
resolved to the stated commit before and after this review. I reviewed the
complete base-to-candidate range and used `6aedcda5166509001db0cc851a5bc74502b4b043..989d0734b0a9b04f314ef4b52aa7d8510f26fe11`
only to locate the latest remediation edits. I did not modify reviewed source.
The repository has no `.codegraph/` directory, so direct source, Git, and
repository search were used.

## Authorities Read

- `AGENTS.md`, especially ownership, struct-centered Rust, public contracts,
  test tiers, implementation simplicity, and mandatory Rust documentation
- `architecture/agent-rules.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/maintainer-style.md`
- Applicable TypeScript and testing guidance in
  `architecture/references/languages/typescript-guide.md` and
  `architecture/references/languages/testing-workflows.md`
- Applicable identity, tenant, UI, and security authority in
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/wyrd-security-posture.md`
- The original task, all supplied remediation tasks, both human directions,
  and the prior TASK-003 review artifacts

## Changed-Surface Coverage

| Surface | Changed owners, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| Browser-session lifecycle and renewal | `BrowserSessions::{complete,exchange_api_key,read,authority,logout,current,renew}`; `Renewal`; credential open/seal helpers; `RefreshTokens`; `ExchangeApiKey`; `TenantTokenIssuer`; BFF handlers; `ServerSessions` consumers; unit and Postgres renewal tests | Session state, transaction meaning, credential opening, and renewal remain on one concrete owner. The three outcomes are explicit: containment commits, ordinary refusal preserves only already-issued authority until exact expiry, and internal failure rolls back and stays retryable without serving stale authority. The bounded relock is visible in `current`; no policy trait, retry framework, or second session owner was added. |
| R4 renewal classification boundary | `IssuanceError::is_refusal`, `RefreshError::is_refusal`, `ExchangeError::is_refusal`; every repository caller; classification unit test | All real callers are inside `wyrd-auth`, and all three methods are now `pub(crate)`. The public error enums remain unchanged. This is the minimum visibility needed by the sibling browser-session module and closes the unearned public-API surface without moving or duplicating policy. |
| R4 stored-envelope contract and proof | `open_text`, `open_credential`, `missing_or_unopenable_renewal_credential_is_retryable_failure`, `only_lifecycle_refusals_end_a_renewing_session`, and the existing refresh/API-key rollback-and-retry Postgres tests | The helper docs now separate raw envelope opening from caller-owned lifecycle policy. The focused unit test directly covers both absent storage and ciphertext under an unheld key, while the existing Postgres tests cover the transaction consequence of `Renewal::Failed`. The test reuses the existing module and keyring rather than adding a fixture or harness. |
| Candidate connection testing and callback | `ConnectionTestRequest`/`ConnectionTestResponse`; `LoginInitiation`; `HumanConnections::{begin_test,tested_candidate,stamp_test_sign_in}`; `AuthorizationCodeExchange`; provider metadata; callback handlers; login-state SQL and migration; settings action; real-provider tests | The headless and UI flows expose one typed authorization URL and reuse the ordinary PKCE/state/nonce, discovery, code exchange, and ID-token verification owners. Candidate-specific behavior is limited to exact revision/tester binding and the final stamp instead of issuance. The obsolete side-effect probes are absent. Names, return types, and rustdoc make the lifecycle differences findable without a second exchange abstraction. |
| RFC 9207 issuer binding | `CallbackQuery`; generated schemas; `ProviderMetadata`; `verify_response_issuer`; callback route and fixtures; issuer-binding tests and security documentation | The optional typed `iss` travels through the existing callback exchange owner. Conditional enforcement is documented beside the code and in security authority, while unrelated provider parameters remain tolerated. Source and generated callback schemas agree. |
| Browser-session persistence and sealing | Browser-session migration and query module; `BrowserSessionWrite`, `LockedBrowserSession`, `BrowserSessionMode`; tenant-definer lookups; `SealedSecretRewrap` and `SealedSecretTable`; boot and rotation tests | SQL types expose the stored invariants and PostgreSQL clock authority directly. Tenant-scoped work uses `TenantConn`; cross-tenant rewrap stays on `OperatorPool`. The canonical inventory includes expired-but-stored envelopes and retains the existing exact-byte CAS owner. No dead lifetime variant or second rotation path remains. |
| Private Rust BFF channel and server composition | `BffChannel`, `bff_router`, service-key middleware, typed request/response shapes, boot/config/router assembly, callback completion | The route module is a thin transport projection over `BrowserSessions`. Service-key admission is centralized before handlers, session-derived tenant authority remains below the transport, and boot injects the existing concrete owners. DTO names and field types correspond to the TypeScript channel shapes. |
| SvelteKit session boundary and UI routes | `ServerSessions`; hooks; upstream URL validation; root/login/completion/layout/settings routes; shell and tenant chooser; action helpers and unit tests | Cookie handling, CSRF, tenant verification, server-only access tokens, chooser resolution, switching, and Wyrd calls are discoverable on the single `ServerSessions` owner. Settings projects the server connection contract and introduces no role mapper or local status authority. Sequential cookie-hint resolution supplies the required concurrency bound without a queue abstraction. |
| Production journeys and fixtures | `identity_ui_e2e`; `identity_e2e`; `production-auth.integration.test.ts`; OIDC provider fixture; Keycloak/Dex fixtures; `mise` selector wiring | Cross-process lifecycle stays in the Rust host, browser behavior in Vitest, and provider sign-in mechanics in the existing OIDC fixture. The journeys name caller-visible scenarios for two replicas, OIDC-off entry, provider switching, mixed callbacks, refusal, logout, and TLS instead of creating a parallel harness. |
| Public docs and generated artifacts | Authentication and SSO setup docs; security posture; callback JSON schemas; generated aggregate docs | The interactive candidate-test flow, shared callback, server-held browser credentials, sealing procedure, and conditional issuer rule align with the inspected contracts. Generated callback schema copies are identical. |

## Material Findings

None.

## Prior-Finding and Direction Closure

| Item | Current-source maintainer evidence | Result |
|---|---|---|
| `FIND-TASK-003-16` | The three error classifiers are `pub(crate)` and repository-wide callers remain confined to `wyrd-auth`; no wrapper, trait, re-export, or public replacement was added. | CLOSED |
| `FIND-TASK-003-17` | `open_text` documents only opening and explicitly assigns lifecycle to its caller; `open_credential` documents retryable internal failure; the focused unit test covers absent and unopenable renewal envelopes. | CLOSED |
| `FIND-TASK-003-14` | `Renewal::{Contained,Refused,Failed}` and `BrowserSessions::current` preserve distinct commit, rollback/relock, and retry behavior. The focused replay, refusal, refresh-failure, and API-key-failure Postgres tests remain present and clearly named. | CLOSED |
| `FIND-TASK-003-15` | `exchange_api_key::pg_tests` and its shared live-key seed helper retain substantive documentation of their shared fixture role and panic boundary. | CLOSED |
| Prior R1/R2 findings | Conditional issuer binding, fixed-cost API-key refusal, server-verified chooser data, canonical sealing inventory, trusted TLS, multi-provider journeys, typed tenant projection, fixed session lifetimes, bounded chooser reads, and strict upstream parsing remain on their established owners. | CLOSED |
| Human issuer direction | A present `iss` is compared exactly, an absent value is refused only for advertising providers, and no provider-specific activation requirement was introduced. | CLOSED |
| Human real-sign-in direction | Candidate testing uses the shared authorization-code exchange, binds the exact candidate and tester, rechecks authority, stamps only that revision, and issues no User, credential, completion, or browser session. | CLOSED |

## Uncertain Preferences Kept Out of Findings

- `renewal_refused` also represents the post-containment relock state. A more
  neutral local name could be marginally clearer, but the `Renewal` enum and
  method rustdoc expose the material transaction distinction; renaming it
  would not remove a concrete maintenance hazard.
- `open_credential` intentionally collapses absent and unopenable envelopes
  into the same internal failure. A separate error variant would add surface
  without changing the caller's transaction or recovery behavior, and the
  direct test now pins both inputs.
- The operator runbook calls the rotated browser rows "live sessions" while
  the canonical inventory also walks expired, unpurged rows. The surrounding
  zero-remaining guarantee is accurate and the source contract is explicit;
  changing that wording alone would not alter an operator decision or justify
  another remediation round.
- The large identity journeys are justified ordered, cross-process workflows.
  Their existing browser, provider, API, and process helpers remove mechanics
  without creating a second fixture layer; splitting them further would add
  setup and obscure the scenarios.

## Verification Assessment

The R4 implementation record reports the exact unit selectors
`missing_or_unopenable_renewal_credential_is_retryable_failure` and
`only_lifecycle_refusals_end_a_renewing_session`, plus the four focused
Postgres selectors for replay containment, ordinary refusal, refresh internal
failure, and API-key internal failure. It also records `test:wyrd`,
`test:identity:journey`, `check:tenant-isolation`, formatting, lints, and
`git diff --check` green. Earlier cumulative evidence records the UI unit and
type checks, real two-replica/provider journeys, SQL, codegen, and docs lanes.

I inspected the named tests, their assertions, the lane wiring, and current
source, but did not rerun Cargo, Postgres, provider, browser, mise, or pnpm
commands in this review-only role. `git diff --check` for the immutable
base-to-candidate range is clean.

## Overall Result

**PASS**

The cumulative implementation keeps browser identity, renewal, persistence,
provider testing, and UI projection on discoverable concrete owners. R4 closes
the two remaining maintainer defects with the smallest local changes: narrower
visibility, accurate adjacent rustdoc, and one direct unit proof. No material
layout, owner/method shape, naming/type, test-clarity, documentation, or
generated-parity defect remains.
