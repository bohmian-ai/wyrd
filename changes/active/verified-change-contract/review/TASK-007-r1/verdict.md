# TASK-007 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `2bd4ded8f212f7885b0dd78bcd450fa0ff118f3e`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review attempt: `changes/active/verified-change-contract/review/TASK-007-r1/`

The candidate remained the stated commit through both review waves. Review
artifacts were written outside the immutable candidate and do not alter it.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-097–099: failed binding runs create independent durable dispatches; other outcomes and direct runs do not | Transactional completion/dispatch SQL, separate leased worker, fan-out and status journeys | PASS |
| REQ-138: one bounded immutable failure context with closed template fields | Typed context, bounded summary, SQL freeze, contract and delivery tests | PASS |
| REQ-139: latest exact tenant credential per attempt with safe diagnostics | Per-attempt connection lookup and rotation journey; key errors expose selectors | FAIL (`FIND-TASK-007-1`) |
| REQ-147: forced-RLS encrypted connection storage, external versioned KEKs, bounded rotation | Ciphertext-only/RLS/rotation proof exists; production key readiness, Vault boundary, and rewrap availability fail | FAIL (`FIND-TASK-007-2`, `-5`, `-7`, `-9`) |
| REQ-148/150: typed management operations and closed provider contracts on every surface | HTTP/shared client/Rust SDK/CLI/MCP behavior exists; Python and TypeScript public types do not faithfully project the closed contract | FAIL (`FIND-TASK-007-6`) |
| REQ-149: exact connection authority and SSRF screen/pin before credential attachment | Authority checks and pinned transport exist; auth headers are materialized before screening | FAIL (`FIND-TASK-007-3`) |
| REQ-140/141: Slack and PagerDuty provider contracts | Local mock-provider payload and response handling journeys; live smoke remains gated | PASS within non-credentialed review evidence |
| REQ-142/146: bounded retry, deadline, concurrency, shutdown, restart, and availability | Recorded timeout/retry/fairness/restart tests pass; inline rewrap can delay the sole delivery loop beyond dispatch deadlines | FAIL (`FIND-TASK-007-9`) |
| REQ-143: Workflow remains non-executable | Registration rejects Workflow delivery and worker cannot report it delivered | PASS |
| REQ-145 and INV-007: RBAC, RLS, transactional audit, and security posture | Route/audit/RLS journeys pass; plaintext Vault transport and loose token-file permissions violate the security boundary | FAIL (`FIND-TASK-007-5`) |
| REQ-152, INV-015, AC-033: PostgreSQL owns coordination time | SQL uses `statement_timestamp()` and tests control database state | PASS |
| INV-006/011/013: no Card secrets; failed-only independent reaction; inline/reference parity | Contract, settlement, and fan-out evidence | PASS |
| AC-029/030/031: provider, authorization/runtime, and tenant-admin journeys | Broad recorded lanes pass, but retained security, typing, and rewrap findings leave these obligations incomplete | FAIL (`FIND-TASK-007-1`–`-3`, `-5`–`-7`, `-9`) |
| Repository rules | Struct ownership, SQL boundaries, generated contracts, journeys, and most gates conform; async file IO and local production imports do not | FAIL (`FIND-TASK-007-7`, `-8`) |
| Scope and non-goals | Product non-goals remain excluded; four unrelated skill-policy edits entered the range | FAIL (`FIND-TASK-007-4`) |

## Review-wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TREV-001`–`TREV-005` |
| Repository standards | FAIL | `STD-007-01`–`STD-007-04` |
| Security domain | FAIL | `SEC-001`–`SEC-004` |
| Persistence and delivery domain | FAIL | `DR-DEL-1`–`DR-DEL-3` |
| Ponytail validation | FIX_REQUIRED | Nine deduplicated findings; six confirmed and three revised |

## Validated finding ledger

| Finding | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-007-1` | CONFIRMED | VIOLATION | Remove provider selectors and raw causes from public errors and logs while preserving stable code and retry classification. |
| `FIND-TASK-007-2` | CONFIRMED | INCORRECT | Restore and enforce multi-tenant production fail-start validation of every active tenant KEK before readiness. |
| `FIND-TASK-007-3` | REVISED | VIOLATION | Screen and pin each effective HTTP URL before attaching the already-authorized credential to the screened send. |
| `FIND-TASK-007-4` | CONFIRMED | DRIFT | Remove the four unrelated implementation/review skill changes from this candidate. |
| `FIND-TASK-007-5` | REVISED | VIOLATION | Require HTTPS Vault in multi-tenant production and apply the existing owner-only file policy to Vault token files. |
| `FIND-TASK-007-6` | CONFIRMED | INCORRECT | Project precise provider-discriminated Python and TypeScript types matching the flattened Rust wire view. |
| `FIND-TASK-007-7` | CONFIRMED | VIOLATION | Move mounted-secret filesystem work off async executor threads while preserving mode checks and redaction. |
| `FIND-TASK-007-8` | CONFIRMED | VIOLATION | Move the three production/library function-scoped imports to module import blocks. |
| `FIND-TASK-007-9` | REVISED | INCORRECT | Run rewrap independently of delivery claims with finite elapsed-time bounds and per-pass old-key-version reuse. |

Full evidence, caller tracing, consequences, corrections, and closure proofs are
preserved in `findings-validation.md`. No proposed finding was rejected. None
requires a new product, public API, security, compatibility, concurrency, or
persistent-data decision, so specification revision is not required.

## Prior-finding closure

This is the first review attempt for TASK-007. There are no prior stable
findings to close or preserve.

## Verification limits

- Reviewers inspected the complete immutable diff and relevant callers but did
  not rerun the large Cargo/Postgres/Python/TypeScript lanes within the review
  budget. Candidate evidence records those required non-credentialed lanes as
  exit zero.
- The credentialed Slack/PagerDuty live smoke was not run and remains gated
  release evidence.
- Generated outputs were traced to their owners and generators rather than
  inspected line by line.
- All required reviewers completed within the 20-minute per-reviewer limit; no
  sub-reviewer gap needs to be recorded.

## Verdict

**FIX_REQUIRED**

The remediation task is
`changes/active/verified-change-contract/review/TASK-007-r1/TASK-007-R1-operator-delivery-corrections.md`.
