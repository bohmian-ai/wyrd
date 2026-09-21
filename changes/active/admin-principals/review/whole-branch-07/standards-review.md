# Repository Standards Review

## Review Findings

### Critical

None.

### Important

- **`REPO-R7-1` — architecture/security violation: same-second credential rotation can return an unusable successor token.**  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:198-213` stores a sub-second `now()` epoch, while `crates/shared/wyrd-auth-issue/src/lib.rs:570-594` truncates a newly exchanged token's `iat` to whole seconds and `crates/shared/wyrd-auth-verify/src/lib.rs:501-552` rejects that token when its `iat` is below the epoch.  This is a reachable production path: `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1169-1172` proves only that the surviving credential exchanges, not that the returned token is admitted, and the implementation evidence explicitly records the same-second refusal.  It violates `architecture/v1/00-foundations/service-identity.md:12-23` (overlap rotation has no unusable-credential window), `architecture/wyrd-security-posture.md:96-112`, and the current architecture's ordered-successor rule.  The observable consequence is that issue B → revoke A → exchange B can succeed yet the very next authenticated request with B's returned token is rejected.  Align revocation epoch and token issuance to one ordering precision so every predecessor is refused on the next request and a successor minted after the revocation is immediately admitted; close this with a real journey that exchanges the surviving credential in the same second, then uses that exact token on an authenticated route.

- **`REPO-R7-2` — public HTTP rejection bypasses the stable Wyrd error contract advertised for the new local download route.**  `crates/wyrd/wyrd-server/src/components/storage/routes.rs:448-479` documents every `400` as `WyrdProblem`/`application/problem+json`, but `Query(LocalDownloadQuery { path }): Query<LocalDownloadQuery>` rejects a missing or malformed query before the handler and returns Axum's plain-text rejection; the router has no rejection-to-`WyrdErrorResponse` mapper (`crates/wyrd/wyrd-server/src/http/router.rs:146-177`).  This violates `AGENTS.md` §9, `architecture/references/architecture/patterns.md` “Server Pattern,” and `architecture/references/languages/errors.md` “HTTP Errors,” all of which require public failures to use the single structured Wyrd error projection.  Agents and generated clients following `/openapi.json` can receive a body and media type outside the declared contract.  Handle the query rejection at the route boundary through the existing `WyrdErrorResponse` mapper (and do the same for the local upload path extractor if its decoding can reject before `parse_upload_id`), then add a served-route test asserting stable code and `application/problem+json` for missing/malformed locators; the current OpenAPI test at `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs:158-225` inspects only the document and cannot prove runtime rejection shape.

- **`REPO-R7-3` — the materially rewritten revocation owner still stores and accepts a raw application pool.**  `crates/wyrd/wyrd-auth/src/revocation_resolver.rs:28-43` defines `pool: Arc<PgPool>` and `SqlRevocationCheck::new(Arc<PgPool>)`, then directly acquires a `TenantConn` at line 67.  `architecture/agent-rules.md` permits only `TenantConn` and `OperatorPool` in library fields/signatures and makes pool construction/acquisition an owning-handle boundary; `architecture/references/architecture/patterns.md` repeats that tenant SQL must not accept a raw pool.  Keeping the raw pool in this newly rewritten security-critical owner bypasses the existing acquisition owner and its lifecycle/telemetry contract.  Store the existing `WyrdPostgres` owner (or receive tenant connections from that owner) and call its `tenant_conn` method; update the production and test assembly sites without introducing another pool wrapper, then prove the resolver's fail-closed and warm-token cases through the same production constructor.

- **`REPO-R7-4` — the new tenant admission queries duplicate RLS with hand-written tenant predicates.**  `crates/wyrd/wyrd-sql/src/queries/auth/revocation.rs:70-75` and `:97-102` add `data_tenant_id = wyrd.current_tenant()` inside queries that already require `&mut TenantConn<'_>`; both underlying tables have forced RLS tenant policies in `crates/wyrd/wyrd-sql/migrations/20260601000001_auth.sql:35-38` and `:97-100`.  This violates the explicit `architecture/agent-rules.md` TenantConn rule and `architecture/references/architecture/patterns.md` “Storage And Registry Pattern”: RLS is the tenant boundary and tenant-scoped queries must not maintain a parallel predicate.  The duplicate scope is currently equivalent but creates a second tenant-isolation expression that can drift from the connection authority.  Remove the manual predicates and rely on the existing RLS-bound `TenantConn`; retain focused user/service admission tests plus `mise run check:tenant-isolation`.

### Suggestions

None; optional cleanup and pre-existing untouched drift are excluded.

## Open Questions

None.  Each finding is resolved by current repository authority and does not require a new product or public-contract decision.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Cumulative base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Latest remediation range: `4d185da9cc2805940786416d0392f4c73bf337c2..ddd80c7a7b723a2f39a729e5aebe6b6072d2b983`
- Candidate was unchanged at report completion.

## Authority Coverage

