# TASK-010 R2 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Complete range: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa..7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Review role: repository standards only; task acceptance is intentionally not assessed here.

The candidate remained the stated commit during this review. `.codegraph/` is
absent, so ordinary repository navigation was used.

## Authority coverage

| Changed surface | Governing authority inspected | Coverage result |
| --- | --- | --- |
| OAuth/OIDC wire contracts, generated JSON schemas, stable OAuth exception errors, and served OpenAPI | `AGENTS.md` §§2, 8-9, 11-12, 16; `architecture/wyrd-design.md` runtime identity and public surfaces; `architecture/references/architecture/patterns.md`; `architecture/references/languages/errors.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| `wyrd-auth` login, callback, device, issuance, refresh, revocation, sealing, and audit workflows | `AGENTS.md` §§4-6, 9, 11, 16; `architecture/agent-rules.md` SQL, audit, async, test, and rustdoc rules; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md` | Complete |
| `wyrd-sql` auth migrations, queries, row types, and Postgres tests | `AGENTS.md` §§2-5, 9, 11, 15-16; `architecture/agent-rules.md` `TenantConn`/`OperatorPool` rules; `architecture/operations/deployment-and-release.md` migration contract | Complete |
| Server auth routes, boot/configuration, client registration, and platform routes | `AGENTS.md` §§2-6, 9, 11-12, 15-16; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/operations/deployment-and-release.md`; focused Rust/error/testing references | Complete |
| Shared Rust client, CLI, and Rust/Python/TypeScript journey consumers | `AGENTS.md` §§2-3, 8-11; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/testing-workflows.md` | Complete |
| Official NGINX gateway rate limit and Docker startup gate | `AGENTS.md` §§11-12, 15; `architecture/agent-rules.md` gate-integrity rule; `architecture/operations/deployment-and-release.md` supported topologies, gateway, and configuration authority; `architecture/wyrd-design.md:1323-1340` | Complete |
| Documentation and active change artifacts | `AGENTS.md` §§11-16; `architecture/references/languages/spec-driven-development.md`; applicable architecture and security authorities | Complete |

## Rule results

| Repository rule | Evidence | Result |
| --- | --- | --- |
| Durable behavior stays server-owned; `wyrd-spec` remains pure and client tiers do not gain server/SQL dependencies | Contracts remain in `wyrd-spec`; serving/orchestration stays in `wyrd-server`/`wyrd-auth`; shared-client edits project the HTTP contract. Current `mise run check:client-tier` exited 0. | PASS |
| Production SQL uses only `TenantConn` or `OperatorPool`; tenant-scoped callees do not own transaction completion; RLS is not duplicated | Changed auth query signatures use `&mut TenantConn<'_>`; server cross-tenant owners use `OperatorPool`; commits remain in workflow owners rather than `wyrd-sql` query functions. Current `mise run check:tenant-isolation` exited 0. | PASS |
| Authorization/login state and required audit effects share their owning transaction | Successful callback, device redemption, issuance, refresh, and revocation paths append through the canonical audit owner before caller-owned commit; best-effort records are limited to refusal outcomes that establish no authority. | PASS |
| New and materially modified Rust follows cohesive owner structs, narrow async IO boundaries, top-level imports, stable errors, and mandatory rustdoc | `HumanConnections`, `CliLogins`, `TenantTokenIssuer`, `RefreshTokens`, `OAuthClients`, and route owners retain cohesive dependencies and methods; changed fallible items carry `# Errors`; no new production `#[allow]` or `#[ignore]` was found. Recorded `mise run lints` is green and current `fmt:check` plus `check:unwrap-audit` exited 0. | PASS |
| Public OAuth errors use the approved RFC exception without creating a second general Wyrd error catalog | OAuth endpoints use typed `OAuthErrorCode`/`OAuthErrorResponse`; non-OAuth handlers retain `WyrdError` projection. | PASS |
| Generated schemas and served OpenAPI stay source-derived and verified | Schema changes are paired in source/generated snapshots; current `mise run codegen:check` exited 0. The task evidence records `test:principals:integration` green for `pg_openapi_contract`. | PASS |
| Public surfaces and documentation remain aligned | Rust client/CLI and first-class language journey consumers were updated to the form wire; docs describe the authorization-server flows. The task evidence records `docs:check`, Python lint, and TypeScript typecheck green. | PASS |
| Verification uses the narrowest task lanes; full integrated journeys are not required during remediation | The remediation records exact focused tests plus `test:principals:integration`, `test:sql`, codegen and boundary checks. Deferring full unfiltered/every-language journeys to change review matches the standing direction and `AGENTS.md` §11. | PASS |
| Gateway user-code limiting uses the existing native deployment boundary rather than a replica-local application mechanism | The candidate removes `tower_governor` and uses NGINX `limit_req` only for `POST /auth/device`, matching `architecture/operations/deployment-and-release.md:21-24`; the startup script checks isolation from other routes and client addresses. | PASS |
| Unreleased migrations may be edited within this unmerged change | The edited/deleted auth migrations were introduced inside this active change. Human lead direction in `review/TASK-004-r2/lead-direction-FIND-TASK-004-11.md` explicitly applies conventional immutability only after shipment; no new ALTER migration is required here. | PASS |
| A failing live gate is fixed at its cause and is not weakened to make it green | The production restart in `test:server:startup` previously exercised the no-implicit-tenant, multi-tenant profile. The candidate sets `WYRD_SERVER_TENANT_SLUG=acme` and a file KEK, thereby selecting the single-tenant exception instead of satisfying the multi-tenant Vault requirement. No other official-image startup lane preserves that proof. | **FAIL — REPO-R2-001** |

