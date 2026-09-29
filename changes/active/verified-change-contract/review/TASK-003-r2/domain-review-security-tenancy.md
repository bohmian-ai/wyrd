# TASK-003 r2 — Domain review: security, tenancy, and authorization

Reviewer: fresh Wave 1 `domain-rev`.

Immutable subject:

- base: `1609e102881dd55b12154248834a8aedbb7a36b2`
- candidate: `449aceb346f5f5fbc27958d260bd9c0c50225466`
- approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- original task: `changes/active/verified-change-contract/tasks/TASK-003-binding-projection-and-runtime-activity.md`
- remediation task: `changes/active/verified-change-contract/review/TASK-003-r1/TASK-003-R1-close-binding-activity-contract.md`

The candidate remained at the pinned commit before and after this review.

## Reviewed boundary

I reviewed only the cumulative TASK-003 security boundary:

- registration RBAC for Operator-bearing bindings and the canonical
  transactional audit path for allowed and denied decisions;
- exact Card-bound principal selection across API-key issuance, workload
  `jwt-bearer`, and CardRef-targeted delegation;
- qualifying and excluded activity grants, monotonic authentication activity,
  schedule arming, current lifecycle admission, A/B versions, and shared
  principal replicas;
- binding identity and owner-occurrence identity at the Card, SQL, and status
  seams; and
- tenant isolation, forced RLS, transaction ownership, and cross-tenant
  visibility for the new control state.

The TASK-004 scheduler/SYSTEM mint path, verifier execution, result writing,
and Operator delivery do not exist in this candidate and were not treated as
TASK-003 findings. The review did inspect the frozen authorization and binding
state those later paths will consume.

## Authority and source coverage

| Boundary | Authority | Source and reachable callers inspected | Result |
|---|---|---|---|
| Operator registration authorization and audit | `architecture/agent-rules.md` transactional audit rules; security posture authorization/audit; REQ-145, INV-007, AC-018/030; remediation `FIND-TASK-003-1` | `cards/routes.rs:394-433`; `cards/service.rs:528-611,1019-1107`; `audit/mod.rs:183-248`; registration route proofs | PASS |
| Exact Card-bound principal selection | Security posture principal/credential lifecycle; REQ-105/106/112; remediation `FIND-TASK-003-2` | `queries/auth/service_accounts.rs:11-34,170-201`; callers in `issue_api_key.rs:86-137`, `jwt_bearer.rs:51-91,141-159`, and `exchange_api_key.rs:271-314,350-365`; test-harness caller inspected separately | PASS |
| Activity grant and lifecycle | Security posture access-token lifecycle; REQ-105–108/112; AC-019/020; remediation `FIND-TASK-003-3/-8` | `issuance.rs:120-158,274-398`; API-key and workload callers; `verification.rs:56-109,319-485`; SQL, Card-route, identity, and observation journeys | PASS |
| Exact binding identity | Wyrd design CardRef/binding doctrine; REQ-104; remediation `FIND-TASK-003-5/-6` | migration 27; `card/verifier.rs:186-210`; `ids.rs:220-297`; `graph/composition.rs:17-114`; `verification.rs:231-317,404-492`; registration projector | PASS |
| Tenant isolation and transaction coupling | Agent rules `TenantConn`/RLS rules; security posture tenant/data isolation; REQ-078/112; INV-007 | migration 27; all production callers of `project_bindings`, `owner_binding_ids`, `record_machine_authentication`, and `binding_activity`; registration and issuance commit owners | PASS |
| Dependency change | Repository supply-chain rule | workspace and `wyrd-sql` manifests plus lockfile entries for pinned `chrono-tz` and `croner 4.0.0`; use is confined to schedule parsing in `wyrd-sql` | PASS; no source-local security issue |

