# TASK-001 r2 — Domain review: security and tenancy

Reviewer: `domain-rev` (independent). Immutable subject: base `5293546f3`,
cumulative candidate `dd0503e7149017d760a987346e2838001e5c3429`. Repository
HEAD is `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`; that later
`AGENTS.md`-only process commit is outside the reviewed source range.

## Reviewed boundary

I traced the TASK-001 registration trust boundary end to end:

- authenticated `POST /v1/cards` and its route-local `card:write` decision and
  audit handoff;
- request decoding and strict Verifier/Trigger/binding validation;
- the shared enumeration of every legal and nested `verified_by` location;
- external and sibling Verifier, Trigger, and Operator reference collection,
  tenant-scoped resolution, effective-spec loading, activation/action checks,
  UID pinning, relationship derivation, and the in-transaction active-reference
  recheck;
- rejection atomicity for wrong-kind, nested, unsupported-action,
  cross-tenant, and under-privileged requests;
- loader refusal of retired Card kinds;
- the remediation-only change to the Bifrost ingest authentication smoke test,
  including the ancestor `tenant_admits_credentials` behavior.

Primary source coverage included
`crates/wyrd-spec/src/{graph/composition.rs,refs/mod.rs,card/trigger.rs,card/verifier.rs}`,
`crates/shared/wyrd-loader/src/{parse.rs,validate.rs}`,
`crates/wyrd/wyrd-server/src/components/cards/{routes.rs,service.rs,resolve.rs}`,
`crates/wyrd/wyrd-server/tests/{pg_card_registration_route.rs,pg_grpc_ingest_smoke.rs}`,
and the complete cumulative diff `5293546f3..dd0503e71` for those surfaces.

## Authority and obligation coverage

| Boundary | Governing authority | Result |
|---|---|---|
| Authentication, route permission, and audit | `AGENTS.md` §§2, 9; `architecture/wyrd-security-posture.md` Security principles / Authorization and policy / Audit integrity | PASS: verified `Caller` supplies tenancy; `card:write` is checked before registration; allowed and denied decisions retain the existing canonical audit path. |
| Tenant isolation for binding refs | Spec REQ-092 and AC-018; security posture Tenant and data isolation; architecture constraints Identity and Tenant Isolation | PASS: resolution and effective-spec reads use the caller tenant's `TenantConn`; the real-server cross-tenant case returns `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` and persists no registration state. |
| Binding location and fail-closed validation | Spec REQ-090/092/094/143, INV-001; prior `FIND-TASK-001-2` | FAIL: the source correction is present, but its required real-server closure proof is missing (DS-R2-1). |
| Unknown and secret-shaped input refusal | Spec AC-004, REQ-046/093, INV-006; prior `FIND-TASK-001-3` | PASS: fieldless struct activation plus enum `deny_unknown_fields` rejects unknown Trigger keys, and `VerifierImplementation` rejects sibling keys such as `api_token`. |
| Server-only binding refusals | Spec AC-004/018; prior `FIND-TASK-001-4` | PASS: referenced workflow Operator, activation mismatch, cross-tenant ref, and under-privileged caller are exercised through the authenticated server and assert no writes. |
| Credential-admission smoke proof | Security posture Principal and credential lifecycle | PASS for the changed test: its minted tenant is now seeded before token verification; the only other `mint_user_jwt` call in that test target already follows the same ordering. The production admission gate predates the task and was not changed. |

## Prior security-finding closure

| Prior finding | Source closure | Required proof closure | Result |
|---|---|---|---|
| `FIND-TASK-001-2` / r1 DS-2 | `Spec::binding_sites` is now the single owner; `spec_binding_errors` refuses non-empty bindings in inline Workflow and Eval-judge Agents; loader and server request validation consume it. | Two focused pure tests pass, but the required authenticated real-server refusal asserting the stable code and no writes does not exist. | FAIL (DS-R2-1) |
| `FIND-TASK-001-3` / r1 DS-3 | Strict Trigger and Verifier implementation decoding restored. | Focused unknown-field and secret-shaped sibling-key tests pass. | PASS |
| `FIND-TASK-001-4` / r1 DS-1 | Effective binding bodies resolve under `TenantConn`; unsupported action and activation pairing fail before write. | `referenced_binding_refusals_leave_no_writes` passes and covers cross-tenant and RBAC denial as well as the two effective-body refusals. | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

