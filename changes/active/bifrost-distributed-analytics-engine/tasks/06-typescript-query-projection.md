---
id: BIFROST-R3-T06-TYPESCRIPT
title: Project the terminal-safe query lifecycle through TypeScript
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 3
depends_on: [BIFROST-R3-T04-PRODUCTION-ACTIVATION]
requirements: [REQ-001, REQ-008, REQ-010, REQ-011]
invariants: [INV-001, INV-003, INV-004, INV-005, INV-008]
acceptance: [AC-006, AC-007, AC-008]
parent_task: BIFROST-R3-T3-FIRST-CLASS-PROJECTIONS
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# TypeScript query projection

## Outcome and value

`@wyrd/sdk` exposes an idiomatic incremental async iterator with the generated
selected-path terminal, structured `WyrdError`, and runtime-only `AbortSignal`
ergonomics. Abort, `return()`, decode/protocol/transport failure, finalization,
and normal terminal settle one native server query and release listeners,
N-API handles, and Rust state exactly once.

Required execution skill: `$wyrd-implement`.

## Current-state amendment and owners

Retain `vala-sdk` as the shared Rust client, `crates/bindings/wyrd-node` as the
N-API boundary, and `typescript/wyrd` as the idiomatic wrapper. The current
incremental stream is the starting seam. Add no serialized request path or
abort field: `signal?: AbortSignal` is wrapper-only and stripped before native
request construction. The private `@wyrd/testing` addon remains test-tier.

Do not add client routing, EXPLAIN, a TypeScript graph owner, V8 handles across
threads/awaits, or Node lifetime tests in Rust.

## Ordered implementation scenarios

### Scenario 1 — Generated terminal and structured error projection

**Behavior.** TypeScript receives `Interactive | Analytical`, validated terminal
outcome, and canonical error properties without string parsing or integer
truncation. Maps REQ-001, REQ-008, REQ-010, INV-001, INV-003, AC-006.

**RED.** Add test `projects typed path terminal and canonical errors` in
`tests/unit/bifrost-query.test.ts`; include malformed/missing/duplicate terminal
and values outside JavaScript's safe integer range. Exact:

```bash
mise run ts:build
mise exec -- bash -lc "cd typescript/wyrd && pnpm exec vitest run tests/unit/bifrost-query.test.ts -t 'projects typed path terminal and canonical errors'"
```

**GREEN.** Project the shared native terminal into generated N-API declarations
and the wrapper's closed union. Keep approved string/nullable representations
for unsafe integers. Throw the existing TypeScript `WyrdError` with canonical
problem fields; protocol failures use a stable server/shared error mapping.

**REFACTOR.** Generated `index.d.ts` remains source-derived; wrapper types add
ergonomics but no durable alternative contract.

### Scenario 2 — Abort and iterator settlement state machine

**Behavior.** Already-aborted signals start no native query. Startup/streaming
abort, repeated abort, `return()`, terminal-versus-abort, decode error,
transport error, and finalization use one idempotent settlement and remove the
listener once. Maps REQ-010, INV-003, INV-004, AC-006.

**RED.** Add test `abort signal settles every terminal race` in the same unit
target with deterministic native gates and exact start/cancel/close/listener
counts. Exact:

```bash
mise run ts:build
mise exec -- bash -lc "cd typescript/wyrd && pnpm exec vitest run tests/unit/bifrost-query.test.ts -t 'abort signal settles every terminal race'"
```

**GREEN.** Make the wrapper own `idle -> starting -> streaming -> terminal |
closing -> closed`. Register one abort listener before native start, check
`signal.aborted`, and remove it in the single `settle` finally path. Native Rust
state requests server cancellation and awaits cleanup before releasing handles;
a validated terminal that wins remains authoritative. `return()` shares the
same path. Finalization signals/leak-reports but never fabricates success.

**REFACTOR.** State transition methods remain cohesive on `BifrostQueryStream`;
callbacks do not duplicate cancellation policy.

### Scenario 3 — Real Node client journey

**Behavior.** The installed package drives Interactive and Analytical success,
post-selection structured failure, abort/iterator close, terminal paths,
production telemetry deltas, and zero server ownership. Maps REQ-001, REQ-008,
REQ-010, REQ-011, AC-006, AC-007, AC-008.

**RED.** Extend `tests/integration/oracle-query.test.ts` with `query paths failure
abort and cleanup`; use the private testing addon only to boot the real server,
then the public package for every query. Exact:

```bash
mise run ts:build
mise run ts:build:testing
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd typescript/wyrd && pnpm exec vitest run tests/integration/oracle-query.test.ts -t 'query paths failure abort and cleanup'"
```

**GREEN.** Select Task 4's existing test-tier multi-node `WyrdTestServer`
composition through the private testing addon without exporting a cluster
class. Drive production routes and native client. Do not reproduce physical
operator qualification or add a TypeScript-specific server lifecycle.

**REFACTOR.** Keep test harness and SDK addons separate; production packaging
must not contain testing symbols.

## Broader verification

```bash
mise run ts:build
mise run ts:typecheck
mise run ts:test:unit
mise run ts:test:integration
mise run ts:napi:check
mise run ts:pack:check
mise run codegen:check
mise run check:client-tier
mise run fmt
mise run lints
git diff --check
```

## Completion evidence and stop conditions

Provide N-API declaration provenance, state/race traces, listener/native release
counts, package-level journey evidence, and zero ownership/telemetry snapshots.
Return `SPEC_REVISION_REQUIRED` if TypeScript requires a divergent wire field,
path selector, successful partial output, client durable ownership, new
dependency/feature, or weakened auth/tenant/audit semantics.

## Authority

`architecture/references/languages/typescript-guide.md`,
`architecture/references/languages/errors.md`, and `AGENTS.md` §§3, 9, 11.
