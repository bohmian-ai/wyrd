---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-sdk-authoring-ergonomics
spec_revision: 5
requirements: [INV-001, INV-002, INV-003, AC-003, AC-005]
depends_on: [TASK-002]
parent_task: TASK-002
remediates: [FIND-TASK-002-2, FIND-TASK-002-3]
---

# Finish general state lookup and required documentation

Route directly to `$wyrd-implement`. Preserve the accepted sharing implementation; complete one missing TS optimization and the mandatory documentation repair together.

Authority: approved revision 5 at `changes/active/sdk-authoring-ergonomics/spec.md`; original task `changes/active/sdk-authoring-ergonomics/tasks/TASK-002-client-authoring-state-memory.md`. Original implementation base `44581bfd33145603ee6565d571a68c823003a86b`; reviewed candidate `67b421735d3ee37c538e068546bf9a50224db015`. Evidence-only HEAD `16393178accb4fb364555c5f0ebb32a2cbc37bc2`. Review authority and validated diagnoses: this directory's `verdict.md` and `findings-validation.md`. Retain these identities for closure and cumulative review.

## Diagnosis and selected correction

### Maintainer waiver: FIND-TASK-002-1

On 2026-11-11 the maintainer explicitly waived additional attributed measurement
work and accepted narrower performance claims. The original task now limits
quantitative claims to the recorded Instruments process totals and withdraws
per-stack/per-kind ratios. FIND-TASK-002-1 is historical and requires no
remediation. No new profiling recordings or owner-level measurement work are
required here; verify the TS correction with focused regression and source proof.

### FIND-TASK-002-2: discarded TS envelope

At `sdks/wyrd-sdk-ts/wyrd/src/index.ts:3180`, `WyrdState.get` calls `card(alias)` solely to read kind for Agent/Prompt dispatch, then calls `agent(alias)` or `prompt(alias)`. Native `card` (`native/src/cards.rs:632`) serializes the complete stored envelope, `NativeLifecycleResult` (`native/src/lib.rs:190,207`) emits owned JSON and `lifecycleValue` (`wyrd/src/native.ts:117`) parses it. The typed accessor serializes/parses the same envelope again. Both fixed warm TS workloads execute this public path 1,000 times. Borrowed native projections remove other intermediates but leave this payload-sized discarded result.

Keep dispatch in existing TS WyrdState and choose kind through existing `cardRef(alias)` (`index.ts:3152`, native `card_ref` at `cards.rs:644`). Then request only the selected full Agent/Prompt projection; other kinds retain normal `card(alias)` results. Shared Rust and Python dispatch already use the exact-reference kind from the same immutable validated index. Preserve errors, result shape and fresh mutable output. This small local correction removes the producer of discarded full bodies; consumer guards, a new native method/cache/dispatch abstraction, or shared mutable outputs are unnecessary. No Rust/Python dispatch or lifecycle change is required.

### FIND-TASK-002-3: required Rust documentation

Root AGENTS and `agent_docs/rust.md` require rustdoc on changed items regardless of visibility, including fields, nested helpers and tests, Errors on fallible functions, and Panics where applicable. The following complete validated inventory remains open:

| Candidate item/location | Required documentation |
|---|---|
| `crates/skald/skald-spec/src/prompt.rs:90`, Sink tuple field | Retained hasher role |
| Same file `:92,96`, Sink write/flush | Intent and Errors; streamed fingerprint bytes, no-op flush, no IO errors produced |
| Same file `:973`, Count tuple field | Retained serialized-byte count |
| Same file `:975,979`, Count write/flush | Intent and Errors; counted bytes, no-op flush, no IO errors produced |
| Same file `:1027`, nested slot_variables scan | Intent, restoration of temporarily removed system slot before either error propagates, Errors |
| `crates/wyrd-spec/src/card/agent.rs:187`, CompiledPrompt tuple field | Canonically compiled native Prompt retained by wrapper |
| `crates/shared/wyrd-client/src/authoring/tests.rs:1850,1882`, both new capture tests | Panics for failed fixture/declaration/ownership/count invariants |
| `crates/shared/wyrd-loader/src/order.rs:297,313`, both changed ordering tests | Intent and Panics for fixture/order assertion failures |
| `crates/shared/wyrd-loader/src/validate.rs:171`, changed duplicate-binding test | Panics for fixture/assertion failures |

