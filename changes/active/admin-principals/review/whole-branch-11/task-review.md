# Admin principals whole-branch review 11 — task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate / reviewed HEAD:
  `41e60be61958c92f562fceb5b7a03f40f611bbc8`
- Approved specification: `changes/active/admin-principals/spec.md`, revision
  14, status `approved`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- R9 remediation:
  `changes/active/admin-principals/review/whole-branch-09/TASK-001-008-R9-close-validated-findings.md`
- R10 remediation:
  `changes/active/admin-principals/review/whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`

The complete cumulative diff and current source were inspected. CodeGraph was
used first to trace token exchange, issuance, verification, policy, audit,
audience admission, shared-client delegation, SDK projection, and CLI secret
resolution. The five-minute self-contained-JWT revocation window is an
explicitly accepted tradeoff and is not a finding.

## Review Findings

### Critical

#### `TREV-WB11-1` — MISSING — RFC 8693 delegation cannot run in a production deployment

- **Violated obligation:** revision-14 `REQ-012`, `REQ-012c`, `REQ-047`,
  `AC-018`, and `AC-020`, plus R10's outcome and RFC-request acceptance row,
  require token exchange to be a supported tenant issuance path. R10 also
  requires stale implementations of the inverted/preview model to be removed.
- **Exact locations:**
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs:176-185`,
  `crates/wyrd/wyrd-server/src/config.rs:2738-2750`, and
  `crates/wyrd/wyrd-server/src/state.rs:2438-2444`.
- **Evidence:** `POST /auth/token` still returns `preview_disabled()` for every
  token-exchange request unless `state.auth.allow_preview` is true. Both config
  validation and `AppState::production_validate` reject a production API server
  when that same flag is true. The proving server hides the contradiction by
  defaulting `WyrdTestServerBuilder.allow_preview_auth` to true. Therefore there
  is no valid production configuration in which the newly required exchange
  flow can issue a token.
- **Observable consequence:** Service B cannot obtain the specified delegated
  JWT for Service A in staging or production, so the corrected RFC flow and all
  three SDK helpers fail before either token is verified.
- **Required testable correction:** retire the delegation preview gate from the
  existing token-exchange route and remove its now-orphaned auth configuration,
  environment parsing, production rejection, error branch, and live docs. Keep
  the existing verifier, issuer, directed policy hook, audit append, and
  production requirement for a non-stub policy. Add no replacement feature
  flag or compatibility path.
- **Focused closure proof:** run the primary A/B/Bifrost journey with a
  production-profile server configuration and a non-stub directional policy,
  proving exchange succeeds without `WYRD_AUTH_ALLOW_PREVIEW`; retain a focused
  config assertion that no preview switch is accepted or required.

### Important

#### `TREV-WB11-2` — INCORRECT — delegation audit rows name the operation instead of the invoke permission actually evaluated

- **Violated obligation:** `REQ-012c`, `REQ-037`, `INV-010`, `AC-009`, and
  R10's audit row require every policy decision to record the effective
  permission/action it evaluated. `AuditEvent.permission` is explicitly the
  dynamic permission field.
- **Exact locations:**
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:280-296,391-420` and
  `crates/wyrd/wyrd-auth/src/issuance.rs:557-609`.
- **Evidence:** the policy evaluates `DELEGATION_POLICY_ACTION == "invoke"`,
  but both no-token decisions and successful exchanges are created through
  `auth_event(..., TOKEN_EXCHANGE_OPERATION, ...)`. `auth_event` initializes
  `permission` from that operation, and these callers overwrite only
  `resource`, leaving `permission == "auth.token.exchange"`. Existing tests
  query outcome, principal, credential, resource, and detail but never read the
  permission column.
- **Observable consequence:** audit history falsely states that the policy
  evaluated a non-permission operation name, so it cannot answer which
  authorization decision permitted or denied the A-to-B invoke.
- **Required testable correction:** keep `operation = "auth.token.exchange"`
  and the one canonical row, but set its existing `permission` field to the
  existing `DELEGATION_POLICY_ACTION` for every delegated allow/deny/no-effect
  result. Do not add another audit row or vocabulary.