The review also read `AGENTS.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, the
applicable security/RBAC and spec-driven-development references, the complete
base-to-candidate diff, the prior R1 verdict and validation ledger, and the
surrounding migration, auth, registry, audit, SQL, and test owners. `.codegraph/`
is absent, so source and caller tracing used repository search and direct file
inspection.

## Prior-finding closure

| Prior finding | Security-domain reassessment | Result |
|---|---|---|
| `FIND-TASK-003-1` | `register_card_http` evaluates `cards:write`, detects any effective non-empty top-level `on_failure`, and evaluates `operators:invoke` exactly once. Allowed rows commit with registration or are recorded standalone when no registration commits; a denial records both already-evaluated decisions and leaves no registration writes. The focused route test proves Operator-free, inline, referenced, denied, allowed, rollback, and exact-cardinality behavior. | CLOSED |
| `FIND-TASK-003-2` | The one shared principal lookup fetches at most two active matches and returns a row only for exactly one. API-key issuance, workload `jwt-bearer`, and CardRef delegation all retain this shared owner. Explicit-space references select their own principal; ambiguous partial references reach existing not-found refusals and create no credential, token, delegation, or activity. | CLOSED |
| `FIND-TASK-003-3` | The activity update uses Postgres `GREATEST(last_authenticated_at, $2)` in the issuing transaction. Reverse timestamp order cannot reduce the stored value, and null-only cursor arming prevents renewal from moving an admitted schedule. | CLOSED |
| `FIND-TASK-003-8` | The assembled Card/auth journey now covers real API-key exchange, cached bearer use, stale-token request-driven re-exchange, idle expiry, Card-free automation, delegation, A/B versions, replicas, suspension, deletion, component inheritance, and cursor stability. The real Keycloak journey covers exact-space workload `jwt-bearer` activation and renewal; the human OIDC/refresh/delegation journey and Rust observation journey prove representative excluded boundaries. The nonexistent SYSTEM path remains correctly excluded. | CLOSED |
| `FIND-TASK-003-4` | Registration now parses the effective Trigger and requires a future occurrence before writes, preventing permanently inert accepted schedules. | CLOSED |
| `FIND-TASK-003-5` | `$owner` is a non-null persisted occurrence key; composition rejects that value as a component alias and the migration enforces the Agent owner domain. | CLOSED |
| `FIND-TASK-003-6` | Activity/query boundaries use `PrincipalId`, `BindingId`, and `CardUid`; invalid stored UUID versions fail decoding. | CLOSED |
| `FIND-TASK-003-7` | The transaction-scoped `BindingProjector` owns freeze/projection without changing transaction or tenant authority. No independent security issue remains. | CLOSED for this domain |
| `FIND-TASK-003-9/-10/-11` | SDK/OpenAPI proof and Rust documentation are outside this domain's acceptance judgment. Inspection found no security contradiction introduced by their remediation. | NO SECURITY FINDING |

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

- Operator-bearing registration spends the additional typed permission at the
  sole end-user authorization boundary, and canonical audit failure remains
  fail-closed.
- All three CardRef-to-principal consumers share one ambiguity-refusing lookup;
  no caller retains the former oldest-row fallback.
- Tenant identity continues to come from verified credentials and
  transaction-local `TenantConn` state. The new binding table enables and
  forces RLS with the canonical tenant policy; no production query accepts a
  raw pool or commits a caller-owned transaction.
- Activity is updated only by the closed API-key/`jwt-bearer` grant set and
  only for an active Card-bound Service or Agent. Delegation, OIDC login,
  refresh, cached requests, Card-free automation, and observation writes do
  not touch it.
- Activity renewal, schedule arming, token issuance, scope-mint audit, and
  exchange audit remain one transaction. Any signing, SQL, or audit failure
  rolls activity back and refuses the token.
- Admission re-reads current principal and Card lifecycle state, so a valid
  five-minute permission snapshot cannot keep a suspended/deleted owner
  eligible for new binding work.
- Binding and owner identities are typed UUIDv7 values, and the total
  occurrence-key domain prevents owner/component identity collision.

## Verification evidence and limits

Independently run at candidate `449aceb346f5f5fbc27958d260bd9c0c50225466`:

- exact `wyrd-sql` CardRef lookup unit test: 1 passed;
- exact Postgres tests for reserved owner occurrence, invalid stored identity,
  monotonic activity, and transactional tenant isolation: 4 passed;
- exact assembled-server tests for Operator RBAC/audit, ambiguous CardRef
  refusal, and qualifying/excluded activity lifecycle: 3 passed;
- `mise run check:tenant-isolation`: passed; and
- base-to-candidate whitespace check was inspected. Its only failures are
  trailing spaces in preserved R1 review prose, not source or security state.

The external Keycloak workload and human OIDC journeys were source-inspected
but not rerun in this review because they require the repository-managed
identity environment. The immutable task record reports `test:identity:journey`
green (21/21), including both journeys. SDK status and OpenAPI tests are
supporting contract evidence, not substitutes for the security proofs above.

## Overall result

**PASS.** The cumulative candidate closes the prior RBAC, principal-confusion,
activity-concurrency, and real-boundary proof gaps. No reachable material
security, authorization, transactional-audit, exact-identity, auth-lifecycle,
or tenant-isolation finding remains in TASK-003.
