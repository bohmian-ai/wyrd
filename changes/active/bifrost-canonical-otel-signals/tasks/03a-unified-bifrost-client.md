---
id: BIFROST-OTEL-T03A
title: Project one canonical Bifrost client through Rust, Python, and TypeScript
kind: implementation
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 9
design: unified-bifrost-client-draft.md
depends_on: [BIFROST-OTEL-T02-R1, BIFROST-OTEL-T03]
requirements: [REQ-001, REQ-004, REQ-009, REQ-017, REQ-021, REQ-022]
invariants: [INV-003, INV-006, INV-007, INV-008, INV-010, INV-011]
acceptance: [AC-002, AC-005, AC-008, AC-010, AC-011, AC-012]
---

# Unified Bifrost client

Required execution skill: `$wyrd-implement`.

## Objective

Ship the user-approved `unified-bifrost-client-draft.md` interface as the
canonical Bifrost client for Rust, Python, and TypeScript. One Rust-owned
implementation shall provide table registration, bound-table writes, unbound
SQL reads, streaming, flushing, and shutdown; Python and TypeScript shall be
idiomatic thin projections of that Rust behavior.

## Constraints

- `crates/vala/vala-sdk` owns the client behavior. PyO3 and N-API layers convert
  language values at the boundary and must not reimplement schema mapping,
  registration, credentials, query streaming, producer lifecycle, or errors.
- Preserve the approved draft's `Bifrost`, `TableConfig`, `ResolvedTable`,
  `Correlation`, and `QueryResult` surfaces, including the Rust blocking facade,
  Python `Bifrost`/`AsyncBifrost`, and the async TypeScript client.
- Reads remain unbound; writes target one active table at a time. Changing the
  active table must not discard or strand buffered work for the prior table.
- Reuse the existing `wyrd-client` credential and endpoint resolution,
  `wyrd-queue` schema conversions and writable-schema rules, producer pool,
  query stream, registration/description contracts, shared runtime, and stable
  public error catalog.
- The server remains authoritative for table identity, schema fingerprint,
  durable state, authorization, correlation, and query terminals. No client
  computes a competing fingerprint or accepts server-managed columns.
- Remove the superseded public split read/write roots and per-row schema/table
  arguments without compatibility aliases. Keep the draft's explicitly skipped
  features out of scope, including a root multi-service client, multi-table
  insert calls, client-side fingerprints, context managers, and a new global
  config file.
- Do not add a new harness, test target, runtime, framework, or schema mapper.
  A dependency edge is permitted only where the approved model-to-schema
  surface cannot use an already-owned dependency through the current boundary.

## Relevant Surface

- Rust owner and transport consumers under `crates/vala/vala-sdk/`, plus the
  existing `wyrd-client`, `wyrd-queue`, and `wyrd-spec::vala::api` contracts.
- The existing Bifrost registration and description routes in `wyrd-server`.
- PyO3 ownership in `vala-sdk`, native aggregation and public exports under
  `python/py-wyrd`, and source-driven Python stubs.
- N-API ownership under `crates/bindings/wyrd-node`, the public TypeScript SDK
  under `typescript/wyrd`, and N-API-generated declarations.
- Existing Rust, Python, and TypeScript Bifrost unit and user-journey owners,
  including `vala-sdk`'s `pg_bifrost_e2e` target and the repository Bifrost
  language lanes.

## Approach

1. Consolidate registration, description, active-table write lifecycle, SQL
   collection, terminal-safe streaming, and query escape-hatch behavior behind
   the Rust `Bifrost` owner, reusing the existing query and producer machinery.
2. Add the approved table configuration, resolved identity, optional
   correlation, collected query result, and shared-runtime blocking projections
   without moving server authority into the SDK.
3. Project the Rust owner into the approved synchronous and asynchronous Python
   surfaces, then regenerate public exports and stubs from their sources.
4. Project the same Rust owner through N-API into the approved asynchronous
   TypeScript surface, then regenerate declarations from their source.
5. Replace existing callers and tests of the superseded client split, deleting
   fake-native and source-text tests that do not exercise owned behavior.
6. Extend the nearest existing language journeys to prove the complete unified
   workflow and its user-visible refusal paths before task 04 consumes it.

## Acceptance Criteria

- Rust exposes one async `Bifrost` client and one thin blocking facade with the
  approved registration, table selection, insert, flush, shutdown, `sql`,
  `stream`, and advanced-query access. Python and TypeScript expose the approved
  language-idiomatic names and semantics over that Rust implementation.
- `TableConfig` can be created through each approved language model/schema path
  or described by registered table name. Registration resolves server-issued
  identity, is idempotent for an equal schema, and returns the stable conflict
  when the registered schema differs.
- Construction with omitted transport values follows the existing endpoint and
  credential chain once; explicit values override it; no resolvable credential
  fails through the stable public error contract.
- Insert without an active table fails with the approved cataloged
  `WYRD_VALA_412_NO_ACTIVE_TABLE` error. Queue saturation reaches the caller,
  optional Card/run correlation preserves REQ-022, and no per-row schema or
  table argument remains on the canonical surface.
- Switching tables preserves buffered rows for both targets, and `flush` and
  `shutdown` drain every owned producer while preserving failure evidence.
  `sql` and `stream` return the same rows, require a successful terminal, and
  propagate cancellation or incomplete-stream failures without presenting
  partial rows as success.
- Superseded Python and TypeScript query/write client roots, the former public
  Rust write-pool name, duplicate language-side behavior, and compatibility
  aliases are absent. `observe::record` retains its separate explicit-table,
  drop-on-full telemetry contract.
- Existing Rust, Python, and TypeScript user-journey owners each prove register
  → insert → flush → SQL/stream → table swap → second write/read through a real
  server, plus no-active-table, schema-conflict, under-privileged,
  non-SELECT/oversized-query, missing-credential, optional-correlation, and
  queue-full behavior at the nearest credible existing owner.
- Generated stubs and declarations match the public surfaces, and task 04 can
  express its canonical signal journeys exclusively through this client.

## Verification

Run the narrowest focused tests added or changed by the implementer, then:

```bash
mise run test:bifrost:journey:sdk
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run py:format
mise run py:lints
mise run py:typecheck
mise run ts:typecheck
mise run ts:napi:check
mise run codegen:check
mise run check:client-tier
mise run check:pyo3-scope
mise run fmt
mise run lints
mise run verify:bifrost
mise run gate
git diff --check
```

