---
id: BIFROST-OTEL-T03A-R1
kind: remediation
status: proposed
spec: changes/active/bifrost-canonical-otel-signals/spec.md
original_task: changes/active/bifrost-canonical-otel-signals/tasks/03a-unified-bifrost-client.md
base: 63ad8cbb8a03c7a83cf357a23fed794521ecbbb1
reviewed_candidate: ae18fea3dd39173763596ba825fdfe8ba3eeae76
required_skill: $wyrd-implement
findings:
  - FIND-BIFROST-OTEL-T03A-1
  - FIND-BIFROST-OTEL-T03A-2
  - FIND-BIFROST-OTEL-T03A-3
  - FIND-BIFROST-OTEL-T03A-4
  - FIND-BIFROST-OTEL-T03A-5
  - FIND-BIFROST-OTEL-T03A-6
---

# Complete the unified Bifrost client

## Issue diagnosis

`FIND-BIFROST-OTEL-T03A-1` identifies three omissions from the approved public surface. Rust `TableConfig` supports Arrow and JSON Schema but not the approved `schemars::JsonSchema` model constructor. The blocking facade delegates ordinary lifecycle/read/write methods but exposes no advanced-query access. TypeScript `QueryResult` exposes retained batches but not the approved `toArrow()` and `toBytes()` conversions. Current compile and type fixtures do not call these members, so their absence is invisible to green checks.

`FIND-BIFROST-OTEL-T03A-2` identifies incomplete acceptance proof. The existing Python and TypeScript owners exercise register, write, swap, flush, and read, but the Rust `pg_bifrost_e2e` owner never registers through the unified client and its newly adapted swap/backpressure cases use a mock sink. Across the reviewed language lanes, no unequal-schema registration conflict and no non-SELECT or oversized query is sent through the unified client. These are explicit task acceptance cases, not optional coverage.

`FIND-BIFROST-OTEL-T03A-3` identifies a reachable identity race. `Bifrost::register` sends a request built from the active table, releases the mutex for network IO, then applies the response to whichever table is active afterward. A synchronous `use_table` during the await can therefore stamp table A's server identity onto table B. The current equal-schema journeys cannot expose the race.

`FIND-BIFROST-OTEL-T03A-4` identifies split public-error authority. `WYRD_VALA_412_NO_ACTIVE_TABLE` and its status, title, and remediation are hand-written in `vala-sdk`, although `wyrd-spec::vala::error::BifrostError` is the derive-backed owner for public Bifrost errors crossing Python and TypeScript. Language tests prove the duplicate SDK mapping, not catalog ownership or generated metadata.

`FIND-BIFROST-OTEL-T03A-5` identifies hard repository-rule violations. The implementation added function-scoped imports in credential classification and SDK error projection, and the intended documentation for private `client_from_env` is accidentally attached to `register_outcome_name`. Lints do not replace the explicit module-import and all-items-rustdoc requirements.

`FIND-BIFROST-OTEL-T03A-6` identifies stale public guidance after the configuration rename. `WyrdClientError::NoCredentials` still tells users to pass removed `ClientConfig::api_key`, so the stable error's actionable text is now wrong.

## Intended correction outcome

The cumulative candidate exposes every approved Rust, blocking Rust, Python, and TypeScript client operation; registration can never associate a server identity with the wrong table; public no-active-table metadata has one catalog owner; the existing language journeys directly prove the omitted workflow/refusal cases; and no changed Rust or public credential guidance violates repository rules.

## Decision-complete recommendation

Keep `vala-sdk::Bifrost`, its private `WriterPool`, and the current language projections as the owners. Add the missing Rust model constructor by delegating to the existing JSON-Schema-to-Arrow path with the workspace's existing `schemars` dependency. Expose the async client's existing `QueryClient` through the blocking facade rather than adding a second advanced-query implementation. Implement the TypeScript collected-result conversions with the already-installed Apache Arrow package over the batches the result already owns; do not add native methods or another schema mapper.

Keep registration's network await outside the standard mutex, but associate the returned identity only with the exact table declaration that produced the request. A concurrent active-table replacement must remain intact and unresolved by another table's response. Extend the nearest existing Rust test owner with a deterministic delayed response or equivalent existing transport seam that swaps the binding while registration is in flight.

