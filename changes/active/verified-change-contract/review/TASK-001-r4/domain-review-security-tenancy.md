# TASK-001 r4 — Domain review: security and tenancy

Reviewer: fresh `domain-rev` for security and tenancy. Immutable subject: base
`5293546f33b3a5fd9de529098e23ea70d472c412`, cumulative candidate
`c8bb490ad814c0c7770cac33ed7779897ff776e4`.

## Reviewed boundary

I traced the complete TASK-001 registration trust boundary through:

- authenticated `POST /v1/cards`, its route-local `card:write` decision,
  stable denial mapping, and transactional audit handoff;
- request decoding, the canonical `Spec::binding_sites` inventory,
  `spec_binding_errors`, and refusal of `verified_by` carried by an inline
  Agent nested under a Workflow or Eval-backed Verifier;
- duplicate Verifier and Operator detection, including caller-supplied
  optional UIDs and repeated inline Operator bodies;
- tenant-scoped resolution, effective-spec loading through `TenantConn`,
  cross-tenant non-disclosure, UID pinning, and relationship/no-write seams;
- the authenticated Postgres-backed refusal test and its exact pinned rerun
  command; and
- the final remediation range `9d7b62662..c8bb490ad`, which changes only
  vocabulary/count documentation and review evidence for this boundary.

Primary source coverage included `AGENTS.md`,
`architecture/agent-rules.md`,
`architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`,
`architecture/wyrd-security-posture.md`,
`architecture/references/languages/spec-driven-development.md`, the approved
specification and original TASK-001, all prior TASK-001 security findings and
remediation packets, the cumulative diff `5293546f3..c8bb490ad`, and:

- `crates/wyrd-spec/src/graph/composition.rs`
- `crates/wyrd-spec/src/reference.rs`
- `crates/wyrd/wyrd-server/src/components/cards/{routes,service,resolve}.rs`
- `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs`

## Authority and obligation coverage

| Boundary | Governing authority | Result |
|---|---|---|
| Authentication, authorization, stable denial, and audit | `AGENTS.md` §§2, 9, 11; security posture authorization and production-composition rules | **PASS.** The authenticated route materializes `Caller`, checks `Permission::card_write()` before registration, returns `WYRD_PERMISSION_403_DENIED_RBAC` for the denied principal, and keeps the allowed audit event tied to the registration transaction or records it standalone on refusal. |
| Tenant isolation and cross-tenant non-disclosure | Spec REQ-092, INV-007, AC-018; security posture tenant and RLS rules | **PASS.** The verified caller supplies `data_tenant_id`; external binding refs resolve through that tenant's `TenantConn`, so a foreign-tenant Verifier is indistinguishable from an unresolved dependency and produces no registration writes. |
| Legal binding locations and fail-closed nested input | Spec REQ-090/092, INV-001; `FIND-TASK-001-2` | **PASS.** `Spec::binding_sites` remains the shared owner. `validate_request` invokes `spec_binding_errors` before graph planning, external resolution, or the write transaction, and rejects non-empty nested inline-Agent bindings with `WYRD_REGISTRY_400_INVALID_CARD_SPEC`. |
| Canonical duplicate identity | Spec REQ-102 and AC-018; `FIND-TASK-001-4` | **PASS.** Referenced Verifiers and Operators use `CardRef::identity_key()`, which excludes the optional server-managed UID exactly as resolution does; repeated inline Operators are rejected by typed `OperatorSpec` equality. |
| Effective referenced-body controls | Spec REQ-093/094/143 and AC-004/018 | **PASS.** Referenced Trigger/Operator bodies are loaded only after tenant-scoped resolution; incompatible activation and Workflow Operator actions fail before persistence. |
| Final remediation preservation | TASK-001-R3 preserved behavior; cumulative candidate | **PASS.** `9d7b62662..c8bb490ad` changes no registration, authn/authz, tenancy, secret, schema, dependency, or persistence behavior. The wording changes neither expose credentials nor alter stable errors or permission resources. |

## Prior-finding closure

| Finding | Closure | Result |
|---|---|---|
| `FIND-TASK-001-2` | The authenticated real-server scenario submits a Workflow with an inline Agent carrying `verified_by`, asserts HTTP 400 and `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, then checks zero registration-operation, Card, and relationship writes. | **CLOSED** |
| `FIND-TASK-001-4` | Canonical identity ignores presented UID for duplicate Verifiers/Operators, equal inline Operators are refused, and the under-privileged route case asserts `WYRD_PERMISSION_403_DENIED_RBAC` plus no registration writes. | **CLOSED** |
| `FIND-TASK-001-17` (security-relevant registration proof) | The recorded command uses the repository-managed Postgres wrapper and exact `mise exec -- cargo nextest run --locked` selector with `--retries 0`. I independently confirmed the selector names exactly one test and reran the recorded command successfully: 1 passed, 23 skipped, no retry. | **CLOSED** |

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

- Tenant identity comes from the verified `Caller`, never a Card payload,
  reference UID, header, path, or submitted object name.
- Binding targets resolve under tenant RLS, and caller-presented UIDs neither
  select identity nor widen authorization.
- Invalid nested bindings are rejected before external reads or durable
  registration mutation, with a stable public code asserted at the HTTP edge.
- Duplicate detection uses the registry's existing named identity instead of
  display strings or caller-managed UID presentation.
- Cross-tenant, effective-body, and RBAC refusals all reuse the same no-write
  proof for operations, Cards, and relationships; required authorization audit
  rows remain separate security evidence rather than being misclassified as
  registration mutation.
- TASK-001 adds no permission, credential path, secret-bearing field,
  alternate reference walker, or dependency for this boundary.

## Verification evidence and limits

Executed during this review:

- `mise exec -- cargo nextest list --locked -p wyrd-server --test
  pg_card_registration_route -E
  'test(=referenced_binding_refusals_leave_no_writes)'` selected exactly
  `wyrd-server::pg_card_registration_route
  referenced_binding_refusals_leave_no_writes`.
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run
  db:migrate:all:inner && WYRD_REG_E2E=1 mise exec -- cargo nextest run
  --locked -p wyrd-server --test pg_card_registration_route -E
  "test(=referenced_binding_refusals_leave_no_writes)" --retries 0'` passed:
  1 test passed, 23 skipped, with no retry.
- `git diff --check 5293546f3..c8bb490ad` passed.

The broader format, lint, codegen, docs, shared, cards integration, Wyrd, and
Bifrost lanes are recorded green in the immutable candidate and were not
repeated for this domain review. Later Verifier execution, result writing,
Operator delivery, and connection-secret behavior belong to TASK-002 through
TASK-008 and were not treated as implemented here.

## Overall result

**PASS.** The final remediation preserves the closed registration security and
tenancy boundary, and the pinned authenticated refusal proof is valid and
independently green. No material security, authorization, tenant-isolation,
stable-error, or registration-atomicity finding remains.