- **DS-R2-1 — MISSING — prior fail-open correction lacks its required real-server closure proof.**
  **Violated obligation:** the independently validated closure proof for
  `FIND-TASK-001-2` in
  `changes/active/verified-change-contract/review/TASK-001-r1/findings-validation.md:48`,
  together with REQ-090/092 and the TASK-001 remediation acceptance requirement
  that an illegal nested binding be refused before any write.
  **Location:** `crates/wyrd-spec/src/graph/composition.rs:238-358` contains the
  pure correction and unit tests; `crates/wyrd/wyrd-server/tests/pg_card_registration_route.rs`
  contains no Workflow-inline-Agent or Eval-judge-inline-Agent registration
  refusal. Repository search finds the nested-binding scenarios only in the
  `wyrd-spec` unit tests.
  **Evidence:** both pure tests
  `spec_binding_errors_refuse_a_workflow_step_inline_agent_binding` and
  `spec_binding_errors_refuse_an_eval_judge_inline_agent_binding` pass, and
  `service.rs:1154-1192` visibly calls the shared validator. However, round 1's
  validated ledger explicitly required a real-server case that asserts the
  stable `WYRD_REGISTRY_400_INVALID_CARD_SPEC` response and no writes. The
  round-2 implementation evidence lists only the pure tests for AC-R2.
  **Observable consequence:** this is not evidence of a currently exploitable
  source defect; the inspected call path appears fail closed. It is a material
  acceptance gap at the original trust boundary: an integration regression
  between HTTP decoding, `validate_request`, error mapping, and registration
  atomicity could re-open the prior nested-binding path while all supplied
  closure tests remain green.
  **Testable correction:** add one case to the existing
  `pg_card_registration_route` suite that submits either a Workflow inline
  Agent or an Eval inline judge Agent with non-empty `verified_by`, authenticates
  as the existing writer fixture, asserts HTTP 400 with
  `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, and reuses
  `assert_no_registration_writes`. Keep the two pure tests for coverage of both
  nested forms; no new harness, error code, or production change is needed.

### Low / Defense In Depth

None.

### Positive Controls

- The request's verified principal supplies `data_tenant_id`; binding targets
  are resolved and loaded only through a tenant RLS connection.
- External UIDs are server-derived, overwritten during binding, and rechecked
  under the registration transaction before mutation.
- Unsupported referenced Workflow Operators, mismatched Trigger activation,
  cross-tenant references, and missing `card:write` fail without Card,
  operation, or relationship writes.
- Unknown Trigger and Verifier-union keys fail decoding rather than being
  silently discarded, including a credential-shaped `api_token` key.
- Verification control-plane refs do not create a new credential or permission
  path, and registration alone starts no verifier work.

## Verification evidence and limits

Executed during this review, all passing:

- `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E
  'test(=graph::composition::tests::spec_binding_errors_refuse_a_workflow_step_inline_agent_binding)
  | test(=graph::composition::tests::spec_binding_errors_refuse_an_eval_judge_inline_agent_binding)'`
  — 2 passed.
- Focused Trigger unknown-field checks — 3 passed.
- `card::verifier_card_tests::verifier_implementation_rejects_sibling_keys`
  — 1 passed.
- Repository-managed Postgres execution of
  `referenced_binding_refusals_leave_no_writes` — 1 passed.
- Repository-managed Postgres execution of
  `ingest_valid_token_is_not_rejected_as_unauthenticated` — 1 passed.

The remediation record reports the broader required lanes green, including
`test:cards:integration`, `test:bifrost`, and `test:wyrd`; this reviewer did not
rerun those aggregates. The reported allocator pool-identity flake is outside
this security/tenancy domain. The unseeded-tenant concern was checked in the
changed ingest target: its other minted-token case already seeds the tenant;
no broader pre-existing test audit is required by TASK-001.

## Overall result

**FAIL.** No exploitable security or tenant-isolation defect remains in the
inspected candidate source, but DS-R2-1 leaves the mandatory real-server closure
proof for the prior fail-open finding incomplete.
