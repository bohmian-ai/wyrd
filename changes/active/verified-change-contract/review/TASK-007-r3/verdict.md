# TASK-007 R3 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `57ee8dd0b3dddc3e1da34bf543ecc566bae2e831`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Prior remediation:
  `changes/active/verified-change-contract/review/TASK-007-r1/TASK-007-R1-operator-delivery-corrections.md`
  and
  `changes/active/verified-change-contract/review/TASK-007-r2/TASK-007-R2-operator-delivery-corrections.md`
- Review attempt: `changes/active/verified-change-contract/review/TASK-007-r3/`

The candidate remained the stated commit through both review waves. Review
artifacts are outside the immutable candidate and do not alter reviewed source.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-097–099, INV-011: failed binding runs create independent durable dispatches; other outcomes and direct runs do not | Transactional settlement/fanout, shared claim loop, fenced settlement, SQL and provider journeys | PASS |
| REQ-138: one bounded immutable failure context with closed template fields | Typed frozen context, registration/render validation, delivery journey | PASS |
| REQ-139, REQ-149: latest exact tenant credential per attempt; exact authority; screen/pin before attachment | Tenant-scoped lookup/open, shared compatibility predicate, per-hop screened client before credential attachment | PASS |
| REQ-140: Slack request and JSON outcome contract | Private Slack provider module plus mock-provider and focused classification tests | PASS |
| REQ-141: PagerDuty stable dedup key without an exactly-once/grouping guarantee | Wire behavior is correct, but provider rustdoc promises every retry collapses to one incident | FAIL (`FIND-TASK-007-13`) |
| REQ-142, REQ-146: retry/deadline/concurrency/shutdown/restart bounds | Database-clock dispatch lifecycle, shared limits, supervised worker, slow endpoint/fairness/shutdown evidence | PASS |
| REQ-143: Workflow remains non-executable | Registration refusal and worker terminal refusal | PASS |
| REQ-145, INV-007: RBAC, RLS, and transactional audit | Tenant connections, typed permissions, canonical audit composition, security-domain review | PASS |
| REQ-147: encrypted durable connections, external exact-version keys, readiness, and bounded rewrap | API-bearing validation, ciphertext/AAD, boot readiness, bounded discovery/tenant rewrap pass all pass; non-API boot and direct construction can still panic on an oversized version | FAIL (`FIND-TASK-007-10`) |
| REQ-148, REQ-150: closed typed management contract across HTTP/client/SDK/CLI/MCP | Shared contracts/client and Rust, Python, TypeScript, CLI, MCP projections with journeys and generated artifacts | PASS |
| REQ-152, INV-015, AC-033: PostgreSQL coordination clock | `statement_timestamp()`-owned claim, deadline, retry, and fencing evidence | PASS |
| AC-029–031: provider, authorization/runtime, and tenant-admin journeys | Focused and recorded broad evidence is green; retained findings leave final acceptance incomplete | FAIL (`FIND-TASK-007-10`, `-13`) |
| Repository documentation rule | Four candidate-added trait methods lack mandatory rustdoc | FAIL (`FIND-TASK-007-14`) |
| Repository import-manifest rule | Candidate-added declarations use fully qualified standard-library types instead of module imports and bare names | FAIL (`FIND-TASK-007-15`) |
| Scope and non-goals | No new provider, cipher, dependency, broker, Alert, executable Workflow, checked-in OpenAPI, or exactly-once implementation; only the PagerDuty documentation overclaims grouping | FAIL (`FIND-TASK-007-13`) |

## Review-wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TREV-R3-001`, `TREV-R3-002` |
| Repository standards | FAIL | `STD-R3-001`–`STD-R3-003` |
| Security/RBAC/tenancy domain | PASS | None |
| Delivery/concurrency/persistence domain | PASS | None |
| Ponytail validation | Complete | Four deduplicated findings; two confirmed and two revised |

## Validated finding ledger

| Finding | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-007-10` | REVISED | INCORRECT | Leave non-API roles on the unused default key owner and make `OperatorKeys::new` return its existing typed error instead of panicking for an unvalidated oversized version. |
| `FIND-TASK-007-13` | CONFIRMED | VIOLATION | Correct only the PagerDuty rustdoc so stable dedup is described as possible grouping, never a guarantee. |
| `FIND-TASK-007-14` | REVISED | VIOLATION | Add the required invariant-focused rustdoc to the four independently confirmed trait methods. |
| `FIND-TASK-007-15` | CONFIRMED | VIOLATION | Import the cited standard-library types at module top and use bare names in the candidate-added declarations. |

Full reachability, evidence, dispositions, consequences, and decision-complete
corrections are preserved in `findings-validation.md`. None requires a new
product, public API, architecture, security, compatibility, concurrency, or
persistent-data decision.

## Prior-finding closure

| Prior finding | R3 status |
|---|---|
| `FIND-TASK-007-1`–`FIND-TASK-007-9` | CLOSED |
| `FIND-TASK-007-10` | OPEN / REVISED: API-bearing validation is exact, but non-API boot and direct construction can still unwind. |
| `FIND-TASK-007-11`–`FIND-TASK-007-12` | CLOSED |

## Verification limits

- All required reviewers completed within the 20-minute per-reviewer limit;
  no sub-reviewer evidence gap was recorded.
- Reviewers freshly ran focused key/redaction/provider tests, tenant-isolation,
  Postgres rewrap/discovery/shutdown/fencing/deadline tests, migrations, and
  `git diff --check`; those checks passed.
- The candidate records the broader SQL/shared/server/CLI/MCP, Rust/Python/
  TypeScript journey, typecheck, codegen, boundary, format, and lint lanes as
  green. The review did not rerun every broad lane.
- No current executable check covers the non-API oversized-version boot path or
  enforces trait-method rustdoc/import source shape.
- Credentialed Slack/PagerDuty smoke remains gated release evidence and was not
  run. It is not relevant to the retained documentation overclaim.
- CodeGraph was unavailable because this checkout has no `.codegraph/` index;
  reviewers traced source and callers directly.

## Verdict

**FIX_REQUIRED**

The remediation task is
`changes/active/verified-change-contract/review/TASK-007-r3/TASK-007-R3-operator-delivery-corrections.md`.