- **Focused closure proof:** extend the existing allow, deny, and
  allowed-then-refused exchange assertions to select `permission` and require
  exactly one row with `permission = "invoke"`; retain the audit-failure proof.

#### `TREV-WB11-3` — MISSING — the proving journey installs an allow-all hook and never proves the directed A-to-B relationship

- **Violated obligation:** `REQ-012c`, `INV-013a`, `AC-020`, and R10's RFC
  request semantics require the existing invoke policy to authorize the
  directed A-to-B relationship and require reversed subject/actor input to
  issue no token.
- **Exact locations:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:1165-1169`,
  `crates/shared/wyrd-auth-check/src/hook.rs:40-74`, and
  `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1629-1687`.
- **Evidence:** the primary journey injects `RecordingPolicyHook`, whose
  implementation records every context and returns `Allow` unconditionally;
  it does not establish an A-to-B policy. The malformed-input table covers an
  invalid token, foreign actor, delegated actor, card-free actor, and
  self-exchange, but never swaps valid A and B tokens. With the journey's hook,
  a valid reversed B-subject/A-actor request is allowed rather than proving the
  directed relationship.
- **Observable consequence:** the evidence would stay green if the production
  request built the policy question backwards or if the policy treated A-to-B
  and B-to-A as equivalent.
- **Required testable correction:** keep the existing `PolicyHook` seam and
  journey harness, but make the journey's policy allow exactly subject A to
  actor B and deny the reverse. Exercise the reverse exchange over the real
  token endpoint and assert it issues no token and commits the one denied
  decision. Do not add a delegation store, role, permission, or second policy
  engine.
- **Focused closure proof:** the existing primary journey must perform both
  exchanges: A subject/B actor succeeds and B subject/A actor receives the
  stable policy denial with no token or Bifrost effect.

#### `TREV-WB11-4` — MISSING — the Python and TypeScript helpers return clients that cannot perform any delegated operation

- **Violated obligation:** `REQ-047`, `AC-020`, and R10's client-helper row
  require Rust, Python, and TypeScript to expose the same useful
  `on_behalf_of` behavior, with foreign runtimes projecting the Rust owner.
- **Exact locations:**
  `sdks/wyrd-sdk-python/src/client.rs:16-79`,
  `sdks/wyrd-sdk-ts/native/src/client.rs:15-99`,
  `sdks/wyrd-sdk-python/tests/unit/client/test_client.py:20-36`, and
  `sdks/wyrd-sdk-ts/wyrd/tests/unit/wyrd-client.test.ts:8-24`.
- **Evidence:** each foreign wrapper exposes only construction and another
  delegation call. Its inner Rust `WyrdClient` is private and cannot be passed
  to the separately constructed Python/TypeScript Bifrost, Cards, or
  `WyrdState` surfaces. Thus a successfully returned delegated client has no
  request method or resource handle with which to use its token. The SDK tests
  exercise only invalid audience and unreachable-server errors; neither
  performs a successful exchange or a delegated request.
- **Observable consequence:** Rust can complete the A/B/Bifrost workflow, but a
  Python or TypeScript developer receives an opaque handle that cannot read or
  write anything, so the advertised SDK helper is not a usable projection of
  the Rust behavior.
- **Required testable correction:** reuse the existing Rust-owned client and
  existing SDK Bifrost projection so the delegated Python/TypeScript result can
  perform the audience-appropriate operation without reconstructing transport,
  headers, exchange, or caching in either language. Choose the smallest
  existing composition seam; do not duplicate the Bifrost client or add a new
  delegation transport.
- **Focused closure proof:** in each existing Python and TypeScript integration
  harness, obtain a delegated client through the public helper and perform one
  real allowed Bifrost read plus one refused write (or the narrowest existing
  equivalent). Unit tests that only prove an error crossed the native boundary
  do not close this finding.

#### `TREV-WB11-5` — REGRESSION — live Bifrost documentation still tells users to put a removed bearer option in argv

- **Violated obligation:** R9 `FIND-admin-principals-R8-2`, `INV-002`, and the
  R9 acceptance result require shipped CLI secrets to use ambient sources and
  require the removed secret options not to remain as the documented workflow.
- **Exact locations:**
  `docs/src/content/docs/bifrost/reading-data.svx:135-142` and
  `crates/wyrd/wyrd-cli/src/query/mod.rs:23-31`.
- **Evidence:** `wyrd query` deliberately removed `--token` and documents in
  source that credentials come only from the ambient chain, while the live
  Bifrost guide still runs `wyrd query --token "$WYRD_ACCESS_TOKEN"`. The R9
  parser test proves that option is rejected. `docs:check` does not validate CLI
  argument examples, so it passes this contradiction.
- **Observable consequence:** the documented query command fails, and users are
  instructed to expose a bearer through shell argv despite the security fix.
- **Required testable correction:** delete `--token` from the existing example
  and state the already-supported ambient source (`WYRD_ACCESS_TOKEN` or the
  normal credential chain). Add no compatibility option.
- **Focused closure proof:** extend the existing docs command check or the
  smallest current CLI-doc assertion so this exact example parses under the
  shipped root CLI without a secret argument.

### Suggestions

None.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-001`–`REQ-011`; `INV-001`, `INV-003`, `INV-008`, `INV-009`: independent principals, generic multi-credential ownership, verifier-only persistence, overlap rotation | Durable principal/grant stores remain separate from UUID-typed credential rows; issuance resolves principal authority rather than storing it on credentials | Principal unit/integration, SQL, platform, identity, and CLI evidence remains cumulative | PASS |
| `REQ-012`, `REQ-012a`, `REQ-012b`; `AC-018`: all grants share current-state issuance and tenant requests verify five-minute `permissions` JWTs locally | `TenantTokenIssuer` and concrete synchronous `TokenVerifier` remain the single owners | R10 issuance/verifier selectors and owning lanes are recorded green | PASS except token exchange is unavailable in production — `TREV-WB11-1` |
| `REQ-012c`; `INV-013a`; `AC-020`: RFC 8693 subject A, actor B, directed invoke policy, audience binding, attenuation, canonical audit | Claims, audience admission, and permission intersection are implemented; the route remains preview-only, the journey policy is allow-all, and audit permission is wrong | Rust journey proves `sub=A`, `act=B`, Bifrost read, denied write, and audience rejection, but not production or reversed direction | **FAIL — `TREV-WB11-1`, `TREV-WB11-2`, `TREV-WB11-3`** |
| `REQ-013`–`REQ-019`; `INV-004`, `INV-004a`, `INV-004b`: closed platform/tenant contexts and permission authorization | Typed extractors and per-plane authorization remain distinct; delegated requests reuse `Caller` | Platform, principal, authz, and cross-plane evidence retained | PASS |
| `REQ-020`–`REQ-024`; `INV-005`; `AC-001`: one explicit transactional deployment initialization | Existing initialization owner remains unchanged by R9/R10 | Prior real-server concurrency/failure evidence retained | PASS |
| `REQ-025`–`REQ-028`; `INV-006`; `AC-002`, `AC-007`, `AC-008`: transactional tenant provisioning and lifecycle | Platform provisioning/lifecycle owners and issuance admission remain in place | Prior platform journey evidence retained | PASS |
| `REQ-029`–`REQ-033`; `INV-007`; `AC-004`–`AC-006`, `AC-012`: tenant administration, scoped principals, lifecycle, recovery | Tenant operations remain RLS-confined; credential IDs are now UUID end to end | R9 OpenAPI, MCP, CLI, principal, SQL, and journey evidence | PASS |
| `REQ-034`, `REQ-035`, `REQ-041`–`REQ-046`; `AC-011`, `AC-015`–`AC-017`: federated tenant/platform humans and current-state platform authorization | Issuance-side OIDC and platform request-time checks remain unchanged | Prior identity/platform journeys retained | PASS |
| `REQ-036`, `REQ-047`; `AC-013`, `AC-014`: shared client and first-class projections | Rust owns exchange/cache/retry; foreign wrappers call Rust but return unusable foreign client handles | Rust journey passes; Python/TS tests cover only validation/transport failures | **FAIL — `TREV-WB11-4`** |
| `REQ-037`; `INV-010`, `INV-011`; `AC-009`: decisions are transactional, fail closed, and name principal, credential, permission, resource, tenant, outcome | Canonical append and credential attribution are present, but delegation rows retain the operation name as permission | R9 attribution and R10 allow/deny/failure tests omit the permission column | **FAIL — `TREV-WB11-2`** |
| `REQ-038`–`REQ-040`: removed bootstrap model and accurate operator/security documentation | Bootstrap artifacts remain absent; most architecture and operator docs are current | `docs:check` passes, but the live Bifrost CLI example invokes a removed secret option | **FAIL — `TREV-WB11-5`** |
| `REQ-048`; `INV-013`: machine re-exchange, human refresh, bounded stateless tenant tokens | Existing credential cache/re-exchange and human refresh families remain separate | R9/R10 shared, identity, auth, and CLI evidence retained; accepted revocation window excluded | PASS |
| `REQ-049`; `AC-019`: runtime `utoipa` OpenAPI is the sole document | Runtime router/document composition and regenerated token schema remain single-owner | `test:principals:integration`, `codegen:check`, and docs evidence recorded green | PASS |
| `INV-002`; R9 `FIND-admin-principals-R8-2`: no secret-valued CLI option or debug exposure | Shipped parser no longer accepts the options and secret values use ambient/file sources | R9 exact parser and CLI journey evidence passes; one live guide is stale | PASS in code; documentation regression is `TREV-WB11-5` |
| R9 `FIND-admin-principals-R9-1`: successful delegation audit carries actor credential while JWT carries none | `TenantGrant::Delegation.actor_credential_id`; event attaches it; token accessor remains `None` | Exact Postgres selector recorded 1/1 | PASS |
| R9 `FIND-admin-principals-R9-2`: credential IDs remain UUID through DTO/client/CLI | `Uuid` is used on issue/list/revoke surfaces and parsed at CLI edge | Served OpenAPI, MCP, malformed CLI, lifecycle evidence recorded | PASS |
| R10 removal constraints: no `delegation:issue`, `requested_subject`, second issuer/verifier, table, role, permission, audit sink, migration, or compatibility route | Removed identifiers have no live production caller; existing owners are reused | Static scans, codegen, boundary checks, and task evidence | PASS, except the stale preview gate remains — `TREV-WB11-1` |
| R10 audience/request middleware | Wyrd routes accept only `wyrd`; Bifrost HTTP and gRPC use `verify_on(..., Bifrost)`; no DB/cache introspection | Primary journey proves Bifrost acceptance, Wyrd-route 401, read/write authorization | PASS |
| R9/R10 required verification and exact selectors | Evidence tables record the requested lanes and positive exact selections; strict docs reported for affected crates | No long-running lane was rerun in this static review | PASS as recorded, but the listed suites do not cover the five gaps above |

## Open Questions

None. Each correction stays inside the approved revision-14 architecture and
reuses an existing owner. The accepted five-minute revocation tradeoff is not
reopened.

## Verification Notes

- This was a review-only static audit; recorded long-running lanes were
  inspected but not rerun.
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f..41e60be61958c92f562fceb5b7a03f40f611bbc8`
  is clean.
- The R10 Rust journey is credible for claim orientation, audience binding,
  attenuation, and Bifrost request behavior, but its builder enables preview
  auth and its policy hook allows every direction.
- The Python and TypeScript unit tests prove native ownership only on failure;
  neither proves successful delegated use.
- Candidate HEAD remained
  `41e60be61958c92f562fceb5b7a03f40f611bbc8` through source inspection.

## Overall result

**FAIL** — R9's three remediation findings are implemented in code, but one
live CLI guide contradicts its secret-removal result; R10's RFC claim shape and
Rust Bifrost behavior are present, but delegation is impossible in production,
the directed reverse case is unproved, policy audit records the wrong
permission, and the Python/TypeScript helpers cannot perform a delegated
operation.
