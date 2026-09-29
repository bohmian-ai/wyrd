# TASK-004 Review Verdict — R1

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `49ad24707de47378b9df51764034c49a129c6b8b`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Review directory: `changes/active/verified-change-contract/review/TASK-004-r1/`

The candidate remained immutable throughout both review waves.

## Verdict

**FIX_REQUIRED**

The cumulative implementation substantially establishes the requested generic
verification runtime, but seven independently validated bounded findings remain.
None requires a specification revision.

## Acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Common durable run lifecycle, leased claims, retries, terminal states, requester identity, and status projection | `wyrd.verifier_runs`, `VerifierRunQueue`, Verification HTTP/shared-client/SDK/MCP surfaces, and the existing SQL/runtime/route journeys | PASS, subject to shutdown admission correction in `FIND-TASK-004-5` |
| Remote result publication through `wyrd_client::Bifrost`, detail-before-summary order, zero-detail handling, and all-required-ACK settlement | `ResultPublisher`, result builders, runner settlement, sealed replay and partial-failure tests | PASS for the implementation flow; required crash proof missing in `FIND-TASK-004-7` |
| SYSTEM result writer is tenant-local, internal-only, and confined to one exact Verifier scope with one canonical Gate audit decision | Provisioning, issuance, token verification, public-route exclusions, Gate matrix, and Scribe stamping | FAIL — absent/null correlation bypasses exact Verifier confinement (`FIND-TASK-004-4`) |
| Tenant SQL and cross-tenant discovery respect repository connection ownership | Runtime uses tenant transactions and `OperatorPool` for discovery | FAIL — raw application pools escape into runtime and fixture owners (`FIND-TASK-004-1`) |
| Result fields remain attached to their declared Arrow names | Current schemas and result mapping tests pass | FAIL — builders join independently obtained schemas by position (`FIND-TASK-004-2`) |
| Changed Rust follows the mandatory top-level import and bare-signature rule | Code compiles and lint evidence is green | FAIL — changed signatures and fields use qualified paths (`FIND-TASK-004-3`) |
| Shutdown immediately stops new scheduler and runner claims, then drains already admitted work for 30 seconds | Existing in-flight drain, release, restart, and health tests | FAIL — cancellation does not cover blocked durable claim transactions (`FIND-TASK-004-5`) |
| Role-separated runner proves remote summary/detail publication and exact writer/Verifier/subject/owner/binding identity | Existing no-local-Scribe journey publishes and queries a direct zero-detail result | FAIL — complete identity/detail journey proof is absent (`FIND-TASK-004-6`) |
| Prohibited alternate paths remain absent | No broker, local Scribe write, result endpoint, process-local registry, new permission, repair coordinator, or atomic cross-table claim entered the diff | PASS |

The complete requirement-by-requirement matrix is preserved in
`task-review.md`; the table above applies the independently validated ledger to
that matrix.

## Review results

| Reviewer | Result | Proposed findings |
|---|---|---|
| Task implementation review | PASS | None |
| Repository standards review | FAIL | `STD-004-R1-001`, `STD-004-R1-002`, `STD-004-R1-003` |
| Security, tenancy, and audit domain review | FAIL | `SEC-001` |
| Persistence and concurrency domain review | FAIL | `PERSIST-001` |
| Bifrost publication domain review | FAIL | `BIFROST-PUB-001`, `BIFROST-PUB-002` |
| Structured Ponytail validation | Complete; non-empty ledger | Six confirmed, one revised, none rejected |

## Validated finding ledger

| Stable ID | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-004-1` | CONFIRMED | VIOLATION | Remove raw `PgPool` fields/parameters from verification runtime and fixture owners; acquire tenant connections through the existing Postgres owner and retain `OperatorPool` only for authorized cross-tenant reads. |
| `FIND-TASK-004-2` | CONFIRMED | VIOLATION | Bind result arrays to table fields by name and reject missing, duplicate, or unexpected authored fields before constructing a batch. |
| `FIND-TASK-004-3` | CONFIRMED | VIOLATION | Replace qualified types in changed Rust fields/signatures/bounds with top-level imports and bare names. |
| `FIND-TASK-004-4` | CONFIRMED | INCORRECT / VIOLATION | Make the canonical reserved-table Gate decision require and authorize a non-null `card_ref` for every row against the SYSTEM token's sole Verifier scope before audit and durable admission. |
| `FIND-TASK-004-5` | REVISED | INCORRECT | Close scheduler and runner claim admission at cancellation across the existing transaction boundary; roll back uncommitted work or fenced-release a claim whose commit wins the race. |
| `FIND-TASK-004-6` | CONFIRMED | MISSING | Extend the existing role-separated journey to publish/query non-empty binding-created summary and details with exact identity and event-time assertions. |
| `FIND-TASK-004-7` | CONFIRMED | MISSING | Exercise a crash after detail ACK while summary is unknown and prove no completion/dispatch until the same run is reclaimed and every required ACK succeeds. |

Full evidence, caller traces, corrections, and closure proofs are authoritative in
`findings-validation.md`.

## Verification limits

- Reviewers inspected the complete base-to-candidate source and supplied
  verification evidence but did not rerun the full heavy verification matrix.
- Independently executed evidence included `git diff --check`; the task reviewer
  also ran the focused default 30-second drain test successfully.
- The task names `mise run test:e2e`, but that task does not exist in the current
  repository. The recorded `test:cards:integration` evidence covers the Rust SDK
  Verification journey and real HTTP route suite, but this command-name mismatch
  remains a task-artifact limitation.
- Existing tests do not cover the exact scope-bypass, cancellation/claim,
  role-separated identity/detail, or detail-ACK/crash interleavings retained in
  the ledger.
- No reviewer exceeded the user's 20-minute ceiling; no reviewer report is
  missing.

## Prior-finding closure

This is the first review attempt for TASK-004. There are no prior stable
findings to close.

## Remediation

Execute
`changes/active/verified-change-contract/review/TASK-004-r1/TASK-004-R1-close-validated-runtime-gaps.md`
through `$wyrd-implement`, then review the complete original
base-to-remediated-candidate range again.
