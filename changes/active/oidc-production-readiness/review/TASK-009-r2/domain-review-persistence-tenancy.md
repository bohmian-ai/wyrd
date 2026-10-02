# TASK-009 r2 persistence and tenancy domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`
- Prior verdict and validated ledger: `changes/active/oidc-production-readiness/review/TASK-009-r1/{verdict.md,findings-validation.md}`
- Human direction: `changes/active/oidc-production-readiness/review/TASK-009-r1/lead-direction-FIND-TASK-009-5.md`; `FIND-TASK-009-5` is withdrawn and was not reopened.

The candidate remained at the stated commit during this review.

## Reviewed boundary and authority

| Boundary | Authority and source coverage | Result |
|---|---|---|
| Platform OIDC request/view contract and derived audience | Spec REQ-004, REQ-007, INV-003; `wyrd-spec/src/auth/platform_identity.rs`; platform configure/read handlers; `platform_connection_from_row`; `CodeRedemption` | PASS for the current wire/runtime shape: the independent human audience is absent and verification derives it from `client_id`. |
| Platform OIDC durable row and migration | Spec REQ-004, REQ-007, INV-004; initial platform schema; new `20261002000001_platform_oidc_client_audience.sql`; platform SQL queries and PG tests | FAIL — `PERSIST-TEN-002`. |
| Platform configuration discovery and process-owned provider state | Spec REQ-003/004/007 and TASK-009/R1 cache obligations; `configure_connection`; `discover_jwks_uri`; `ScreenedHttp::provider_metadata`; `PlatformLogin`; boot composition; served platform journey | FAIL — `PERSIST-TEN-001`. |
| Platform/tenant SQL capability separation | `AGENTS.md` and `architecture/agent-rules.md`; `OperatorPool`, `begin_platform_audited`, `TenantConn`; platform query signatures; workload admin and boot callers | PASS. Platform reads use `OperatorPool`; audited platform mutations use the operator-created transaction; tenant workload persistence remains on a tenant-bound `TenantConn`. No raw `PgPool` entered the changed production signatures. |
| Workload issuer discovery persistence | TASK-009 non-goal and R1 `FIND-TASK-009-3`; `ScreenedHttp::provider_metadata`; admin create; boot seeding; trusted-issuer query owner and tests | PASS. Both callers persist the discovery document's typed `jwks_uri` without fetching keys, while runtime key retrieval remains with the workload verifier. |
| Connection revision and secret persistence | Spec REQ-003/005/016; tenant human-connection resolver/query paths; platform sealing and upsert paths; migration column set | PASS within this diff. Tenant candidate/tested revision semantics are untouched. Platform secret bytes remain sealed and are neither projected nor dropped by the audience migration; the revised upsert binds the remaining columns in matching order. |

## Source trace

Platform configuration parses the issuer, then calls the shared workload helper at `crates/wyrd/wyrd-server/src/components/platform/identity.rs:228-237`. That helper now calls `ScreenedHttp::provider_metadata` and returns only the advertised URI at `crates/wyrd/wyrd-server/src/components/admin/routes.rs:735-775`. The helper's owner explicitly returns typed metadata with an empty key set and performs no JWKS request at `crates/shared/wyrd-auth-oidc/src/relying_party.rs:197-240`. The handler nevertheless persists the connection and commits it at `components/platform/identity.rs:239-267`. Served login later uses the separately composed process-owned `PlatformLogin`, whose `begin` enters `RelyingParty::cached` and therefore performs full discovery plus JWKS fetch (`wyrd-auth/src/platform_login.rs:131-170`; `wyrd-auth-oidc/src/relying_party.rs:377-435`). Configuration neither validates the keys nor refreshes that cache.

The platform audience migration consists only of an unconditional column drop at `crates/wyrd/wyrd-sql/migrations/20261002000001_platform_oidc_client_audience.sql:1-4`. The prior schema allowed `expected_audience` and `client_id` to differ (`20260601000023_platform_identity.sql:41-53`), and the reviewed base used the former for verification. After migration, `platform_connection_from_row` derives verification audience from `client_id` (`wyrd-auth/src/pg_resolvers.rs:592-624`). The analogous tenant human migration already establishes the repository's conventional safe-upgrade pattern by refusing mismatched persisted values before removing the duplicate contract (`20260925000000_auth_human_connections.sql:57-66`).

## Material findings

### PERSIST-TEN-001 — REGRESSION: platform configuration bypasses full relying-party discovery and its process cache

