# TASK-002 R4 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Branch: `verified-change-contract`
- Original base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Cumulative candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior reviews and remediation: `changes/active/verified-change-contract/review/TASK-002-r1/` through `TASK-002-r3/`

The complete original-base-to-candidate range and every prior remediation were
reviewed. The candidate remained
`b56560e511918efbdd84d8756b13100fc381eda0` through both waves. Per explicit
current user authority, AI co-author trailers are allowed and are not findings.

## Wave 1 results

| Independent review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TASKREV-R4-001` |
| Repository standards | PASS | None |
| Contracts and SDK projections | FAIL | `DC-R4-001` |
| Data and durability | FAIL | `DATA-R4-001` |
| Security and tenancy | PASS | None |
| Concurrency and lifecycle | PASS | None |

## Acceptance matrix

| Obligation | Result | Evidence or finding |
|---|---|---|
| One state-owned Bifrost lifetime with retryable ambiguous drain and terminal successful shutdown | PASS | Shared lifecycle owner and focused lifecycle/concurrency tests |
| Fixed startup describes both required tables and refuses unavailable or incompatible schemas | PASS | Exact schema comparison and all three real SDK preflight journeys |
| One invocation supplies immutable Card-scoped views with writer/subject separation | PASS | Shared `Run`/`Observe` owner and three SDK journeys |
| Drift and Eval use canonical records and the existing bounded queue | PASS | Shared projections, fixed schemas, queue tests, and journey readback |
| Eval journeys preserve session/media and fixed-width trace/span identity and refuse invalid inputs | PASS | `FIND-TASK-002-13` remains closed in Rust, Python, and TypeScript journeys |
| Generic fixed-size-binary columns decode canonical hex with exact widths | PASS | Three focused queue regressions |
| Explicit dynamic routing describes once, caches, and refuses reserved/unknown/denied/stale declarations | PASS | Shared owner tests and real SDK/server journeys |
| TypeScript preserves every accepted observation value and refuses silent omission before native admission | **FAIL** | `FIND-TASK-002-4` reopened for accessor double reads and pre-validation media projection |
| Changed Rust imports and declaration types satisfy repository rules | PASS | `FIND-TASK-002-11` closed |
| Eval TypeScript option shapes are closed readonly aliases | PASS | `FIND-TASK-002-16` closed |
| Recorded evidence names the implemented structured error | PASS | `FIND-TASK-002-17` closed |
| Security, audit, tenant isolation, publisher/subject identity, and production cfg boundaries remain intact | PASS | Independent security review |
| Bounded admission, cache convergence, shutdown retry, and Oracle cutoff proof remain intact | PASS | Independent concurrency review |
| No second serializer, queue, cache, producer pool, transport, schema authority, lifecycle state, or authorization model | PASS | Complete cumulative diff inspection |
| No verdict wait, per-observation flush, run registry, retention policy, or atomic multi-row API | PASS | Source and non-goal inspection |

## Prior-finding closure

| Stable finding | R4 status |
|---|---|
| `FIND-TASK-002-1` | CLOSED |
| `FIND-TASK-002-2` | CLOSED |
| `FIND-TASK-002-3` | CLOSED |
| `FIND-TASK-002-4` | OPEN / REOPENED |
| `FIND-TASK-002-5` | CLOSED |
| `FIND-TASK-002-6` | CLOSED |
| `FIND-TASK-002-7` | CLOSED |
| `FIND-TASK-002-8` | CLOSED |
| `FIND-TASK-002-9` | CLOSED |
| `FIND-TASK-002-10` | CLOSED |
| `FIND-TASK-002-11` | CLOSED |
| `FIND-TASK-002-12` | CLOSED |
| `FIND-TASK-002-13` | CLOSED |
| `FIND-TASK-002-14` | CLOSED |
| `FIND-TASK-002-16` | CLOSED |
| `FIND-TASK-002-17` | CLOSED |

## Validated finding ledger

| ID | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-002-4` | REVISED / REOPENED | INCORRECT | Make the existing serializer stringify its one-read validated snapshot, and validate each original Eval media descriptor's own keys before projecting declared fields. |

The complete independently validated caller tracing, reproductions,
consequences, and focused proof are preserved in `findings-validation.md`.

## Verification limits

The task records `verify:bifrost` passing 9/9 at `d6231892`, after all R3 code
changes, together with TypeScript unit/type/integration lanes, three queue
fixed-width tests, the shared observation suite, formatting, lints,
Clippy-allow audit, and `git diff --check`. Wave 1 reran selected lifecycle,
queue, schema, and TypeScript tests at the candidate.

Those green checks do not exercise accessor values that change across reads or
properties removed before `mediaJson` reaches `strictJson`; both cases were
reproduced through the public built TypeScript `Observe.eval` surface.

## Verdict

**FIX_REQUIRED**

The one retained finding is a bounded correction in the existing TypeScript
boundary and requires no specification or architecture decision.

Remediation task:
`changes/active/verified-change-contract/review/TASK-002-r4/TASK-002-R4-finish-typescript-exact-value-validation.md`.
