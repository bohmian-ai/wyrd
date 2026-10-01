# Maintainer Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation task:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md`
- Human direction:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
- Approved specification: `SPEC-oidc-production-readiness` revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20`

The candidate remained at the stated commit during this review. I reviewed the
complete base-to-candidate change and used the prior-candidate-to-candidate
diff only to locate remediation owners. I did not modify reviewed source.
CodeGraph was unavailable because the repository has no `.codegraph/`
directory.

## Authorities Read

- `AGENTS.md`, especially ownership, struct-centered Rust, documentation,
  typed client projection, and test-tier rules
- `architecture/agent-rules.md`
- `architecture/references/languages/maintainer-style.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/typescript-guide.md`
- `architecture/references/languages/testing-workflows.md`
- Applicable identity, tenant, UI, and security authority in
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and
  `architecture/wyrd-security-posture.md`

## Changed-Surface Coverage

| Surface | Changed owners, callers, and tests inspected | Maintainer assessment |
|---|---|---|
| Callback contract and issuer binding | `CallbackQuery`; generated callback schemas; `ProviderMetadata`; `AuthorizationCodeExchange::exchange`; `verify_response_issuer`; server callback route; fixture callback forwarding; focused and journey tests | The typed `iss` value follows one discoverable path from the wire contract to the existing exchange owner. The conditional rule matches the human direction and its tests name the advertised and non-advertised cases directly. |
| Browser API-key exchange | `BrowserSessions::exchange_api_key`; `ExchangeApiKey::execute`; the dummy-verification path; BFF handler; focused Postgres test | The browser owner now delegates to the existing exchange owner. The method documents its one-verification invariant and the test enumerates each presented-key class without adding a second verifier. |
| Browser session projection and chooser | `BrowserSessionView`; `ReadResponse`; TypeScript `Read` and `ServerSession`; `ServerSessions.read`, `context`, `metadata`; layout, hooks, chooser consumers; unit and HTTP journeys | The authoritative tenant id is carried server-side without entering page metadata. Cookie suffixes remain lookup hints and verified options are projected through the existing session owner. Rust and TypeScript fields agree. |
| Sealing-key lifecycle | `SealedSecretRewrap`; `SealedSecretTable`; `SealedSecretRow`; inventory/CAS statements; boot gate; rotation and keyless tests; operator documentation | The existing rewrap owner covers browser-session columns and preserves one CAS mechanism. `MAINT-R2-001` covers the generalized row field that still carries its former client-secret name. |
| Browser-session persistence shape | `BrowserSessionWrite`; `insert_browser_session`; both construction sites; query exports and tests | The unused lifetime enum is gone. One concrete duration reaches one PostgreSQL-clock expression while refresh expiry remains independent. |
| BFF origin posture | `serverUrl`, all callers in `ServerSessions`, readiness, and focused URL tests | One native `URL` boundary rejects non-loopback plaintext before fetch. No bypass or parallel upstream setting was added. |
| Server composition and documentation | BFF DTO conversions; `BrowserSessions` debug implementation; server config helpers; boot assembly and config-field projection | The prior misplaced/missing rustdoc is corrected at the owning items, and modified fallible Rust items describe their relevant errors. Composition remains on existing concrete owners. |
| Production UI and provider-replacement journeys | `identity_ui_e2e` process/tenant setup; TypeScript browser helpers and four journeys; callback and sealing journeys; mise selector routing | The host owns process lifecycle and setup; browser mechanics stay in the TypeScript journey. New scenarios use caller-visible results and preserve the existing two-replica path instead of creating another harness. |
| Docs and generated declarations | Security posture, self-hosting authentication/SSO docs, schema docs, generated JSON and aggregate docs | Source and generated callback declarations agree, and the residual non-advertising-provider exposure is documented without presenting it as a new supported-provider restriction. |

## Material Findings

### MAINT-R2-001 — Generalized sealed-secret rows retain a client-secret-only field name

- **Location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:159-168,478-503`;
  consumer `crates/wyrd/wyrd-auth/src/sealing.rs:96-104`
