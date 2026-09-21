# Admin principals whole-branch review 06 — task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate / reviewed HEAD: `2c0408b683f7a548cec6dd08b35698d761d33b31`
- Product-code candidate: `5ecc8a4e5a3a76390bc32f00353e558ea6a685d2`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 10,
  status `approved`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior review: `changes/active/admin-principals/review/whole-branch-05/`
- Remediation task:
  `changes/active/admin-principals/review/whole-branch-05/TASK-001-008-R5-close-validated-findings.md`

The complete cumulative base-to-candidate diff, the current source and callers,
the original tasks, the prior finding ledger, the R5 remediation task, and its
recorded verification evidence were inspected. The implementer's closeout
narrative was not treated as acceptance evidence. HEAD matched the candidate
before this report was written.

## Overall result

**FAIL**

Eight R5 roots are closed. The OpenAPI root remains open in a narrower,
reachable form: two authenticated local-storage routes are intentionally mounted
without route annotations or OpenAPI registration. Revision 10 requires the
canonical document to describe every served public route, and the R5 task
requires served method/path operations to come from co-registration. No
specification revision is needed; the exception is a bounded implementation
correction.

## Material proposed findings

### `TREV-WB06-1` — VIOLATION — local blob routes remain outside the canonical OpenAPI owner