- **Violated obligation:** REQ-004 requires setup to discover provider endpoints and JWKS and validate the configured issuer; REQ-003 requires hosted changes to take effect without restarting replicas; TASK-009 and R1 require platform login to use the process-owned relying party/cache. R1 `FIND-TASK-009-3` restores metadata-only discovery only for workload setup.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/platform/identity.rs:228-267`; shared metadata-only helper at `crates/wyrd/wyrd-server/src/components/admin/routes.rs:735-775`; process owner and full fetch at `crates/wyrd/wyrd-auth/src/platform_login.rs:131-170` and `crates/shared/wyrd-auth-oidc/src/relying_party.rs:377-435`.
- **Evidence:** The same `discover_jwks_uri` helper serves both workload issuer creation and platform human configuration. R1 changed it from `RelyingParty::discover` to `ScreenedHttp::provider_metadata`, which intentionally does not fetch JWKS. The platform handler then commits the row without touching the process-owned platform relying party. A missing/unusable JWKS can therefore produce a successful configuration that fails on the first login. If the same issuer is reconfigured while cached, the old discovery document/key set can remain active until TTL or an unknown-key refresh, so the new durable configuration and the process's effective provider state disagree.
- **Observable consequence:** Operators can receive `200` and a durable platform login configuration that is not usable. Reconfiguration of an already cached issuer need not take effect for new login attempts immediately, despite the durable row having changed; old provider endpoints or keys can remain in use transiently.
- **Required testable correction:** Keep metadata-only discovery on the two workload setup callers. Route platform human configuration through the existing process-owned platform relying party's full discovery path so it validates discovery plus JWKS and replaces that issuer's cached provider before the durable configuration commits. Reuse the existing screened transport/cache; add no second cache, setting, probe, retry system, or provider-specific branch.
- **Closure proof:** Through the served platform configure route, prove unavailable/unusable JWKS returns a refusal and leaves no replacement row. Prime an issuer, change its advertised endpoint/key material, reconfigure that same issuer, and prove the next begin/callback uses the newly discovered provider state without a restart.

### PERSIST-TEN-002 — REGRESSION: the audience migration silently changes existing platform trust

- **Violated obligation:** REQ-004 makes human audience equal to client ID by construction, while INV-004 requires trust changes to remain fail closed. Durable upgrades must not silently reinterpret an existing configured trust value.
- **Exact location:** `crates/wyrd/wyrd-sql/migrations/20261002000001_platform_oidc_client_audience.sql:1-4`; prior persisted contract at `crates/wyrd/wyrd-sql/migrations/20260601000023_platform_identity.sql:41-53`; new consumer at `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:592-624`.
- **Evidence:** Before this migration the public contract and table permitted `expected_audience != client_id`, and the verifier consumed `expected_audience`. The migration drops that evidence unconditionally; the new resolver consumes `client_id`. Thus an existing row's accepted audience changes during upgrade without operator confirmation, and after the drop the mismatch cannot be diagnosed or repaired from stored state. The sibling tenant human migration already uses a preflight refusal for this exact mismatch.
- **Observable consequence:** An upgraded deployment can start accepting ID tokens for `client_id` that its persisted pre-upgrade configuration explicitly did not accept, while ceasing to accept the previously configured audience. This is a silent authentication trust change, not merely schema cleanup.
- **Required testable correction:** In the existing migration, use the repository's established migration-preflight pattern to abort before the drop when the singleton row has `expected_audience <> client_id`, with a non-secret repair instruction. Then drop the redundant column only for an absent or already-equivalent row. Add no compatibility field, fallback, option, or permanent check.
- **Closure proof:** A migration test starting from an equal-valued row succeeds and preserves issuer, client ID, authentication method, claim mapping, TTL, and sealed secret while removing only the duplicate column. A mismatched row makes the migration fail atomically, leaving the old row and column intact for repair.

## Verification limits

The remediation record reports green focused tests, `test:wyrd`, identity journeys, codegen, lints, and boundary lanes on candidate `0b516e235`, followed by a documentation-only commit. The reviewed tests prove fresh-schema request/view round trips, served callback behavior, metadata-only workload setup, cache reuse, and current-row replacement. They do not exercise an upgrade from a pre-existing mismatched platform audience, nor platform configuration with unavailable JWKS, nor same-issuer reconfiguration against an already populated process cache. Those missing proofs correspond directly to the two reachable findings above; no unavailable reviewer or missing subject evidence limited this review.

## Overall result

**FAIL** — tenant/platform SQL capability boundaries, revision handling, workload issuer persistence, and secret preservation remain sound, but two bounded persistent-configuration regressions remain: platform configuration no longer validates/primes the human relying party, and the destructive audience migration silently reinterprets existing trust.
