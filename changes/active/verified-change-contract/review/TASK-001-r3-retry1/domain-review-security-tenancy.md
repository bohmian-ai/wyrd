# TASK-001 r3 retry 1 — Domain review: security and tenancy

Reviewer: fresh `domain-rev` (security and tenancy). Immutable subject: base
`5293546f33b3a5fd9de529098e23ea70d472c412`, cumulative candidate
`9d7b6266206f15136f306b66c06571066fc6bd13`.

## Reviewed boundary

I traced the TASK-001 registration trust boundary and the round-2 security
remediation through:

- authenticated `POST /v1/cards`, its route-local `card:write` decision,
  stable denial mapping, and audit handoff;
- request decoding, `Spec::binding_sites`, `spec_binding_errors`, and the
  refusal of `verified_by` carried by a Workflow's inline Agent;
- duplicate Verifier and Operator validation, including caller-supplied
  optional UIDs and repeated inline Operator bodies;
- tenant-scoped resolution and effective-spec loading through `TenantConn`,
  cross-tenant non-disclosure, UID pinning, and relationship/no-write seams;
- the Postgres-backed refusal test's writer and denied principals, response
  codes, and durable-state assertions.

Primary source coverage included
`crates/wyrd-spec/src/graph/composition.rs`,
`crates/wyrd-spec/src/reference.rs`,
`crates/wyrd/wyrd-server/src/components/cards/{routes,service,resolve}.rs`,
`crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs`, the cumulative
diff `5293546f3..9d7b62662`, the approved specification and TASK-001, both prior
review ledgers and verdicts, and the TASK-001-R2 remediation and evidence.

## Authority and obligation coverage

| Boundary | Governing authority | Result |
|---|---|---|
| Authentication, authorization, stable denial, and audit | `AGENTS.md` §§2, 9, 11; `architecture/wyrd-security-posture.md`; error and agent-harness references | **PASS.** Authentication materializes `Caller`; the route checks `card:write` before registration. The denied case now asserts `WYRD_PERMISSION_403_DENIED_RBAC` and no registration writes. |
| Tenant isolation and cross-tenant non-disclosure | Spec REQ-092, INV-007, AC-018; security posture tenant/data isolation | **PASS.** Binding refs resolve through the caller tenant's `TenantConn`; a foreign-tenant Verifier remains unresolved and produces no Card, operation, or relationship write. |
| Legal binding locations and fail-closed nested input | Spec REQ-090/092, INV-001; `FIND-TASK-001-2` | **PASS.** `Spec::binding_sites` remains the shared owner; nested inline-Agent bindings are rejected by `validate_request` before reference resolution or the write transaction. The authenticated server case proves HTTP 400, `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, and no registration writes. |
| Canonical duplicate identity | Spec REQ-102 and AC-018; `FIND-TASK-001-4` | **PASS.** Referenced Verifiers and Operators use `CardRef::identity_key()`, whose tuple excludes optional UID exactly as resolution and authorization do. Equal inline `OperatorSpec` bodies are rejected by typed equality. |
| Preserved effective-body controls | Spec REQ-093/094/143 and AC-004/018 | **PASS.** Wrong kinds and duplicate entries remain pure pre-write refusals; effective Trigger/Verifier pairing and referenced Workflow Operator refusal remain tenant-scoped server checks. |

## Prior-finding closure

| Finding | Source closure | Proof closure | Result |
|---|---|---|---|
| `FIND-TASK-001-2` | The shared production owner still enumerates and rejects a non-empty nested inline-Agent binding before resolution and persistence. | The fifth scenario in `referenced_binding_refusals_leave_no_writes` uses the authenticated writer route, asserts the exact 400 code, and reuses `assert_no_registration_writes`. Combining it with the four registry-dependent refusals does not weaken the proof: it runs through the same real server and fixture, has a unique idempotency key/Card name, and independently asserts its response and durable absence. | **CLOSED** |
| `FIND-TASK-001-4` | UID-sensitive display keys were replaced with the existing canonical identity; referenced and inline Operator duplicates are both covered; the RBAC denial's stable code is asserted. | The committed evidence records expected RED for all three newly reachable duplicate cases, GREEN for eight binding-validation tests, 664 shared tests, and 24 card-registration integration tests. | **CLOSED** |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None.

### Positive Controls

- Tenant identity comes from the verified `Caller`, never the Card payload or
  a presented CardRef UID.
- Referenced binding targets are resolved under tenant RLS and caller-presented
  UIDs are not trusted as identity or authorization selectors.
- Duplicate checks use the same exact named identity as registry resolution;
  optional UID presentation cannot bypass one-Verifier/one-Operator rules.
- Invalid nested bindings are rejected before external resolution and before
  the write transaction; stable problem codes are asserted at the HTTP edge.
- The no-write proof checks registration operations, Cards, and relationships;
  authorization audit remains a separate required security write rather than
  being mistaken for registration mutation.
- No new permission, credential path, secret field, dependency, or alternate
  reference walker was introduced by the remediation.

## Verification evidence and limits

The candidate records the focused server test passing through the
repository-managed Postgres wrapper, the eight focused binding-validation tests
passing after expected RED, and the broader `test:shared`,
`test:cards:integration`, `test:wyrd`, lint, format, and boundary lanes green.
I independently inspected the complete relevant function bodies, callers, and
cumulative diff. I did not rerun Cargo-backed commands because this shared
checkout had other active review processes and repository authority requires
Cargo-backed work to run sequentially. This does not create a security proof
gap: the exact focused and aggregate results are committed in the immutable
candidate, and the reviewed source matches those recorded commands.

The review is limited to TASK-001's registration, authorization, and tenancy
boundary. Later runtime activation, execution, Operator delivery, and
connection-secret behavior belong to later tasks and were not treated as
implemented here.

## Overall result

**PASS.** `FIND-TASK-001-2` and `FIND-TASK-001-4` are closed. No material
security, authorization, tenant-isolation, stable-error, or registration
atomicity finding remains in the reviewed TASK-001 candidate.