- **Violated obligation:** `REQ-049`, `AC-014`, `AC-019`, original TASK-008's
  independent-client outcome, and R5 stable `FIND-admin-principals-13`'s
  acceptance requirement that served method/path operations and the runtime
  OpenAPI document come from co-registration with no duplicate or omitted
  route knowledge.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/components/storage/routes.rs:35-55`,
  `crates/wyrd/wyrd-server/src/http/openapi.rs:110-117`, and
  `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:114-146`.
- **Evidence and reachability:** `storage_router` returns an
  `OpenApiRouter`, but its local-backend branch mounts
  `PUT /cards/upload/local/{*id}` and
  `GET /cards/download/local/{*path}` with plain `.route(...)` calls and an
  explicit comment saying they are not documented. These are not dormant
  helpers: the local backend returns these URLs to the client, the shared
  storage client dispatches them, and `storage_e2e.rs` drives both paths. The
  runtime document is composed only from the `routes!` registrations. Its
  contract test checks a selected path list and omits both local operations, so
  it passes while the served document is incomplete. This also contradicts
  `WyrdApiDoc`'s invariant that a served method cannot exist without being
  documented.
- **Observable consequence:** an independent client generated from
  `/openapi.json` cannot implement the repository's supported local-development
  artifact upload/download flow, even though the server serves and its own
  client consumes that flow. A future method or error change on those routes can
  drift with no OpenAPI closure failure.
- **Required testable correction:** delete the undocumented-route exception.
  Make the local blob locator representable by OpenAPI (for example, an opaque
  single-segment identifier or a query parameter rather than an Axum wildcard),
  update the local URL producer/consumer at that same owner boundary, annotate
  the typed success and reachable problem responses, and register both methods
  through `utoipa_axum::routes!`. Do not add a manual path entry, compatibility
  route, snapshot, or route parser. Extend the assembled-server contract proof
  to assert both local operations are present with the real methods, parameters,
  authentication, bodies, and stable errors, and retain the existing local
  client/server round trip.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-001 / `REQ-001`–`REQ-011`, `REQ-039`, principal and credential separation, five kinds, plane-valid tenancy, optional Card binding, verifier-only multi-credential lifecycle | Runtime/spec types; platform and tenant migrations; principal-generic issuance, lookup, verification and revocation owners | Recorded principal unit/integration, shared, SQL and codegen lanes; no contrary current source path found | PASS |
| TASK-001 constraints and non-goals: `wyrd-spec` remains IO/async/PyO3-free; platform uses `OperatorPool`, tenant data uses `TenantConn`; no RBAC redesign or parallel credential stack | Dependency graph and cumulative source; existing Argon2 and credential owners are reused | Recorded client-tier, tenant-isolation, lint and SQL checks | PASS |
| TASK-002 / `REQ-012`–`REQ-019`, `REQ-031`: one verified two-plane context; request authorization derives from verified Wyrd claims; plane and tenant separation; synchronous permissions | `AuthContext`, tenant and platform extractors, verifier, permission and authorization owners | Recorded shared, principal, platform, identity and protected-route evidence | PASS |
| TASK-002 audit and revocation acceptance: allowed/denied decisions are staged transactionally; unauditable decisions fail closed; credential/principal/role changes retire stale authority | Canonical append paths; platform and tenant decision transactions; tenant epoch and per-request platform re-reads | R5 same-second role-withdrawal proof plus platform/no-effect journey evidence | PASS |
| TASK-003 / `REQ-020`–`REQ-024`, `AC-001`: explicit one-time initialization, disclosure before commit, concurrency refusal, failure retryability, no server-start secret | `boot/init.rs`, command dispatch and fallible terminal boundary | Recorded platform journey twice, concurrency and writer-failure cases | PASS |
| TASK-003 non-goals: no initialization HTTP route, tenant creation, or platform OIDC in the initialization owner | Router and boot-command inspection | Source inspection | PASS |
| TASK-004 / `REQ-025`–`REQ-028`, `AC-002`, `AC-007`, `AC-008`: provisioning, readiness/admission, failure/retry/concurrency, suspension/resumption, bootstrap removal | Provisioning service, tenant lifecycle/admission migrations and platform routes | Recorded platform and CLI journeys, durable-stage failure cases, and directory/admission tests | PASS |
| TASK-004 non-goals: no billing, organization, signup UI, deletion/destruction, legacy bootstrap alias, fabricated Card, or synthetic operator | Complete cumulative diff and final tree | Source and documentation inspection | PASS |
| TASK-005 / `REQ-029`–`REQ-031`, `REQ-046`, `AC-004`, `AC-012`: tenant-admin configuration, restricted machines, tenant-only grants, RLS isolation, no platform escalation | Tenant admin/principal routes and `TenantConn` queries | Recorded principal integration, identity and operator journeys; tenant-isolation check | PASS |
| TASK-005 transactional audit, including stable no-effect issuer/binding deletion | Decision and mutation share the tenant transaction; R5 commits an allowed decision before the stable not-found response | `an_authorized_request_that_changes_nothing_still_records_the_decision` and tenant admin SQL coverage recorded green | PASS |
| TASK-006 / `REQ-006`–`REQ-010`, `REQ-032`–`REQ-033`, `AC-005`, `AC-006`: metadata-only lifecycle, overlap rotation, independent revocation, same-principal tenant recovery, distinct recovery authority | Tenant credential routes/client handles and platform recovery owner | Recorded principal/platform/CLI journey evidence | PASS |
| TASK-006 revocation immediacy and shared renewal: revoked tokens retire; tenant credential revoke re-exchanges and replays once, then stops | Epoch owners and `Principals::revoke_credential` through `request_json` | Two exact shared-client first/second-401 tests recorded green | PASS |
| TASK-006 non-goals: no credential UI/sharing/delegation change or application-level global-root recovery | Final public surfaces and diff | Source/documentation inspection | PASS |
| TASK-007 / `REQ-034`–`REQ-035`, `REQ-041`–`REQ-046`, `AC-011`, `AC-015`–`AC-017`: tenant/platform humans, pre-registration, one-time pin, independent scopes, platform grants, provider independence and secret redaction | Tenant callback; platform connection, identity, login and session owners | Recorded identity journey and platform journey evidence | PASS |
| R5 `FIND-admin-principals-R4-3`: equal-second predecessor refused, explicitly ordered successor admitted, unchanged login stable | Next-whole-second epoch returned to the callback and used as successor `iat`; verifier retains strict-older rejection | Exact issuer/verifier/callback proofs and identity journey recorded green | PASS |
| R5 `FIND-admin-principals-R4-5`: first federated pin, grant and audit are atomic and use stored kind | `PlatformSessions::issue_federated` owns one audited transaction; both credential and federated events carry the stored kind | Append-failure/retry attribution test and platform journeys recorded green | PASS |
| R5 `FIND-admin-principals-R4-9`: MCP retains rmcp framing but owns no Wyrd bearer/header/refresh policy | MCP delegates Wyrd decoration and bounded replay to `HttpTransport::authenticated_replay`; local code only extracts rmcp status | Shared-client and real MCP first/second-refusal evidence recorded green | PASS |
| R5 `FIND-admin-principals-13`: every served method/path and reachable error comes from one runtime `utoipa` contract, with no omitted public route | Most route modules now use `OpenApiRouter`/`routes!`, and `/auth/token` declares reachable 500/503 failures; local blob routes are explicitly mounted outside registration | Contract suite proves selected composed paths and token audit failure, but does not inspect the two local routes | **FAIL — `TREV-WB06-1`** |
| R5 `FIND-admin-principals-R5-1`: candidate-added declaration types use imports and bare names | R5 declaration-only cleanup across touched modules; no contrary candidate-added declaration found in the inspected remediation diff | Recorded format and lint lanes | PASS |
| R5 `FIND-admin-principals-R5-2`: stable authorized no-effect responses commit exactly one decision and no effect; store failures remain atomic | Platform credential/identity/status and tenant issuer/binding routes commit their existing decision before stable logical responses | Focused real-server no-effect audit journey recorded green | PASS |
| R5 `FIND-admin-principals-R5-3`: shipped Vala migration bytes remain immutable and credential attribution is forward-only | Base migration is byte-identical; `20260910000027_audit_staging_credential_id.sql` adds one nullable column | Digest and staged-row upgrade tests plus SQL/Bifrost SQL lanes recorded green | PASS |
| R5 `FIND-admin-principals-R5-4`: tenant credential revoke uses existing bounded renewal/replay and preserves 204 success | `Principals::revoke_credential` uses shared `request_json::<(), ()>` | Exact first-401 replay and terminal second-401 tests recorded green | PASS |
| R5 `FIND-admin-principals-R5-5`: principal MCP tools advertise and consume shared input/output DTO schemas | DTOs live in `wyrd-spec`; MCP derives schemas from and deserializes/serializes those same types | Real MCP catalog/result conformance journey recorded green | PASS |
| TASK-008 / `REQ-036`, `REQ-047`–`REQ-049`, `INV-015`, `AC-013`, `AC-014`, `AC-018`, `AC-019`: CLI and MCP project the shared client contract; one header/renewal owner; independent client can implement the served HTTP surface from OpenAPI | CLI and MCP consolidation is present; runtime OpenAPI still omits reachable local-storage operations | CLI/MCP/shared-client lanes are credible; OpenAPI proof is incomplete for the reachable local flow | **FAIL — `TREV-WB06-1`** |
| TASK-008 prohibited changes and non-goals: no Python/TypeScript admin binding, restored ignored Rust journey, platform-via-tenant extractor, platform epoch, compatibility alias, UI, or widened tenant access | Complete diff and final tree | Source inspection and recorded client-tier/codegen evidence | PASS |
| `REQ-037`, `AC-009`: administrative authorization decisions are attributable to real principal, credential where applicable, permission/resource/tenant/outcome and fail closed | Canonical audit append; R5 platform-human kind correction and stable no-effect commits | Audit, platform, principal, identity and publication evidence recorded green | PASS |
| `REQ-038`, `REQ-040`, architecture/docs obligations: removed bootstrap model stays absent; operator, SaaS, rotation and recovery documentation describes the shipped model | Current architecture and public docs | Recorded docs and codegen checks | PASS |
| Global non-goals and material constraints: no new RBAC engine, Card kind, billing/org/deletion/migration product, audit sink, cache/blacklist, second transport, OpenAPI YAML/snapshot/generator, Python/TypeScript admin binding, UI, or compatibility shim | Complete base-to-candidate inventory | Diff inspection | PASS |
| Verification scope `VER-001`–`VER-006`: focused capability proof, no aggregate gate requirement, generated/boundary checks retained | R5 packet records the mandated exact and scoped lanes; `mise run gate` was not substituted | Evidence is broad and credible for its selected paths, but green lanes cannot override the OpenAPI omission | PASS with the explicit gap above |

## Prior-finding closure

R5 closes `FIND-admin-principals-R4-3`, `FIND-admin-principals-R4-5`,
`FIND-admin-principals-R4-9`, and `FIND-admin-principals-R5-1` through
`FIND-admin-principals-R5-5` at their named boundaries. Stable
`FIND-admin-principals-13` remains open only through `TREV-WB06-1`.
Earlier findings recorded closed by whole-branch-05 remain closed; the owner
waiver of `FIND-TASK-001-10` and the withdrawn historical audit-compatibility
proposal are unchanged.

## Verification limits

The R5 packet records format, lint, boundary, shared, principal, platform,
identity, CLI, MCP, SQL, Bifrost, storage, codegen, docs, rustdoc, exact-test,
migration-upgrade, and diff-hygiene evidence as passing. This review did not
rerun the long environment-owning suites. The recorded commands are credible
for their selections, and current source inspection confirms eight remediation
roots. They do not prove `TREV-WB06-1`: the assembled-document test deliberately
checks a selected path set, while the existing local storage journeys prove only
that the undocumented routes work.