- **Governing principle:** The maintainer guide requires names and typed shapes
  to describe their actual responsibility. `AGENTS.md` requires cohesive
  owners and the smallest directly understandable implementation.
- **Evidence:** `SealedSecretRow::client_secret_enc` originally represented
  provider client secrets. The remediation generalized the row to six
  `SealedSecretTable` variants and now aliases browser access tokens, refresh
  tokens, API keys, and CSRF tokens to `client_secret_enc`. The canonical
  rewrap loop consequently calls `settle(&row.client_secret_enc, ...)` even
  for `BrowserSessionCsrf` and `BrowserSessionAccess`.
- **Concrete maintenance cost:** The value's type-level name contradicts the
  table variant that identifies it. A maintainer extending or diagnosing the
  canonical key inventory must mentally override the field name at every read
  and CAS bind, and can reasonably mistake the row as provider-only despite
  the task making browser-session ciphertext part of the same authoritative
  inventory.
- **Smallest testable correction:** Rename only the generalized row field and
  its SQL aliases/callers to a neutral value name such as `sealed`, preserving
  `SealedSecretTable`, the existing inventory, and the CAS statements. Run
  format/lints plus the existing sealing-rotation and keyless-boot proofs; no
  new type, module, or test is needed.
- **Nearby pattern:** The same generalized shape already uses the neutral
  names `SealedSecretRow`, `sealed_tenant_secrets`, and
  `swap_sealed_tenant_secret`; the value field should match that established
  vocabulary.

## Prior Maintainer Finding Closure

| Prior finding | Source evidence | Result |
|---|---|---|
| `MAINT-001` — misplaced config rustdoc | `parse_bff_service_key_hashes` now documents parsing and overlap, while `env_opt` again has the environment-reading contract and its own `# Errors` section. | CLOSED |
| `MAINT-002` — fabricated tenant identifier | `BrowserSessionView.tenant_id` flows through `ReadResponse.tenant_id`, TypeScript `Read`/`ServerSession`, and `ServerSessions.context`; the focused test asserts the server value and its absence from page metadata. | CLOSED |
| `MAINT-003` — unused lifetime branch | `SessionLifetime` is absent; `BrowserSessionWrite::lifetime` is one `Duration`, and the insert uses one PostgreSQL-clock duration bind. | CLOSED |

## Calibration Notes

- `ServerSessions.metadata` uses `Promise.all` over session-cookie hints. The
  cookie header supplies a platform-bounded input, and each lookup is the
  required independent server verification; introducing a queue here would
  add machinery without a demonstrated operational or maintenance problem.
- The sealing-rotation and callback journeys are long, but each follows one
  ordered cross-boundary workflow and has named setup/read helpers. Splitting
  them into smaller tests would either repeat expensive environment setup or
  obscure the lifecycle being proved.
- `SealedSecretTable` remains in `human_connections.rs`, where the pre-existing
  cross-table rewrap SQL already lived. Moving the owner during this bounded
  remediation would be a larger, optional module refactor; it is not a
  finding.

## Verification Assessment

The remediation records successful focused issuer, API-key, session, URL,
keyless-boot, and UI-journey proofs, plus the unfiltered identity journey, UI
test/typecheck, `test:wyrd`, `test:sql`, codegen, tenant-isolation, docs,
format, lints, and `git diff --check`. I inspected the named selectors and lane
wiring but did not rerun Cargo-, pnpm-, provider-, or Postgres-backed commands
in this review-only role. `git diff --check` for the immutable range is clean.
Those checks prove behavior but do not make the generalized field name accurate.

## Overall Result

**FAIL**

The prior maintainer findings are closed and the changed owners are otherwise
discoverable and internally consistent. `MAINT-R2-001` is a bounded naming
defect in the newly generalized canonical rewrap shape.
