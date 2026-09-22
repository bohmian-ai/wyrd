# Task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 14,
  SHA-256 `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`
- Original tasks: `TASK-001` through `TASK-008`, reviewed cumulatively with
  all remediation tasks through `TASK-001-008-R11`

The candidate commit remained unchanged during this review. An uncommitted
`mise.toml` edit appeared in the shared worktree after review started; it is not
part of the immutable candidate, was not used as implementation evidence, and
was neither edited nor reverted by this reviewer.

## Review Findings

### Critical

None.

### Important

- **`TASK-REV-1` — INCORRECT** —
  [`crates/wyrd-spec/src/auth/token.rs:108`](../../../../../crates/wyrd-spec/src/auth/token.rs)
  still documents `TokenResponse.refresh_token` as present for API-key exchange,
  and that false statement is published verbatim in
  `crates/wyrd-spec/schemas/auth_token_response.json` and its test golden.
  `REQ-048`, `AC-013`, and `AC-018` require machine API-key grants to return no
  refresh token and require generated contracts to describe that model
  accurately; the implementation correctly returns `None`
  (`wyrd-auth/src/issuance.rs`) and proves no refresh row is written
  (`exchange_api_key::pg_tests::api_key_exchange_issues_no_refresh_token_or_row`),
  but an independent client reading the shipped schema is told the opposite.
  Correct the owning Rust field documentation to say refresh tokens are present
  only for initial human OIDC login and human refresh rotation, regenerate the
  owned JSON schemas/OpenAPI projections, and close with the existing API-key
  no-refresh focused test plus `codegen:check` and a served-contract assertion
  or inspection showing the generated description no longer names API-key
  exchange.

### Suggestions

