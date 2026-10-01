# TASK-007 R2 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Prior verdict and remediation: `changes/active/verified-change-contract/review/TASK-007-r1/`
- Review attempt: `changes/active/verified-change-contract/review/TASK-007-r2/`

The candidate remained the stated commit through both review waves. Review
artifacts were written outside the immutable candidate and do not alter it.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-097–099: failed binding runs create independent durable dispatches; other outcomes and direct runs do not | Transactional fan-out, fenced claims, independent settlement, and provider journey evidence | PASS |
| REQ-138: bounded immutable failure context and closed template fields | Typed frozen context, registration validation, and schema/provider proof | PASS |
| REQ-139: latest exact tenant credential per attempt with selector-safe diagnostics | Per-attempt lookup and safe detail/logging pass, but the public problem remediation still publishes selector templates | FAIL (`FIND-TASK-007-1`) |
| REQ-147: forced-RLS encrypted storage, exact versioned KEKs, fail-start readiness, and bounded rewrap | Storage, envelope encryption, readiness, HTTPS Vault, and tenant rewrap bounds pass; oversized versions are silently changed and discovery is outside the pass bound | FAIL (`FIND-TASK-007-9`, `-10`) |
| REQ-148/150: typed management operations and closed provider contracts on every surface | HTTP/shared client/Rust/Python/TypeScript/CLI/MCP projections and static/runtime contract evidence | PASS |
| REQ-149: exact authority and screen/pin before credential attachment | Registration and attempt checks plus per-hop credential-order tests | PASS |
| REQ-140/141: Slack and PagerDuty provider contracts | Provider payload/response behavior passes, but provider wire mechanics violate the required provider-module boundary | FAIL (`FIND-TASK-007-12`) |
| REQ-142/146: bounded retry, deadline, concurrency, shutdown, restart, and availability | Delivery remains available beside rewrap and the recorded runtime journeys pass; whole-pass rewrap time remains unbounded at discovery | FAIL (`FIND-TASK-007-9`) |
| REQ-143: Workflow remains non-executable | Registration and worker rejection paths | PASS |
| REQ-145 and INV-007: RBAC, RLS, transactional audit, and security posture | Security-domain review and tenant-isolation proof pass; selector-rich derived `Debug` violates the required diagnostic boundary | FAIL (`FIND-TASK-007-11`) |
| REQ-152, INV-015, AC-033: PostgreSQL owns coordination time | SQL-owned availability, lease, retry, and deadline decisions | PASS |
| INV-006/011/013: no Card secrets; failed-only independent reaction; inline/reference parity | Contract, settlement, and fan-out evidence | PASS |
| AC-029/030/031: provider, authorization/runtime, and tenant-admin journeys | Recorded broad and focused lanes are green, but retained configuration, diagnostic, rewrap, and structural findings leave acceptance incomplete | FAIL (`FIND-TASK-007-1`, `-9`–`-12`) |
| Scope and non-goals | No new provider, cipher, broker, Alert, executable Workflow, secret-read API, checked-in OpenAPI, or compatibility path; unrelated skill changes are absent | PASS |

## Review-wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TREV-R2-001` |
| Repository standards | FAIL | `REPO-R2-1`–`REPO-R2-3` |
| Security/RBAC/tenancy domain | PASS | None |
| Delivery/concurrency/persistence domain | FAIL | `DR-DEL-R2-1` |
| Ponytail validation | Complete | Five deduplicated findings; four confirmed and one revised |

## Validated finding ledger

| Finding | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-007-1` | CONFIRMED | VIOLATION | Make the existing public key-unavailable remediation selector-free while preserving its stable metadata and retry classification. |
| `FIND-TASK-007-9` | REVISED | INCORRECT | Apply the existing whole-pass rewrap budget to cross-tenant discovery and return cleanly on expiry. |
| `FIND-TASK-007-10` | CONFIRMED | INCORRECT | Reject active key versions above `i32::MAX` at existing configuration validation and preserve accepted versions exactly. |
| `FIND-TASK-007-11` | CONFIRMED | VIOLATION | Replace selector-rich derived `Debug` on the three key configuration/owner types with the existing redacted pattern. |
| `FIND-TASK-007-12` | CONFIRMED | VIOLATION | Move only Slack and PagerDuty wire construction/interpretation into private provider modules, retaining common transport and policy owners. |

Full caller tracing, evidence, consequences, corrections, and closure proofs are
preserved in `findings-validation.md`. No retained correction requires a new
product, public API, architecture, security, compatibility, concurrency, or
persistent-data decision, so specification revision is not required.

## Prior-finding closure

| Prior finding | R2 status |
|---|---|
| `FIND-TASK-007-1` | PARTIALLY OPEN: detail and logs are safe; public remediation still exposes selector templates. |
| `FIND-TASK-007-2` | CLOSED: production verifies every active tenant key before readiness. |
| `FIND-TASK-007-3` | CLOSED: each effective URL is screened and pinned before credential attachment. |
| `FIND-TASK-007-4` | CLOSED: unrelated skill changes are absent from the cumulative diff. |
| `FIND-TASK-007-5` | CLOSED: production Vault requires HTTPS and token files use owner-only reads. |
| `FIND-TASK-007-6` | CLOSED: Python and TypeScript project precise closed provider contracts. |
| `FIND-TASK-007-7` | CLOSED: mounted-secret reads use the bounded blocking boundary. |
| `FIND-TASK-007-8` | CLOSED: changed production/library imports are module-scoped. |
| `FIND-TASK-007-9` | PARTIALLY OPEN: delivery independence, tenant bounds, and per-pass key reuse are closed; cross-tenant discovery remains outside the whole-pass bound. |

## Verification limits

- Every required reviewer completed within the 20-minute ceiling; no missing
  sub-reviewer gap was recorded.
- Reviewers independently ran focused security/key/HTTP unit tests and the
  repository's codegen, type, format, lint, client-tier, PyO3, unwrap, and
  tenant-isolation checks. The candidate records the broader SQL, server,
  CLI, MCP, SDK, and journey lanes as exit zero; those long lanes were not all
  rerun during this bounded review.
- No existing focused test stalls cross-tenant discovery or exercises the key
  version upper bound, generic remediation selectors, or redacted `Debug`;
  those proof gaps directly map to retained findings.
- Credentialed Slack/PagerDuty smoke remains gated release evidence and was
  not run.
- CodeGraph was unavailable because this checkout has no `.codegraph/` index;
  reviewers traced source and callers directly.

## Verdict

**FIX_REQUIRED**

The remediation task is
`changes/active/verified-change-contract/review/TASK-007-r2/TASK-007-R2-operator-delivery-corrections.md`.