Extend `crates/vala/vala-sdk/tests/pg_bifrost_e2e.rs` for the missing real-server Rust unified workflow. Add unequal-schema registration conflict and non-SELECT/oversized query proof to the nearest existing language journey owners that already start `WyrdTestServer`; do not create a target, harness, fixture family, or duplicate cross-language scenario beyond what is needed to prove each public projection.

Put `WYRD_VALA_412_NO_ACTIVE_TABLE` in the existing derive-backed `BifrostError` catalog and have `ValaSdkError` delegate its public metadata to that owner. Preserve the existing client-local queue and credential codes; this remediation does not redesign their established taxonomy. Move the newly added imports to their module dependency blocks, repair the misplaced `client_from_env` rustdoc, and replace the stale `ClientConfig::api_key` guidance with `ClientConfig::credential` in the existing error owner and test.

## Constraints and preserved behavior

- Preserve one Rust-owned unified client, a private producer pool, unbound reads, one active write table, bounded queue refusal, terminal-safe streams, all-producer drain, and the separate drop-on-full `observe::record` contract.
- Preserve server authority for UID, fingerprint, authorization, correlation, durability, and query terminals.
- Preserve API-key prefix classification and opaque-token forwarding without client-side token-shape validation.
- Preserve the existing endpoint and credential precedence chain and the `ClientConfig::credential` name.
- Reuse `wyrd-queue` schema conversion/writable-schema owners, shared runtime, current N-API/PyO3 owners, existing Arrow dependencies, current journey targets, and `WyrdTestServer`.
- Keep `wyrd-spec` IO-free and PyO3-free. Regenerate public artifacts from their owners.
- Do not fix or suppress the reported pre-existing Forge/Scribe workload, OTLP placeholder, tenant-isolation, or unwrap-audit failures as part of this remediation.

## Explicit non-goals

- No root multi-service client, multi-table insert, client-side fingerprint, context manager, global config file, compatibility alias, migration, backfill, or legacy reader.
- No new harness, test target, runtime, framework, schema mapper, Arrow dependency, or parallel query/write implementation.
- No redesign of query streaming, producer lifecycle, authentication grants, or client-local queue error taxonomy.
- No unrelated cleanup of `vala-sdk`, `wyrd-client`, `wyrd-queue`, bindings, generated files, or test infrastructure.

## Acceptance criteria

| Finding | Required observable result |
|---|---|
| `FIND-BIFROST-OTEL-T03A-1` | A Rust compile/test fixture constructs `TableConfig` from a `schemars::JsonSchema` type; blocking Rust reaches the same advanced `QueryClient`; TypeScript type/runtime checks call `QueryResult.toArrow()` and `toBytes()` and receive the same rows/IPC represented by the result. |
| `FIND-BIFROST-OTEL-T03A-2` | The existing Rust journey registers, inserts, flushes, streams/collects, swaps, writes again, and reads both tables through a real `WyrdTestServer`; existing language journey owners prove unequal-schema conflict and the non-SELECT and oversized query refusals through the unified client. |
| `FIND-BIFROST-OTEL-T03A-3` | When the active table changes while registration is awaiting its response, no response identity is attached to a different table; the table that remains active retains its own prior resolution state. |
| `FIND-BIFROST-OTEL-T03A-4` | `WYRD_VALA_412_NO_ACTIVE_TABLE` code, status, title, and remediation come from one derive-backed `wyrd-spec` Bifrost error and match in Rust, Python, TypeScript, and generated metadata. |
| `FIND-BIFROST-OTEL-T03A-5` | No newly added production import remains function-scoped, `client_from_env` has accurate intent/error rustdoc, and repository format/lint/doc checks pass without suppressions. |
| `FIND-BIFROST-OTEL-T03A-6` | The no-credentials public text names only `ClientConfig::credential`; an existing `wyrd-client` test fails if removed `ClientConfig::api_key` guidance returns. |

## Verification

Run exact focused tests for every changed scenario through `mise exec -- cargo nextest run --locked` with package, target, and exact test expression, plus the existing TypeScript/Python focused commands selected by their owning tasks. Then run:

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

Record baseline-identical unrelated failures separately; do not weaken, suppress, ignore, or delete any gate or test to obtain a pass.