None. Optional improvements and unrelated pre-existing debt were excluded.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `REQ-001`–`REQ-011`, `REQ-039`, `INV-001`–`INV-003`, `INV-008`–`INV-010`, `INV-012`, `INV-014`: principals and credentials are separate, principal-generic, verifier-only, independently revocable durable records | Principal/credential migrations and query owners under `wyrd-sql`; `wyrd-auth/src/issue_api_key.rs`, `credential_verify.rs`, and `platform_credentials.rs`; UUID wire contracts in `wyrd-spec/src/auth` | Recorded `test:principals:unit` 39/39 and `test:principals:integration` 77/77; credential lifecycle and cross-tenant platform journeys run in `platform_admin_e2e` | PASS |
| `REQ-012`, `REQ-012a`, `REQ-012b`, `REQ-013`–`REQ-019`, `REQ-031`, `REQ-047`, `INV-004`, `INV-004a`, `INV-011`, `INV-013`, `INV-015`, `AC-003`, `AC-010`, `AC-018`: one issuance owner, local tenant-JWT verification, current-state platform verification, plane separation, one client/header | `wyrd-auth/src/issuance.rs::TenantTokenIssuer`; concrete synchronous `TokenVerifier`; `AuthenticatedPrincipal`/`Caller`; platform session confirmation; CLI routes through `wyrd-client`; removed revocation-check/cache/epoch implementation | `auth_e2e` 8/8, `test:platform:journey`, `test:cli:journey`, `test:wyrd`, `test:shared`, client-tier and auth audit checks recorded passing; exact revocation/snapshot tests are present and positively selected in cumulative evidence | PASS |
| `REQ-020`–`REQ-024`, `INV-005`, `AC-001`: explicit, one-transaction, concurrency-safe deployment initialization with one-time secret disclosure | `wyrd-server/src/boot/init.rs::initialize_platform_root`; server command dispatch; durable initialization guard; no server-start initialization path | `platform_admin_e2e::{initialization_happens_at_most_once,an_operator_initializes_the_deployment_through_the_shipped_binary}` and staged-failure coverage recorded passing | PASS |
| `REQ-025`–`REQ-028`, `INV-006`, `AC-002`, `AC-007`, `AC-008`: transactional/resumable tenant provisioning, lifecycle isolation, suspend/resume | `components/platform/provisioning.rs::TenantProvisioning`; platform routes and tenant-admission checks | Real-server platform journeys for operator provisioning, staged failure/retry, racing provisioning, and suspend/resume recorded passing | PASS |
| `REQ-029`–`REQ-033`, `AC-004`–`AC-006`, `AC-012`: tenant administration, restricted machine principals, credential rotation/recovery, cross-tenant refusal | Tenant principal routes, tenant-scoped SQL owners, platform recovery owner, shared `Principals` client | Real-server journeys for tenant credential loss recovery, overlap rotation, non-escalation, and cross-tenant isolation recorded passing | PASS |
| `REQ-034`, `REQ-035`, `REQ-041`–`REQ-046`, `AC-011`, `AC-015`–`AC-017`: federated tenant/platform humans, pre-registration and pinning, grant-based platform authority, no tenant escalation | `wyrd-auth/src/platform_login.rs`, `platform_sessions.rs`, platform identity routes, tenant OIDC identity resolution | Identity journey 20/20 under its dependency-owning lane and platform escalation/scope-separation journeys recorded passing | PASS |
| `REQ-036`–`REQ-038`, `REQ-040`, `REQ-049`, `AC-009`, `AC-014`, `AC-019`: CLI/MCP/HTTP/OpenAPI and canonical audit/replacement behavior | `utoipa-axum` route registration; `/openapi.json`; shared CLI client; MCP tools; canonical `vala.audit_staging` append; removed bootstrap command and duplicate platform identity schema | Principals OpenAPI integration, CLI and MCP journeys, audit failure tests, `docs:check`, and boundary checks recorded passing | PASS |
| `REQ-048`, `AC-013`, `AC-018`: machine grants return no refresh token and all public/generated contracts describe that behavior | Runtime behavior in `TenantTokenIssuer::issue` returns `refresh_token: None`; however `TokenResponse.refresh_token` rustdoc and generated JSON schemas explicitly say API-key exchange issues one | Runtime no-refresh test passes, but `codegen:check` only reproduces the incorrect source description and therefore cannot prove contract accuracy | **FAIL — `TASK-REV-1`** |
| `REQ-012c`, `INV-013a`, `AC-020`, R10: RFC 8693 subject/actor direction, audience binding, semantic attenuation, one invoke-policy/audit path, shared helper | `DelegateToken::execute`, `policy_context`, `TenantGrant::Delegation::into_access_grant`, `PermissionSet::intersection`, `WyrdClient::on_behalf_of`, local audience verification | `query::service_b_acts_for_service_a_with_only_a_table_authority`; focused malformed/cross-tenant/policy/audit/intersection tests; Rust/Python/TypeScript client journeys recorded passing | PASS |
| R9: no CLI secrets in argv/debug, successful delegation credential attribution, UUID credential identifiers | Ambient credential chain in CLI; actor credential attached only to exchange audit; UUID response fields | CLI parser/secret-source tests, UUID OpenAPI/CLI journey, strict docs recorded passing | PASS |
| `R11-1`: production delegation has no preview gate or replacement flag | Preview configuration, error, boot/state gate, and public residue removed; production validation retains verifier/policy/audit checks | `production_auth_carries_no_preview_setting` and `production_valid_target_serves_token_exchange_without_preview`, one selected each | PASS |
| `R11-2`: delegated decisions record operation `auth.token.exchange`, permission `invoke`, exactly once | `exchange_audit_event` and `record_decision` preserve operation and overwrite only delegated permission/resource | Focused allow, deny, allowed-then-refused, and audit-failure selectors recorded passing | PASS |
| `R11-3`: directed A-to-B allow differs from B-to-A deny | Existing `PolicyHook` receives subject A and actor B; no delegation store or parallel policy model added | Primary Bifrost journey submits both directions and checks one denied reverse decision | PASS |
| `R11-4`: Python/TypeScript Bifrost consume an existing delegated `WyrdClient`; normal environment resolution is unchanged | Thin PyO3/N-API projections call Rust `Bifrost::connect_with_config`; optional client conflicts with transport options; Rust interface unchanged | Python integration 2 selected, TypeScript integration 1 selected, plus full Python 57 and TypeScript 18 integration tests recorded passing | PASS |
| `R11-5`: native-ingest audit retains actor attribution | `PostgresGateAudit::append_write_decision` projects the verified chain only when non-empty | Primary journey checks denied delegated write under A with B in detail and direct B write with no delegation detail | PASS |
| `R11-6`: query guidance uses ambient credentials | Bifrost reading guide removed the secret-valued argument and names ambient resolution | CLI secret-option focused tests and `docs:check` recorded passing | PASS |
| `R9-2`: card-bound issue-key credential ID remains UUID through response/schema/consumer | `IssueKeyResponse.key_id: Uuid` and direct UUID projection | UUID OpenAPI contract and real CLI issue-key journey recorded passing | PASS |
| `R11-7`: R9/R10 Rust items carry substantive rustdoc/error/panic contracts | Diff-scoped documentation commit covers the affected Rust owners and tests without lint suppression | `check:docs` and affected focused selectors recorded passing | PASS |
| `R11-8`: TypeScript authority permits the thin shared-client projection without permitting duplicate transport/auth | TypeScript guide now permits wrapping shared `WyrdClient` and retains the `QueryClient`/transport duplication bans | `ts:typecheck` recorded passing; semantic wording verified from the candidate diff | PASS |
| Approved non-goals and constraints: no new identity/Card kind, RBAC engine, delegation role/store, verifier trait/factory, compatibility surface, request-time tenant DB auth, or second audit sink | Searches of the candidate show no production `delegation:issue`, `requested_subject`, revocation-check/cache/epoch type, or compatibility route; old epoch column exists only in add/drop migration history | Boundary/audit/codegen checks recorded passing | PASS |

## Open Questions

None. The remaining correction is bounded and does not require a specification,
architecture, security, persistence, or public-interface decision.

## Verification notes

- Reviewed the complete immutable base-to-candidate range, current source and
  caller paths, the approved revision-14 specification, original tasks, and all
  cumulative remediation tasks through R11.
- The implementation evidence records positive focused selections for every R11
  row and passing owning lanes, including real Python/TypeScript delegated-client
  journeys, the 20-test dependency-owned identity lane, strict rustdoc,
  `codegen:check`, `docs:check`, and `git diff --check`.
- The later full gate and ungated-lane runs strengthen confidence but do not
  cure `TASK-REV-1`: generation faithfully reproduces the wrong public source
  description.
- Per the current user override, gate-enablement and other otherwise unrelated
  test/production changes were not classified as drift; they were inspected
  only for reachable regressions.
- The approved five-minute stateless-JWT revocation window was not treated as a
  finding.

## Overall result

**FAIL** — one bounded public-contract correction remains (`TASK-REV-1`).