Add concise documentation at these owners without changing implementation or assertions. The explicit merge gate makes this mandatory even with passing tests. No helper redesign, unrelated test documentation sweep or optional import cleanup is part of this task.

## Preserved behavior and non-goals

Preserve public signatures, JSON and persisted contracts, exact references, errors, unknown aliases/kind refusals, graph validation/hash/replay, snapshots/replacements and Artifact publication. Preserve Rust borrows, Python holder identity/GC and TS independent mutable results, including Agent Prompt navigation. Keep existing owners, dependencies, production allocators and lifetimes. Do not redesign state loading, graph construction, Prompt caching, run/verification/evidence ownership, writer cleanup, authorization or tenancy. No full journey or aggregate rerun belongs to this remediation.

## Ordered proof and acceptance

1. For FIND-TASK-002-2, add/select one focused regression in the existing TS state/native test boundary. Before changing dispatch, prove that general Agent/Prompt access makes the discarded preliminary full-envelope projection. After correction, prove only the selected full projection occurs, typed shape and independent mutable output remain correct, and unknown aliases/other-kind envelope results retain their behavior. Confirm and record the actual exact selector before RED; do not introduce a harness just to count an implementation detail. Existing mutable-output preservation assertions remain intact.
2. For FIND-TASK-002-3, statically inspect every inventory item for intent/invariants and applicable Errors/Panics. Documentation uses verification-only proof; do not manufacture runtime tests. Preserve the narrowed measurement claims; make no new numerical savings claim for this correction without measurements.

Narrow verification for the final write set:

```bash
mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/observe.test.ts tests/unit/authoring.test.ts
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run fmt:check
mise exec -- git diff --check
```

Record the new regression's exact command separately after confirming its name. Rust changes here are documentation-only: focused source audit plus formatting supplies their closure proof. If implementation scope changes, add only the narrow check covering that change; run Cargo-backed commands sequentially. No additional profiling, full SDK journey matrix, full crate suite or broad gate is required here; integrated change review owns the broader behavioral proof.

Closure review retains the original task/candidate, original findings and the explicit waiver, and checks the two active findings, remediation diff and affected consumers. It carries forward unchanged source-backed acceptance rather than restarting comprehensive discovery.

## Implementation evidence

Commits: `2fb4a0c4c` (TS dispatch + regression), `10eeb9f8d` (Biome format of the committed profiling workload, which turned `ts:format:check` red), `d8fd0aab6` (rustdoc).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-002-2: `get` projects only the selected full Card | `sdks/wyrd-sdk-ts/wyrd/src/index.ts` `WyrdState.get` dispatches on `cardRef(alias).kind`; Agent/Prompt call only `agent`/`prompt`, others only `card` | RED before change: `card` called once for an Agent (`[1,1,0]` vs `[0,1,0]`); GREEN after. `state get projects only the full Card of the alias's kind` spies native `card`/`agent`/`prompt`, checks Agent/Prompt/Model counts, Agent→Prompt navigation, an independent mutable result, and the unknown-alias 404 with no projection | PASS |
| Preserved typed shape, errors, mutable output | Existing assertions unchanged | `vitest run tests/unit/observe.test.ts tests/unit/authoring.test.ts tests/unit/wyrd-client.test.ts`: 48 passed | PASS |
| FIND-TASK-002-3: rustdoc inventory | `Sink`/`Count` fields + write/flush (Errors), nested `scan` (restoration invariant, Errors), `CompiledPrompt` field, two capture tests, two order tests, duplicate-binding test (Panics) | Static audit of each inventory item; `cargo check --locked -p wyrd-skald-spec -p wyrd-spec -p wyrd-loader -p wyrd-client --tests` | PASS |
| Formatting and hygiene | — | `ts:format:check`, `ts:lints`, `ts:typecheck`, `fmt:check`, `git diff --check` | PASS |

Regression command: `mise exec -- pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/unit/wyrd-client.test.ts -t "state get projects only the full Card of the alias's kind"` (needs a current `dist`; `ts:build` or `tsc -p tsconfig.build.json`).

Non-goals held: no Rust/Python dispatch, native method, cache, signature, contract or assertion change; Rust edits are documentation-only; no new numerical claim and no new profiling.
