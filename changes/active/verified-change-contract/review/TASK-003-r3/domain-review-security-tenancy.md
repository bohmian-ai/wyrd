# TASK-003 r3 — Domain review: security, tenancy, and authorization

Reviewer: fresh Wave 1 security domain review.

Immutable subject:

- original base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- cumulative candidate: `a5a5b60f446981760ac831f64aba871582ec45e4`
- approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- prior verdicts, ledgers, and remediations: `review/TASK-003-r1/` and `review/TASK-003-r2/`

Candidate `HEAD` was the pinned cumulative candidate before and after this
review. `.codegraph/` is absent, so source and caller tracing used repository
search and direct inspection.

## Reviewed boundary

This review is limited to the cumulative TASK-003 security boundary:

- exact CardRef-to-principal resolution for API-key issuance, workload
  `jwt-bearer`, CardRef delegation, and the test credential helper after removal
  of the manual tenant predicate;
- `TenantConn` authority, forced RLS, transaction ownership, and cross-tenant
  visibility for machine principals and verification bindings;
- Operator-bearing registration authorization and the canonical transactional
  audit path for allowed and denied decisions;
- qualifying and excluded authentication grants, monotonic activity renewal,
  null-only schedule arming, current lifecycle admission, A/B versions, and
  shared replicas; and
- prior R1/R2 security findings and their cumulative remediation seams.

The TASK-004 SYSTEM issuer, scheduler, Verifier execution, result publication,
and Operator delivery do not exist in this candidate and are not TASK-003
findings. Their frozen binding and authorization inputs were reviewed where
TASK-003 creates them.

## Authority and source coverage

| Boundary | Authority and source/callers inspected | Result |
|---|---|---|
| Exact Card-bound principal resolution | Security posture principal and credential lifecycle; REQ-105/106/112; R1 `FIND-TASK-003-2`; R2 `FIND-TASK-003-12`; `queries/auth/service_accounts.rs:11-33,169-200`; callers in `issue_api_key.rs:86-137`, `jwt_bearer.rs:141-159`, `exchange_api_key.rs:350-365`, and `wyrd-testing/src/server.rs:2518-2573` | **PASS.** One shared query accepts exactly one active match, rejects ambiguity, and now carries no tenant predicate or tenant bind. |
| RLS and transaction authority | `architecture/agent-rules.md`; security posture tenant/data isolation; REQ-078/112 and INV-007; `tenant_conn.rs:15-73`; auth migration RLS at `20260601000001_auth.sql:97-101`; binding migration at `20260601000027_verification_bindings.sql:53-101`; verification SQL owners and callers | **PASS.** Runtime access remains on `wyrd_app`, transaction-local tenant state, and forced RLS. No reviewed production query accepts a raw pool or commits a caller-owned transaction. |
| Registration RBAC and audit | Agent rules transactional-audit requirements; security posture authorization/audit; REQ-145, INV-007, AC-018/030; R1 `FIND-TASK-003-1`; `cards/routes.rs:394-430`; `cards/service.rs:528-610,1019-1107`; `audit/mod.rs:183-248` | **PASS.** Operator-bearing requests evaluate `operators:invoke` once in addition to `cards:write`; denials are durable, and allowed rows commit with registration or are recorded standalone when no registration commits. |
| Authentication activity lifecycle | Security posture token lifecycle; REQ-105-108/112; AC-019/020; R1 `FIND-TASK-003-3/-8`; `issuance.rs:123-157,263-405`; API-key and workload callers; `verification.rs:56-109,319-484` | **PASS.** Only API-key and workload-JWT grants for an active Card-bound Service/Agent stamp activity; updates are monotonic, cursor arming is null-only, and current principal/Card lifecycle gates new work. |
| Cumulative migration and dependency posture | Repository supply-chain and migration rules; workspace/`wyrd-sql` manifests and lockfile; migration 27 | **PASS.** Schedule parsing dependencies are pinned and narrowly owned by `wyrd-sql`; the migration enables and forces RLS and grants only the required runtime operations. No source-local supply-chain or deployment-security issue was found. |

Authority review included `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/wyrd-security-posture.md`, the foundation security and principal
extraction references, testing guidance, approved spec revision 33, the
original task, the complete base-to-candidate diff, and both prior verdicts,
validation ledgers, security reports, and remediation records.

## Prior-finding closure

