# TASK-004 R2 Verdict

## Immutable subject

- Base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Candidate: `2af4cc3ff95a609d1df682be4f633345f96934e1`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Prior review and remediation: `changes/active/verified-change-contract/review/TASK-004-r1/`

The candidate remained unchanged throughout both review waves.

## Verdict

**FIX_REQUIRED**

The cumulative candidate closes six of the seven prior findings and satisfies
the result-publication, security, tenancy, audit, SDK/MCP, and most durable
runtime obligations. It does not yet establish a known ordering between
scheduler shutdown and an in-flight occurrence commit, and the cumulative
changed Rust surface still contains mandatory import-manifest violations.

## Acceptance matrix

| Obligation | Result | Evidence |
|---|---|---|
| One supervised durable Verification runtime and typed HTTP/SDK/MCP status/manual contract | PASS | Task review source matrix and recorded Rust/Python/TypeScript/MCP journeys |
| SYSTEM identity, exact Verifier scope, tenant isolation, and one canonical Gate audit decision | PASS | Security review; focused Gate tests; recorded authenticated gRPC proof |
| Result schemas map by name; details precede summary; every required ACK gates settlement and dispatch | PASS | Bifrost review; independently rerun mapping, crash/reclaim, and role-separated proofs |
| Durable claims, lease fencing, retry accounting, restart, permits, and bounded drain | PASS | Persistence review and recorded Postgres/runtime evidence |
| Shutdown admits no scheduler work with an unknown commit outcome | **FAIL** | `FIND-TASK-004-5`; scheduler cancellation can drop an in-flight `COMMIT` future |
| Required Rust top-level import/bare-type source shape across the cumulative changed surface | **FAIL** | `FIND-TASK-004-8`; five changed module seams retain qualified signature/field types |
| Prohibited alternate broker, local Scribe path, result endpoint, permission, identity hierarchy, or recovery protocol remains absent | PASS | Complete cumulative diff inspection |

The detailed requirement-by-requirement matrix is preserved in
`task-review.md`. Wave 2 revised its shutdown and prior-finding conclusions as
recorded in `findings-validation.md`.

## Wave results

| Review | Result | Material finding |
|---|---|---|
| Task implementation | PASS, revised by Wave 2 | None proposed |
| Repository standards | FAIL | `STD-004-R2-001` |
| Security, tenancy, and audit | PASS | None |
| Persistence, concurrency, and shutdown | FAIL | `PERSIST-R2-001` |
| Bifrost publication, Arrow, and durability | PASS | None |
| Structured Ponytail validation | Non-empty validated ledger | `FIND-TASK-004-5`, `FIND-TASK-004-8` |

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-004-5` | CONFIRMED; prior finding remains open | INCORRECT | Give scheduler cancellation and occurrence commit one known linearized ordering, and prove the in-flight-commit branch with a deferred-commit Postgres test. |
| `FIND-TASK-004-8` | CONFIRMED; new finding | VIOLATION | Replace the cited qualified field/signature/impl types with top-level imports and bare names across the complete cumulative changed surface. |

The exact locations, reachability, consequences, decision-complete corrections,
and focused closure proofs are authoritative in `findings-validation.md`.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-004-1` | CLOSED |
| `FIND-TASK-004-2` | CLOSED |
| `FIND-TASK-004-3` | CLOSED AS SCOPED; distinct omitted locations are `FIND-TASK-004-8` |
| `FIND-TASK-004-4` | CLOSED |
| `FIND-TASK-004-5` | OPEN |
| `FIND-TASK-004-6` | CLOSED |
| `FIND-TASK-004-7` | CLOSED |

## Verification limits

- Reviewers inspected the complete cumulative diff and candidate source. They
  did not rerun the entire recorded repository matrix.
- Focused Arrow mapping, Gate, detail-ACK crash/reclaim, and role-separated
  journey tests were independently rerun and passed. Static boundary checks
  rerun by the standards reviewer also passed.
- Existing green tests do not exercise cancellation while the scheduler's
  `COMMIT` itself is in flight. The current scheduler test covers rollback
  before commit selection only.
- The task's `mise run test:e2e` command name is not present in the repository;
  recorded capability-specific real-server journeys supply the applicable
  evidence, but the command-name mismatch remains an artifact limitation.
- No reviewer exceeded the requested 20-minute sub-review ceiling, and every
  required report is present.

## Remediation

Execute
`changes/active/verified-change-contract/review/TASK-004-r2/TASK-004-R2-close-scheduler-ordering-and-source-shape.md`
through `$wyrd-implement`, then review the complete original
base-to-remediated-candidate range again.