## Material findings

### REPO-R2-001 — VIOLATION: the startup gate bypasses the multi-tenant production/Vault boundary

- **Violated rule:** `AGENTS.md:560-571` and
  `architecture/agent-rules.md` prohibit weakening a failing gate and require
  fixing the diagnosed failure. `architecture/wyrd-design.md:1323-1340` fixes
  the live security boundary: multi-tenant production uses per-tenant Vault
  KEKs over HTTPS; owner-only files are allowed only for an explicitly
  single-tenant deployment. `architecture/operations/deployment-and-release.md:11-15`
  keeps both multi-tenant SaaS and single-tenant enterprise topologies supported.
- **Location:** `scripts/server/test-startup.sh:53-63,116-125,228-231` and
  `mise.toml:818-820`.
- **Evidence:** the startup lane's production restart runs the same
  `serving_env` used by its official-image journey. Adding
  `WYRD_SERVER_TENANT_SLUG=acme` makes `OperatorKeysConfig::validate` treat the
  process as single-tenant, and `WYRD_OPERATOR_KEK_SOURCE=file` then takes the
  explicitly permitted file-key branch (`crates/wyrd/wyrd-server/src/config.rs:1781-1846`).
  The task evidence diagnoses the red lane as production's Vault-only refusal;
  this change avoids that condition rather than exercising it. Repository
  search found no second official-image production startup lane that supplies
  Vault and preserves the removed multi-tenant proof. Unit tests of the
  configuration predicate and mock Vault reader do not replace that deployed
  image path.
- **Observable consequence:** `mise run test:server:startup` can be green even
  if the official image cannot start in the supported multi-tenant production
  topology, cannot consume its required Vault-backed per-tenant KEK, or
  regresses the production Vault composition. The lane still proves a valid
  single-tenant production deployment, but it no longer proves the live
  multi-tenant boundary it previously reached.
- **Testable correction:** keep the owner-only volume fix for the signing key,
  but restore the startup lane's multi-tenant production profile and satisfy
  the existing production key contract through the repository's existing
  Vault integration path. The production restart must become ready using an
  HTTPS Vault-backed per-tenant KEK and must fail when that required provider
  or active tenant key is unavailable. Do not add another key mechanism,
  configuration option, repository check, or broad aggregate. A separate
  single-tenant/file case is optional and cannot replace the restored
  multi-tenant case.

## Non-blocking notes

None. Placement, naming, structure, and wording-only observations were not
promoted to findings.

## Verification evidence and limits

- Current review execution: `git diff --check`, `mise run fmt:check`,
  `mise run check:client-tier`, `mise run check:tenant-isolation`,
  `mise run check:unwrap-audit`, and `mise run codegen:check` all exited 0.
- Task evidence records green `lints`, `docs:check`, `test:sql`,
  `test:principals:integration`, focused identity journeys, Python lint, and
  TypeScript typecheck.
- The review did not rerun the Docker startup lane. Its reported green result
  cannot clear `REPO-R2-001`, because the candidate changed the topology that
  result exercises.
- Full unfiltered identity and every-language journeys are intentionally left
  to change review; that is not a repository-standard gap for this task review.

## Overall result

**FAIL**

One material repository-rule violation remains: `REPO-R2-001`.
