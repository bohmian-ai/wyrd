# Security, RBAC, and Tenancy Domain Review

## Reviewed Boundary

- Immutable comparison: base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599` in `/tmp/wyrd-task-002-r2-review-f345cd8a4`.
- Approved authority: `changes/active/surfaces-oracle-integration/spec.md`, revision 7; original `TASK-002-converge-client-and-sdks.md`; both prior task-review cycles; and `TASK-002-R2-close-r1-review-findings.md`.
- Reviewed security boundary: client credential-error projection and redaction; public authentication and authorization ordering; caller-derived tenant propagation and lifecycle IDOR resistance; canonical decision audit behavior; external OIDC issuer trust resolution; system-owner identity in JWT, Postgres, WAL, and Bifrost paths; migration and deployment compatibility; and generated refusal metadata.
- Review posture: practical, review-only security audit of the complete cumulative candidate. No implementation source was modified.

## Authority and Source Coverage

| Authority | Security obligation examined | Principal source coverage | Result |
|---|---|---|---|
| `AGENTS.md` §§2, 7-9, 11-12; `architecture/agent-rules.md` | Server-derived tenant, protected public boundaries, safe credentials, canonical fail-closed decision audit | shared client error/auth/transport owners; server router, Bifrost and query services; OIDC/auth resolver paths | PASS, except the out-of-scope trust-boundary changes below |
| `architecture/wyrd-security-posture.md` | Verified identity and tenant; typed authorization; secret redaction; issuer allowlisting; system audit attribution; canonical audit | `/v1` and `/mcp` middleware, Bifrost/query services, `wyrd-auth-verify`, OIDC/Postgres resolvers, system-owner users | PASS for runtime controls; scope/persistence findings remain |
| `architecture/wyrd-design.md`; `architecture/bifrost-design.md` | Server-owned authorization, tenant-qualified Bifrost state, system audit sentinel, shared SDK facade | Bifrost catalog/query routes and services, `DataTenantId`, Scribe WAL/hot-stage/catalog bindings | FAIL: candidate changes the durable system-tenant authority outside approved TASK-002/R2 scope |
| `architecture/operations/deployment-and-release.md` | Migrations are immutable; checksums and compatibility gate rollout | Wyrd system-tenant seed migration; Vala coordination and reader-authority migrations | FAIL: previously existing migrations are edited in place |
| `architecture/references/{doctrine/architecture-constraints,languages/errors}.md` | One safe catalog projection across SDKs; no client-selected tenant | `wyrd-client/src/error.rs`, Cards/Bifrost conversions, Python/TypeScript tests | PASS |
| Spec REQ-013, REQ-019A, REQ-023-REQ-025, REQ-026B, REQ-054, REQ-059, INV-005, AC-002, AC-006; original TASK-002 and R2 task | Credential/error parity, tenant isolation, correct system attribution, preserved auth/audit behavior, no persistent/auth boundary work in R2 | complete base-to-candidate source and recorded focused evidence | FAIL due `SEC-R2-001` and `SEC-R2-002` |

## Prior-Finding Closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| `FIND-TASK-002-1` | The existing shared conversion at `crates/shared/wyrd-client/src/error.rs:89-121` now emits only `{field, reason}`, `{transport}`, or `{}`. Cards, Bifrost, Python, and TypeScript continue to reuse that owner; raw transport text is not added to `details`. | PASS |
| `FIND-TASK-002-3` | All non-success/default responses on the seven Bifrost table/query/lifecycle operations declare `application/problem+json`, and the generated contract matches the one runtime error mapper. | PASS |
| Security closure from the prior R1 review (`SEC-001` through `SEC-003`) | Shared credential error identity, TypeScript construction projection, and typed refusal publication remain closed in the cumulative candidate. | PASS |

## End-to-End Security Result

| Boundary | Result | Evidence |
|---|---|---|
| Credential handling and error redaction | PASS | `SecretString` remains the secret-bearing boundary; missing credentials expose `{}`, configuration exposes the established field/reason pair, and transport details expose only the `http`/`grpc` discriminator. Presigned external transfers remain credential-free and authenticated requests retain the same-origin fence. |
| Public authentication, RBAC, and tenant isolation | PASS | `crates/wyrd/wyrd-server/src/http/router.rs:41-73` default-denies the complete `/v1` and `/mcp` surfaces. Bifrost/query services authorize before resource access, derive tenant solely from `Caller`, scope lifecycle controls by tenant plus `RequestId`, and retain fail-closed authorization audit ordering. |
| OIDC issuer trust | FAIL (scope) | `afdc3b798` changes the production authentication resolver API and both external-token/login lookup paths from tenant-list filtering to a keyed lookup. The keyed SQL remains parameterized and RLS-scoped, so no cross-tenant exploit was found, but this trust-boundary change is unrelated to the five approved R2 corrections. See `SEC-R2-002`. |
| System audit tenant and durable identity | FAIL | `5cfe7b6b9` changes `DataTenantId::SYSTEM_OWNER`, JWT decoding, API-key parsing, Bifrost/WAL decoding, Postgres seed data, and two Vala migrations. This violates R2's preserved-persistence boundary and the immutable migration contract. See `SEC-R2-001`. |
| Injection and unsafe deserialization | PASS | Reviewed changes add no SQL interpolation, command/template execution, path traversal, or unsafe deserializer. The keyed issuer query binds the unverified `iss` as a SQL parameter and executes under `TenantConn` RLS. |
| Generated authorization refusals | PASS | OpenAPI media types now agree with runtime `application/problem+json`; 401/403 responses retain `WyrdProblem` rather than exposing internal error strings. |

## Verification Limits

- Static inspection covered the complete cumulative range and every new auth/OIDC/system-tenant change after the previously reviewed `4d9d74b34` candidate.
- I did not rerun the implementer's recorded focused or broader verification. The R2 evidence reports the client-error and OpenAPI focused proofs plus formatting, language, codegen, docs, and boundary lanes passing.
- That evidence does not establish approval or deployment compatibility for the later system-tenant identity rewrite or OIDC authentication optimization; neither change belongs to the R2 acceptance matrix, and no migration-upgrade proof is recorded.
- The candidate remained `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599` and the immutable worktree remained clean throughout this review.

## Material Proposed Findings

### SEC-R2-001 — VIOLATION — R2 changes the durable system tenant and edits migrations in place

- Violated obligation: `TASK-002-R2` requires credentials/redaction, authz/audit ordering, tenant isolation, and persistent state to remain unchanged; its non-goals exclude persistent-state work outside the five validated corrections. `architecture/operations/deployment-and-release.md:128-151` independently requires immutable migrations and checksum verification. The approved specification authorizes in-place changes only for explicitly unshipped audit-publication state; it does not approve replacing the platform tenant identity across authentication, Postgres, and durable Bifrost formats.
- Exact location: `crates/wyrd-spec/src/ids.rs:142-149`; `crates/wyrd/wyrd-sql/migrations/20260601000015_seed_system_tenant.sql:13-35`; `crates/vala/vala-sql/migrations/20260910000013_oracle_coordination.sql:10`; `crates/vala/vala-sql/migrations/20260910000025_oracle_reader_authority.sql:76`; `crates/shared/wyrd-auth-verify/src/lib.rs:843-856`; plus the system-owner decode and storage checks changed by commit `5cfe7b6b9`.
- Evidence: after the prior reviewed candidate, `DataTenantId::SYSTEM_OWNER` changes from nil to fixed UUID `00000000-0000-7000-8000-000000000000`; the access-token marker compatibility path is deleted; API-key, Scribe WAL, hot-stage, table-binding, platform-tenant, and audit code are rewritten around the new value; and three existing migration files replace the old UUID rather than adding an ordered transition. None is required to emit the R2 client detail keys, correct OpenAPI media types, add rustdoc/imports, or repair the reading guide.
- Observable consequence: a database that recorded the earlier migration checksums fails the deployment checksum gate, while bypassing that gate leaves the old `wyrd-system` row and durable nil-tenant WAL/state incompatible with the new decoder and identity checks. The result is startup/readiness refusal or loss of audit-publication/recovery availability. Because this is the security-event attribution tenant, treating the mismatch as ordinary cleanup risks an audit continuity boundary rather than only a test-fixture inconvenience.
- Required testable correction: remove the system-tenant identity and in-place migration rewrite from the TASK-002 cumulative candidate. If the identity correction is still desired, route it as a separately approved persistent-data/security change with an ordered migration/compatibility decision and upgrade/restart/audit-recovery proof; do not weaken checksum enforcement or accept both identities ad hoc inside this remediation.

### SEC-R2-002 — DRIFT — R2 includes an unrelated production OIDC trust-resolver optimization

- Violated obligation: TASK-002/R2 is limited to shared-client convergence and five named corrections, preserves authentication/credential behavior, and explicitly excludes cleanup outside validated cumulative touched modules. Security-posture issuer trust is a live authentication boundary and cannot be changed incidentally to an error-details/docs remediation.
- Exact location: `crates/shared/wyrd-auth-oidc/src/config.rs:37-60`; `crates/shared/wyrd-auth-verify/src/lib.rs:564-573`; `crates/wyrd/wyrd-auth/src/issuer.rs:22-42`; `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:115-148` (commit `afdc3b798`).
- Evidence: the candidate adds a new `IssuerConfigResolver::trusted_issuer` capability, overrides it with a production Postgres lookup, and switches external-token verification and login issuer resolution from the existing tenant-list mechanism. The query is parameterized and executes through tenant-bound RLS, so inspection found no injection or tenant bypass; the defect is that this live trust-boundary behavior has entered an unrelated immutable task candidate without an acceptance obligation or its own security decision/evidence.
- Observable consequence: acceptance of TASK-002 would silently approve a changed authentication lookup and resolver contract whose failure/load behavior is not represented in TASK-002/R2 evidence. A regression in keyed issuer lookup could deny all external authentication for one tenant or misclassify resolver availability, while the task's green SDK/error lanes would not detect it.
- Required testable correction: delete the OIDC keyed-lookup optimization from this TASK-002 candidate and keep the existing tenant-scoped resolver path. If retained as separate work, review it under an auth-owned task with focused same-tenant, cross-tenant, unknown-issuer, resolver-unavailable, and login/external-token verification proofs; no new abstraction is required in TASK-002.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- `[crates/wyrd/wyrd-sql/migrations/20260601000015_seed_system_tenant.sql:16]` Existing migration and durable system-tenant identity are rewritten in place. A previously initialized deployment fails checksum verification or retains state the new JWT/WAL/tenant decoders reject, disrupting security-audit recovery and readiness. Remove this rewrite from TASK-002 and handle it through an approved ordered migration with recovery proof.

### Low / Defense In Depth

- No additional defense-in-depth recommendation is required for task acceptance. `SEC-R2-002` is a scope-drift finding, not an observed exploitable issuer-trust defect.

### Positive Controls

- `/v1` and `/mcp` remain default-deny authenticated, including unknown-route fallbacks.
- Bifrost permissions are typed; tenant identity comes from the verified caller; lifecycle lookup and cancellation remain tenant-qualified.
- Allowed and denied authorization decisions retain the canonical fail-closed audit path, and Oracle reads retain only the approved WAL-first exception.
- The OIDC keyed query uses parameter binding and `TenantConn` RLS; unknown issuers and resolver outages continue to fail closed.
- Client error `details` exclude raw transport failure text and credential material, while preserving actionable stable keys.
- Generated 401/403 and other pre-stream refusals now advertise the same problem media type emitted at runtime.

## Overall Verdict

**FAIL** — the requested R2 credential-detail and refusal-contract corrections are security-safe, and runtime authentication, RBAC, tenancy, audit ordering, and refusal projection remain fail-closed. The cumulative candidate nevertheless includes two material unrelated trust-boundary changes after the prior reviewed candidate: an unapproved durable system-tenant/migration rewrite and an OIDC authentication lookup optimization. Both must leave TASK-002 before it can pass.
