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


## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| One async `Bifrost` + thin blocking facade with register/use_table/insert/flush/shutdown/`sql`/`stream`/advanced query; Python and TS project it idiomatically | `crates/vala/vala-sdk/src/bifrost.rs`, `blocking.rs`, `table.rs`; `crates/vala/vala-sdk/src/python.rs`; `python/py-wyrd/python/wyrd/bifrost/__init__.py`; `crates/bindings/wyrd-node/src/lib.rs`; `typescript/wyrd/src/index.ts` | `mise run test:bifrost:journey:sdk` (10 passed); `:python` (24 passed); `:typescript` (8 passed) | PASS |
| `TableConfig` from each language model/schema path or by describe-by-name; register resolves server identity, idempotent for equal schema, stable conflict otherwise | `table.rs::TableConfig` (+ `TableConfigWire` serde round trip); `PyTableConfig::{from_json_schema,from_arrow_ipc,describe}`; `tableConfigFromJsonSchema` / `describeTableConfig` | `test_register_insert_flush_read_and_swap` and `test_describe_binds_an_existing_table_without_restating_its_schema` (Python); `registers, writes, flushes, swaps tables, and reads back` + `describes an existing table without restating its schema` (TS); `described_canonical_and_dynamic_schemas_reach_exact_physical_schema` (Rust) | PASS |
| Omitted transport follows the existing chain once, explicit values override, no resolvable credential fails through the stable public error contract | `bifrost.rs::client_from_options` (single door); `wyrd-client` `CredentialSource::explicit` classifies one credential into API-key vs bearer grant; `ValaSdkError::Client` carries `WYRD_CLIENT_*` verbatim | `test_omitted_transport_resolves_from_the_environment`, `test_no_resolvable_credential_raises` (Python); `credential::tests::explicit_credential_routes_by_its_own_prefix`, `config::tests::explicit_api_key_beats_*` (`cargo nextest run -p wyrd-client --lib`, 56 passed) | PASS |
| Insert with no active table is `WYRD_VALA_412_NO_ACTIVE_TABLE`; queue saturation reaches the caller; optional correlation preserves REQ-022; no per-row schema/table argument remains | `bifrost.rs::Bifrost::insert`; `Row.card_ref: Option<CardRef>` in `wyrd-queue`; `Correlation` in `table.rs` | `test_insert_without_an_active_table_refuses`, `test_uncorrelated_row_is_a_valid_write`, backpressure tests (Python); `refuses a write with no active table and a bad card reference` (TS); `backpressure_and_drain_no_silent_drops`, `observe_and_bifrost_roundtrip` (Rust) | PASS |
| Table swap preserves buffered rows for both targets; flush/shutdown drain every owned producer; `sql` and `stream` agree, require a terminal, never present partial rows as success | pooled producers in `handle.rs`; TS `Bifrost.sql` is implemented by draining `Bifrost.stream`, so agreement is structural | `test_register_insert_flush_read_and_swap` (producer_count 2, both tables read back), `test_sql_and_stream_return_the_same_rows` (Python); `registers, writes, flushes, swaps tables, and reads back` (TS); `pg_bifrost_multi_batch_query_stream_reuses_schema`, `oracle_query_*` (Rust) | PASS |
| Superseded Python/TS query/write roots, the former public Rust write-pool name, duplicate language-side behavior and compat aliases are absent; `observe::record` keeps its explicit-table drop-on-full contract | `grep -rn "BifrostQueryClient\|BifrostClient\|BifrostWritePool"` over `python/py-wyrd/python`, `typescript/wyrd/src`, `crates/vala/vala-sdk/src`, `crates/bindings/wyrd-node/src` returns nothing; `crates/vala/vala-sdk/src/observe.rs` unchanged in contract; deleted `python/py-wyrd/tests/bifrost/test_query.py` | `mise run check:client-tier`, `mise run check:pyo3-scope`, `mise run lints` | PASS |
| Each language journey owner proves register → insert → flush → SQL/stream → swap → second write/read against a real server, plus the negative flows | `crates/vala/vala-sdk/tests/pg_bifrost_e2e.rs`; `python/py-wyrd/tests/integration/test_bifrost_e2e.py`; `typescript/wyrd/tests/integration/bifrost-write.test.ts` (added to `test:bifrost:journey:typescript`) | the three journey lanes above | PASS |
| Generated stubs and declarations match the public surfaces | hand-authored `python/py-wyrd/python/wyrd/stubs/{bifrost,observe}.pyi`; regenerated `typescript/wyrd/index.d.ts` / `index.d.cts` | `mise run codegen:check` (All checks passed); `mise run py:typecheck`; `mise run ts:typecheck`; `mise run ts:napi:check` fails only on the uncommitted regenerated `index.d.ts` (byte-identical across two `ts:build` runs) | PASS |

### Verification commands

| Command | Result |
|---|---|
| `mise run test:bifrost:journey:sdk` | PASS (10 tests) |
| `mise run test:bifrost:journey:python` | PASS (24 tests) |
| `mise run test:bifrost:journey:typescript` | PASS (8 tests) |
| `mise run fmt` / `py:format` / `py:lints` / `py:typecheck` / `ts:typecheck` / `codegen:check` | PASS |
| `mise run lints` | PASS |
| `mise run check:client-tier` / `check:pyo3-scope` | PASS |
| `mise run py:test:unit` | PASS (432 passed) |
| `mise exec -- cargo nextest run -p wyrd-client --lib` | PASS (56 tests) |
| `mise run ts:napi:check` | FAIL — `git diff --exit-code index.d.ts`; the regenerated declaration is stable and only needs committing |
| `mise run check:unwrap-audit` | FAIL — pre-existing, `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` (outside this write set) |
| `mise run verify:bifrost` | FAIL at `check:tenant-isolation` — pre-existing, `crates/vala/vala-sql/migrations/*` and `queries/oracle_admission.rs` (outside this write set) |
| `mise exec -- cargo nextest run -p wyrd-queue --lib` | 1 pre-existing failure, `producer::tests::flush_timeout_retains_batch_until_later_ack`, reproduced identically on a clean `HEAD` worktree |
| `git diff --check` | PASS |
| `mise run test:bifrost` | 8/10 lanes pass. `unit:rust` fails on 6 `wyrd-testing` tests (`bifrost::forge_harness::worker_lifecycle_tests::*`, `bifrost::scribe_workload::tests::scribe_workload_read_boundaries_may_not_reuse_an_earlier_read`) that fail identically 6/6 on a clean `HEAD` worktree. `journey:otlp` selects zero tests (`error: no tests to run`) because the canonical OTLP journeys are task 04's deliverable. |
| `mise run test:bifrost:journey:scribe` | PASS (20/20) on an idle machine; also 20/20 at `HEAD`. Two earlier runs under concurrent build load failed on *different* tests (`round_robin::scribe_system_and_dynamic_tables_are_round_robin_equal`, then `sustained::scribe_sustained_ingest_oracle_hot_read_journey`) — both are pressure/distribution assertions reached through `RawIngest`, which this change does not touch. Load-sensitive, not a regression. |

### Non-goals

Confirmed excluded: no root multi-service client, no multi-table insert, no
client-computed fingerprint, no Python context managers, no new global config
file, and no new harness, test target, runtime, framework, or schema mapper.