| Changed surface | Applicable authority read | Coverage/result |
|---|---|---|
| Active spec, tasks, review packets, evidence, and architecture prose | `AGENTS.md` §§1, 12, 14; `architecture/agent-rules.md`; reference router; `languages/spec-driven-development.md`; `languages/implementation-execution.md` | Packet is tracked and evidence is mapped; PASS except the behavior/proof gaps above. |
| Principal, credential, tenant lifecycle, OIDC, revocation, authenticated context | `architecture/wyrd-design.md` runtime identity; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/v1/00-foundations/service-identity.md`; `AGENTS.md` §§2, 9 | Plane separation and fail-closed resolution are present; FAIL `REPO-R7-1`. |
| Tenant and operator SQL, migrations, RLS, transactional audit staging | `architecture/agent-rules.md` SQL/audit rules; `architecture/references/architecture/patterns.md`; `architecture/operations/deployment-and-release.md`; `AGENTS.md` §§2-3, 9 | Transaction and migration ownership otherwise conform; FAIL `REPO-R7-3` and `REPO-R7-4`. |
| HTTP routes, served OpenAPI, stable errors, local storage transfer | `AGENTS.md` §§9, 11; `architecture/references/architecture/patterns.md`; `languages/errors.md`; `languages/testing-workflows.md`; `languages/agent-harness.md` | Co-registration and binary schemas conform; FAIL `REPO-R7-2`. |
| MCP principal tools and shared wire schemas | `AGENTS.md` §§2-3, 9; `wyrd-doctrine.mdx`; `patterns.md`; `languages/agent-harness.md`; `languages/errors.md` | Typed UUID inputs/outputs, shared contracts, scope gating, and runtime schema tests conform; PASS. |
| CLI and shared Rust client projections | `AGENTS.md` §§2-4, 9; `wyrd-doctrine.mdx`; `patterns.md`; `languages/rust-core.md`; `languages/errors.md` | Client surfaces project server contracts without moving durable behavior; PASS. |
| Rust source, manifests, dependencies, async boundaries, documentation | `AGENTS.md` §§3-6, 12; `architecture/agent-rules.md`; `languages/rust-core.md` | Removed cache/listener dependencies are justified and async functions await IO; bare declaration aliases and rustdoc evidence pass, subject to raw-pool finding. |
| Audit staging/schema/projection and Vala changes | `AGENTS.md` §§2-3, 10; `architecture/agent-rules.md`; `patterns.md` Audit Pattern; `wyrd-security-posture.md` | One canonical append/publisher and tenant-qualified retained history remain; PASS. |
| User journeys, integration tests, codegen/docs/check lanes | `AGENTS.md` §11; `languages/testing-workflows.md`; `languages/spec-driven-development.md` | Required lane classes are represented and reported green; runtime malformed-locator and same-second-successor proof are missing as described above. |

## Rule-by-Rule Results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Principal identity is independent of credentials; planes remain disjoint | Typed `PrincipalId`/`PrincipalKindTag` contracts and separate tenant/platform route owners in the cumulative diff | PASS |
| Revocation is observed no later than the next request, including verified-token cache hits | `SqlRevocationCheck::epoch` at `revocation_resolver.rs:59-93`; warm-token journey at `platform_admin_e2e.rs:1049-1173` | PASS |
| Overlap rotation leaves another credential immediately usable | Sub-second epoch plus whole-second `iat`, locations in `REPO-R7-1` | **FAIL** |
| Public HTTP errors are stable problem+json through the single mapper | Local query extractor at `storage/routes.rs:475-479` can reject outside the mapper | **FAIL** |
| Public HTTP/OpenAPI routes are typed and co-registered | `storage_router` uses `routes!` at `storage/routes.rs:34-49`; local operations have typed/binary `utoipa` declarations; served-document test exists | PASS |
| Agent-facing MCP schemas match runtime parsing | `ListCredentialsArgs` and `RevokeCredentialArgs` use `PrincipalId`/`Uuid` in `wyrd-spec/src/auth/tenant_principals.rs:76-105`; MCP schema/output tests validate actual payloads | PASS |
| Library SQL boundaries use owning handles, `TenantConn`, or `OperatorPool`, never raw pools | `SqlRevocationCheck` field/constructor at `revocation_resolver.rs:28-43` | **FAIL** |
| `TenantConn` relies on RLS without a manual tenant predicate | New admission SQL at `revocation.rs:70-75,97-102` | **FAIL** |
| TenantConn callees do not prematurely commit/rollback | Changed SQL query functions operate through `conn.transaction()` and leave lifecycle to callers; no new `&mut TenantConn` callee commit was found | PASS |
| Every evaluated permission decision uses the canonical append and commits/refuses with its decision | Changed no-effect branches commit the existing decision transaction; evidence test asserts one allowed row and zero effects | PASS |
| No new cache/listener/blacklist or speculative dependency remains | Epoch cache, listener, notification fan-out, `moka`, and unused `tokio-util` dependency removed | PASS |
| Generated contracts are source-derived; OpenAPI is tested from the served router | Typed DTO changes feed codegen; `pg_openapi_contract` reads `GET /openapi.json`; reported `codegen:check` passes | PASS |
| New/materially changed Rust is documented and fallible functions state errors | Candidate rustdoc audit reported clean apart from documented script exclusions; inspected remediation symbols have intent and `# Errors` where fallible | PASS |
| No gate was weakened or bypassed | No production lint allowance was added; ignored tests are journey-lane tests with explicit environment ownership; required lanes were reported independently | PASS |
| No generated artifact, secret, machine path, compatibility alias, or legacy vocabulary was introduced | Cumulative diff and reported codegen/docs checks | PASS |

## Verification Notes

Reviewed the appended evidence table rather than treating it as implementation proof.  It reports passing `fmt:check`, workspace lints, client-tier/unwrap/clippy-allow/tenant-isolation checks, shared/principal/SQL/storage/Bifrost tests, platform/identity/CLI/MCP journeys, codegen, examples, docs, and a clean cumulative `git diff --check`.  The Python-backed checks used a local `python` → `python3` shim; no repository source was changed for that substitution.

The evidence does not close `REPO-R7-1`: the new journey stops after credential exchange and never spends the successor token.  It also does not close `REPO-R7-2`: the OpenAPI test asserts documented response schemas, not the runtime extractor rejection.  `REPO-R7-3` and `REPO-R7-4` are explicit source-shape rules not enforced by the reported gates.

## Overall Result

**FAIL** — four material repository-authority violations remain.
