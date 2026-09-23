# TASK-002 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Branch: `verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `fbfc2591a985b288935180098f892aecdf3b8b49`
- Range: `c8bb490ad814c0c7770cac33ed7779897ff776e4..fbfc2591a985b288935180098f892aecdf3b8b49`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Logic authorities: `changes/active/verified-change-contract/architecture/logic/run_api.md` and `table_schema.md`

The candidate remained unchanged throughout both review waves.

## Wave 1 results

| Independent review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TASKREV-001` through `TASKREV-006` |
| Repository standards | FAIL | `RS-001` through `RS-006` |
| Contracts and SDK projections | FAIL | `DC-1` through `DC-4` |
| Data and durability | PASS | None |
| Security and tenancy | FAIL | `SEC-AUD-001`, `SEC-TEN-001` |
| Concurrency and lifecycle | FAIL | `CONC-001` through `CONC-003` |

## Acceptance matrix

| Obligation | Result | Evidence or retained finding |
|---|---|---|
| Canonical Drift and Eval records omit duplicate run/Verifier identity | PASS | Canonical contracts and generated schemas match revision 32. |
| Five fixed verification tables have the exact approved physical schemas, managed columns, partitions, and Bloom floors | PASS | Vala table definitions and exact schema tests match `table_schema.md`. |
| Writer principal and observed subject Card identity remain distinct | PASS | Shared correlation projection and server-stamped publisher identity preserve the split. |
| Rust configured startup matches the locked public API | FAIL | `FIND-TASK-002-1` |
| Startup refuses every incompatible fixed-table schema | FAIL | `FIND-TASK-002-2` |
| Python mapping/dataclass inputs reject non-string keys before serialization | FAIL | `FIND-TASK-002-3` |
| TypeScript rejects values that JSON would omit or coerce | FAIL | `FIND-TASK-002-4` |
| Rust, Python, and TypeScript use explicit trace IDs first and runtime-local active spans second | FAIL | `FIND-TASK-002-5` |
| Concurrent dynamic-table first use converges on one describe and one existing producer | FAIL | `FIND-TASK-002-6` |
| Every successful state shutdown is terminal, including start/shutdown races | FAIL | `FIND-TASK-002-7` |
| Ambiguous state shutdown is proven retryable on the same handle | FAIL | `FIND-TASK-002-8` |
| Bounded audit lock timeout preserves an honest caller-owned transaction result | FAIL | `FIND-TASK-002-9` |
| New Rust modules/items meet mandatory documentation rules | FAIL | `FIND-TASK-002-10` |
| Imports and signature types meet repository placement rules | FAIL | `FIND-TASK-002-11` |
| Real SDK/server boundaries refuse unknown and unauthorized dynamic-table describes before admission | FAIL | `FIND-TASK-002-12` |
| Fixed-size binary trace/span columns decode canonical hex losslessly | PASS | Focused `wyrd-queue` regression and schema projection evidence. |
| Drift projects one tall row per feature and Eval projects one canonical row | PASS | Shared client projection tests and three SDK journeys. |
| Explicit destinations cannot switch inside an active view; reserved tables are refused | PASS | Immutable views, local guard, and SDK tests. |
| Observation calls enqueue through the existing bounded queue without flushing or waiting for a verdict | PASS | Shared Bifrost facade and producer path; proposed atomic multi-row admission was rejected as contrary to the approved contract. |
| No second queue, transport, schema authority, run registry, observation type, or retention policy was introduced | PASS | Complete diff inspection. |
| Adjacent verification-forced fixes preserve their stated production boundaries | FAIL | Forge clock-domain fixes pass; audit timeout contract remains `FIND-TASK-002-9`. |

## Validated finding ledger

| ID | Status | Classification | Correction boundary |
|---|---|---|---|
| `FIND-TASK-002-1` | CONFIRMED | VIOLATION | Restore the locked configured Rust startup over existing `QueueConfig` and `Bifrost::connect_with_config`. |
| `FIND-TASK-002-2` | CONFIRMED | INCORRECT | Compare the complete ordered fixed-table user schema, including nullability. |
| `FIND-TASK-002-3` | CONFIRMED | INCORRECT | Reject non-string Python mapping/dataclass keys before the existing strict dump. |
| `FIND-TASK-002-4` | CONFIRMED | INCORRECT | Reuse one strict TypeScript boundary serializer for observation inputs and media. |
| `FIND-TASK-002-5` | CONFIRMED | MISSING | Capture runtime-local active OpenTelemetry identity in Python and Node when explicit IDs are absent. |
| `FIND-TASK-002-6` | REVISED | INCORRECT | Serialize rare cache misses on the existing `Bifrost` owner and recheck the existing cache. |
| `FIND-TASK-002-7` | CONFIRMED | INCORRECT | Fence lifecycle transitions so successful shutdown is terminal and a racing claim cannot reopen it. |
| `FIND-TASK-002-8` | CONFIRMED | MISSING | Prove ambiguous shutdown retry through `WyrdState` using the existing mock seam. |
| `FIND-TASK-002-9` | REVISED | REGRESSION | Propagate PostgreSQL lock timeout instead of returning success from an aborted transaction. |
| `FIND-TASK-002-10` | CONFIRMED | VIOLATION | Add only the missing intent-bearing module/item rustdoc. |
| `FIND-TASK-002-11` | CONFIRMED | VIOLATION | Move imports to module scope and use imported bare signature types. |
| `FIND-TASK-002-12` | REVISED | MISSING | Add the required real-boundary unknown/unauthorized describe evidence without broadening AC-030. |

The full independently validated evidence, caller tracing, consequences, and
closure proofs are preserved in `findings-validation.md`. `RS-004` was rejected:
atomic multi-row producer admission would contradict the approved one-row-at-a-time
queue contract.

## Verification limits

The recorded candidate evidence is broad and credible for the paths it runs:
`verify:bifrost`, shared and Rust SDK tests, the focused queue regression, all
three SDK unit/integration/type lanes, code generation, client/PyO3 boundaries,
formatting, lints, and diff checks passed. Wave 1 also ran the unwrap, Clippy
allow, mocks-scope, and production-wheel boundary checks successfully.

Those green lanes do not exercise the retained wrong-schema shapes, strict
foreign-runtime serialization cases, runtime-local active spans, concurrent
first describe, terminal start/shutdown race, state-level ambiguous retry,
explicit audit timeout result, or real unknown/unauthorized describe boundary.
They therefore cannot close the validated findings.

## Prior-finding closure

This is the first review of TASK-002. There are no prior TASK-002 finding IDs to
close. TASK-001 review records in the cumulative range do not substitute for
TASK-002 acceptance.

## Verdict

**FIX_REQUIRED**

The twelve retained findings are bounded implementation and proof gaps within
the approved revision 32 behavior. None requires a new product, public API,
architecture, security, compatibility, cross-service, concurrency-ownership,
resource-ownership, or persistent-data decision.

Remediation task:
`changes/active/verified-change-contract/review/TASK-002-r1/TASK-002-R1-close-scoped-observation-gaps.md`.