| Prior finding | R3 security-domain reassessment | Result |
|---|---|---|
| `FIND-TASK-003-1` | `register_card_http` detects effective non-empty `on_failure`, spends `operators:invoke` exactly once in addition to `cards:write`, records a denial before refusal, and passes allowed events into the registration transaction. The focused route proof distinguishes Operator-free, denied, and fully authorized registration and checks audit cardinality/rollback. | **CLOSED** |
| `FIND-TASK-003-2` | `service_account_by_card_ref` fetches at most two active rows and returns a principal only for exactly one match. API-key issuance, workload `jwt-bearer`, CardRef delegation, and the fixture helper all use this owner. Explicit-space/UID references remain exact; a partial reference matching sibling spaces reaches the callers' existing refusal paths and changes no principal activity. | **CLOSED** |
| `FIND-TASK-003-3` | `RECORD_AUTHENTICATION_SQL` uses Postgres `GREATEST(last_authenticated_at, $2)`. Reverse-time commits cannot move activity backward, while schedule arming remains conditional on a null cursor. | **CLOSED** |
| `FIND-TASK-003-8` | Existing real boundaries cover API-key and workload-JWT activation and renewal, cached bearer use, stale-token re-exchange, inactivity, Card-free automation, delegation, human OIDC/refresh, observation no-touch, A/B versions, replicas, component inheritance, suspension, and deletion. The nonexistent TASK-004 SYSTEM path remains correctly excluded. | **CLOSED** |
| `FIND-TASK-003-12` | The final cumulative query removed `data_tenant_id = $1` and `.bind(conn.data_tenant_id())`; principal kind and CardRef are now `$1`/`$2`. Forced RLS on `wyrd.auth_service_accounts` is the sole tenant authority, while `LIMIT 2` plus exact-one handling preserves ambiguity refusal for every caller. | **CLOSED** |
| Other R1/R2 findings | `FIND-TASK-003-4/-5/-6/-7/-9/-10/-11/-13` were inspected at security-relevant seams. Their schedule, identity, workflow-owner, SDK/OpenAPI, documentation, and whitespace remediations introduce no security contradiction and do not reopen the findings above. | **NO SECURITY FINDING** |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No speculative hardening is proposed.

### Positive Controls

- CardRef principal selection has one shared, ambiguity-refusing owner across
  every production credential/delegation consumer; creation order is no longer
  an identity selector.
- Tenant identity comes from the verified credential boundary and
  transaction-local `TenantConn` state. Both relevant tables force RLS, and
  the final CardRef lookup duplicates no tenant authority in SQL.
- Activity mutation, token issuance, scope-mint audit, and exchange audit share
  the caller-owned transaction. Signing, SQL, audit, or commit failure refuses
  the token and rolls back the activity change.
- Registration evaluates the extra Operator permission at the sole end-user
  authorization boundary, and audit failure remains fail-closed.
- Admission re-reads current principal and Card lifecycle state, so a valid
  five-minute permission snapshot cannot admit new binding work for a
  suspended or deleted exact owner.
- Binding and owner identities are typed UUIDv7 values; the non-null reserved
  owner occurrence prevents owner/component identity collision.

## Verification evidence and limits

Independently run at candidate `a5a5b60f446981760ac831f64aba871582ec45e4`:

- exact SQL boundary test
  `queries::auth::service_accounts::tests::service_account_by_card_ref_uses_jsonb_card_ref_binding`: 1 passed;
- `mise run check:tenant-isolation`: passed;
- assembled-server exact-reference, Operator RBAC/audit, and activity-lifecycle
  tests: 3 passed;
- Postgres transactional tenant-isolation, monotonic activity, lifecycle/A-B,
  and excluded-principal tests: 4 passed;
- migration application/idempotence prerequisites run by both focused
  Postgres wrappers: passed; and
- `git diff --check 1609e102881dd55b12154248834a8aedbb7a36b2..a5a5b60f446981760ac831f64aba871582ec45e4`: passed.

The external Keycloak workload and human OIDC journeys were source-inspected
but not rerun because they require the repository-managed identity environment.
The immutable R1 implementation record reports that identity lane green, and
the R2 review independently inspected those paths. Broad SDK, codegen, format,
lint, and full-family lanes were not repeated for this security-only review;
the R2 remediation record reports their final relevant lanes green. These
limits do not hide an untested source-local security finding in the reviewed
boundary.

## Overall result

**PASS.** The cumulative candidate preserves the R1 closures and closes the R2
duplicate-tenant-authority finding without weakening exact-one principal
selection. No reachable material security, tenant-isolation, exact CardRef
resolution, RLS-authority, authentication-lifecycle, authorization, or
transactional-audit finding remains in TASK-003.
