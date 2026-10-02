# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `8289fa298ed33d21f2568558bc0a02905fd0b218`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation task:
  `changes/active/oidc-production-readiness/review/TASK-003-r2/TASK-003-R2-production-ui-remediation.md`
- Human directions:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  and
  `changes/active/oidc-production-readiness/review/TASK-003-r2/human-direction-connection-test.md`
- Approved specification: `SPEC-oidc-production-readiness` revision 7

The candidate remained at the stated commit during this review. I reviewed the
complete base-to-candidate change and used `622a77028..8289fa298` only to locate
the latest remediation owners. I did not modify reviewed source. CodeGraph was
not used because this repository has no `.codegraph/` directory.

## Authorities Read

- `AGENTS.md`, especially ownership, struct-centered Rust, documentation,
  typed contracts, and test-tier rules
- `architecture/agent-rules.md`
- `architecture/references/languages/maintainer-style.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/typescript-guide.md`
- `architecture/references/languages/testing-workflows.md`
- Applicable identity, public-surface, tenant, and UI authority in
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/wyrd-security-posture.md`

## Changed-Surface Coverage

| Surface | Changed owners, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| Public connection-test contract | `ConnectionTestRequest`, `ConnectionTestResponse`, `LoginInitiation`, `ConnectionTester`, auth exports, identity route DTOs, settings action, setup documentation | The response now names the one operation callers must perform: follow a typed `authorization_url`. The test-caller binding is a closed initiation variant, not a second login model, and its Rust documentation distinguishes test completion from redeemable browser/CLI completion. |
| Provider discovery and callback exchange | `RawProviderMetadata`, `ProviderMetadata`, `parse_raw_metadata`, `HumanConnections::begin_test`, `AuthorizationCodeExchange::{execute,complete,finish_id_token_exchange}`, server callback response selection, provider fixtures | Discovery, authorization URL construction, code exchange, ID-token verification, and conditional RFC 9207 handling stay on the existing provider and callback owners. The candidate-test branch is explicit at the two lifecycle points where behavior truly differs and otherwise reuses the production login path. |
| Candidate lifecycle and caller reauthorization | `HumanConnections`, `TestTarget`, `tested_candidate`, `stamp_test_sign_in`, `tester_authorized`, `test_candidate`; admin-handler and callback Postgres tests | The connection handle remains the cohesive owner of candidate state. The begin and stamp phases have distinct, descriptive methods; the latter documents and owns the second authorization decision and transactional audit/stamp boundary. Removed probe helpers, constants, and their tests leave no parallel testing workflow. |
| Login-state persistence | `LoginInitiation`; `InitiationColumns`; `initiation_columns`/`initiation_from_columns`; `LoginState`; insert/consume/redeem queries; `20261001000002_auth_connection_test_state.sql`; round-trip test | One closed initiation representation projects to one row shape. The storage helper refuses ambiguous/corrupt combinations, the migration constraints match the Rust variants, and browser/CLI callers continue through the same query API. Names and field types expose the binding rather than hiding it in generic metadata. |
| Browser-session renewal and sealing inventory | `BrowserSessions::current`/`renew`; `LockedBrowserSession` expiry fields; envelope inventory SQL and `SealedSecretRewrap`; keyless-boot and focused renewal tests | The remediation keeps the workflow on the existing session and rewrap owners. The single retry flag and `Renewal` enum make the one-relock ceiling and refused-versus-infrastructure outcomes visible without a new policy layer. Inventory documentation now accurately includes expired but stored envelopes. |
| BFF upstream and chooser bounds | `serverUrl`, every `ServerSessions` caller, `ServerSessions::metadata`, URL and chooser unit tests | Nullish defaulting distinguishes absent configuration from explicit emptiness. Sequential resolution is the smallest clear bound and keeps cookie validation, clearing, and projection on the existing `ServerSessions` owner without introducing a queue or concurrency abstraction. |
| Settings projection | `settings/+page.server.ts`, `settings/+page.svelte`, server connection-test response, production provider-replacement journey | The UI projects the server contract and redirects to the returned provider URL; it adds no local test status or identity authority. Existing `mutate`, `call`, CSRF, and safe-problem paths remain the only settings action boundary. |
| Production-shaped identity harness | `identity_ui_e2e` process and TLS owners; `production-auth.integration.test.ts` browser/provider helpers and four journeys; `identity_e2e` candidate-test helpers; `wyrd-testing::oidc_fixture`; Dex fixture and mise selector | Process lifecycle stays in the Rust host, browser behavior in Vitest, and provider form driving in the shared OIDC fixture where Rust journeys need it. The new TLS leg and Dex topology extend the existing lane rather than creating a second harness. Tests assert caller-visible outcomes and give helpers domain-specific names. |
| Documentation and generated parity | self-hosting SSO documentation, auth schema source/exports, served route annotations, codegen and docs evidence | The documented headless and UI workflows agree with `ConnectionTestResponse`: beginning returns an authorization URL; only the verified callback stamps the revision; no User, credential, or session is issued. No stale probe-based workflow remains in the inspected documentation or source. |

## Material Findings

None.

## Prior-Finding and Direction Closure

| Item | Maintainer evidence | Result |
|---|---|---|
| `FIND-TASK-003-4` | The existing `SealedSecretRewrap` and `SealedSecretTable` owners now describe and enumerate every stored non-null browser-session envelope, without an expiry/liveness filter. | CLOSED |
| `FIND-TASK-003-5` | `serverUrl` remains the single BFF upstream boundary; the existing Rust UI host adds a trusted TLS terminator and routes one replica through it. | CLOSED |
| `FIND-TASK-003-6` | Existing multi-provider helpers name Keycloak and Dex explicitly, and the callback mutation helper retains the genuine provider parameters while changing only state. | CLOSED |
| `FIND-TASK-003-10` | `BrowserSessions::current` documents the refused-early-renewal lifecycle and contains the one bounded relock on the existing owner; the focused test names the preserved-until-expiry outcome. | CLOSED |
| `FIND-TASK-003-11` | The raw RFC 9207 flag and `parse_raw_metadata` have substantive field and error documentation aligned with their actual conversion roles. | CLOSED |
| `FIND-TASK-003-12` | `ServerSessions::metadata` replaces unbounded `Promise.all` with an obvious sequential loop and documents the one-in-flight invariant. | CLOSED |
| `FIND-TASK-003-13` | `serverUrl` uses `??`, and its documentation and focused test distinguish absent from explicitly empty configuration. | CLOSED |
| Human issuer direction | The typed optional callback issuer remains on the common exchange owner; the documentation states the conditional advertised-support rule and no provider-specific activation branch exists. | CLOSED |
| Real-sign-in test direction | Probe machinery is deleted. `HumanConnections::begin_test` starts the ordinary PKCE/state/nonce flow, the common callback verifies it, and `stamp_test_sign_in` records only the authorized revision test. UI and headless journeys use that contract. | CLOSED |

## Calibration Notes

- The settings `test` action temporarily carries the returned URL outside
  `mutate` before redirecting. Redirecting inside the callback would also be
  readable, but the current shape makes successful mutation data explicit and
  adds no public abstraction or duplicated policy; this is not a material
  maintenance cost.
- `AuthorizationCodeExchange::complete` branches once to resolve candidate
  trust and `finish_id_token_exchange` branches once to stamp instead of issue.
  Combining those into a generic strategy or second exchange service would add
  indirection; the two explicit lifecycle differences are easier to trace.
- The inherited `SealedSecretRow::client_secret_enc` name remains imperfect for
  browser-session aliases, but the prior independent validation rejected that
  proposal as optional naming cleanup. The table discriminator, SQL aliases,
  and row documentation make the value unambiguous; this review does not
  reissue it.
- The long identity journeys are warranted cross-process workflows. Their
  browser, provider, API, and process helpers remove repeated mechanics while
  keeping each scenario's ordered behavior visible; splitting them into more
  test binaries or harness layers would increase setup and indirection.

## Verification Assessment

The remediation records successful focused proofs for envelope inventory,
trusted TLS, multi-provider switching and callback mix-up refusal, proactive
renewal refusal, discovery projection, bounded chooser verification, and empty
upstream configuration. It also records the real candidate-test sign-in
handler/callback tests, Keycloak and Dex journeys, the unfiltered identity
journey, UI tests and typecheck, `test:wyrd`, `test:sql`, codegen, tenant
isolation, docs, formatting, lints, and `git diff --check`.

I inspected the named selectors, assertions, and lane wiring but did not rerun
builds or tests in this review-only role. The evidence covers the changed
contracts and the maintainability-sensitive seams; no generated declaration or
documentation mismatch was found.

## Overall Result

**PASS**

The cumulative task and both remediation rounds keep each workflow on an
existing concrete owner, preserve typed contract parity, remove the obsolete
probe path, and leave tests and documentation discoverable. No material layout,
ownership, naming/type, test-clarity, documentation, or generated-parity defect
remains.
