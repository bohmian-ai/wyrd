# TASK-001 r3 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior remediation tasks: `review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md` and `review/TASK-001-r2/TASK-001-R2-exact-integers-and-late-errors.md`
- Candidate stability: `HEAD` resolved to the candidate before validation.
- CodeGraph: unavailable because the repository has no `.codegraph/` directory. Validation used the cumulative Git diff, direct source and caller tracing, repository search, and the pinned dependency checkout.

## Validation coverage

This pass read every required discovery report and the focused follow-up in
`TASK-001-r3`, then independently inspected the cited owners, full function
bodies, callers, sibling consumers, tests, applicable authority, original task,
both remediation tasks, and the complete cumulative change inventory. It also
read the pinned `datafusion-distributed` error conversion at revision
`4cfa166`, where `DataFusionError::External` is serialized as its `Display`
string and reconstructed as a generic external error.

The binding human decisions control this ledger:

- Iceberg lineage relies only on standard Iceberg v3 behavior. No duplicate-ID
  scan or optional-metrics gate is required in production or proof code.
- `serde_json/arbitrary_precision` stays disabled, and integers outside the
  `i64`/`u64` union are refused.
- Every late failure with catalog identity carries its complete catalog problem;
  only an uncatalogued failure becomes `QueryExecutionFailed`.
- Python and TypeScript Interactive-only late-failure journeys are accepted;
  the Rust multi-pod journey owns distributed proof through the shared client.
- A mechanism, check, file, setting, or option absent both established standards
  and comparable widely used projects is `DRIFT` and is not a remediation
  requirement.

## Proposed-finding disposition

| Proposed source ID | Result | Stable finding | Validation |
|---|---|---|---|
| `INV-R3-001` | **CONFIRMED** | `FIND-TASK-001-4` | The active authority contradicts revision 11's numeric range and omits its all-errors late-terminal contract. |
| `MAINT-001`, `REPO-R3-1`, `FUP-002` | **CONFIRMED** | `FIND-TASK-001-4` | These are the same authority-parity failure; the follow-up correctly adds the omitted late-terminal paragraph to the correction boundary. |
| `MAINT-002`, `REPO-R3-2` | **REVISED** | `FIND-TASK-001-10` | The rule violation is real, but both reports list only part of the cumulative changed declarations. The validated ledger uses the complete source list from direct inspection and `FUP-003`. |
| `FUP-003` | **CONFIRMED** | `FIND-TASK-001-10` | The cited local imports and qualified signature, field, return, and bound names are present in changed source and violate the explicit prior closure criterion. |
| `SYS-001`, `ODF-001` | **REVISED** | `FIND-TASK-001-12` | The Variant-only carrier and message heuristics violate the universal contract. Their specific worker `QueryTimeout` example is not established by the inspected Analytical worker path; the reachable worker footer refusal proves the defect without that example. |
| `FUP-001` | **CONFIRMED** | `FIND-TASK-001-12` | The follow-up correctly narrows reachability to a worker `QueryTenantInvariant` and the candidate's own test proves other serialized catalog errors are intentionally discarded. |

No new stable finding ID is needed. These are closure failures of prior
`FIND-TASK-001-4`, `FIND-TASK-001-10`, and `FIND-TASK-001-12`, not new
obligations.

## Empty-report validation

- The Iceberg durability report's empty ledger is **CONFIRMED**. The pinned v3
  reader/writer and Forge proofs retain field-ID projection, per-batch
  presence/type/null validation, unchanged copy, repeated rewrite, and
  maintenance coverage. The rejected duplicate-ID and optional-metrics gates
  are absent. No replacement lineage mechanism, check, option, or setting was
  added.
- The Variant/Arrow report's empty ledger is **CONFIRMED**. Raw-token
  classification accepts only `i64` then `u64`, the server trust boundary
  validates declared Variant values before ACK/WAL, and no second Variant model
  or `arbitrary_precision` feature was introduced.
- The security/tenancy report's empty security ledger is **CONFIRMED** as to
  authorization, tenant isolation, admission, audit, and exposure. Its general
  statement that remote catalog identities are typed is not evidence against
  `FIND-TASK-001-12`: the remote footer refusal currently regains identity by
  message text, which is the retained contract/drift defect.
- The behavior and SDK-parity reports' empty ledgers are **REJECTED** as overall
  acceptance conclusions. Both state that no Variant-only/prose recovery
  remains, while `VariantQueryError::decode` accepts exactly four Variant
  variants and `map_datafusion_error` still classifies sibling catalog failures
  from message fragments. The behavior report also accepts architecture parity
  contradicted by the current authority text.
- Direct inspection found no additional material finding in the remaining
  changed storage, Variant, SDK, tenancy, recovery, generated-contract, or
  language-projection surfaces. In particular, protobuf reserved fields are
  standard evolution practice, the installed `serde_json/raw_value` feature is
  the approved lexical mechanism, and the pinned Iceberg forks are approved
  narrow owners rather than new public settings or compatibility paths.

## Final validated ledger

### FIND-TASK-001-4 — Active Bifrost authority contradicts the revision-11 numeric contract and omits the complete late-error contract

- **Discovery sources:** `INV-R3-001`, `MAINT-001`, `REPO-R3-1`, `FUP-002`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** `AGENTS.md` makes
  `architecture/bifrost-design.md` the active Bifrost authority, and the
  original task plus prior `FIND-TASK-001-4` require that authority to match the
  shipped contract. Revision 11 REQ-004 accepts only `i64` and `u64` integral
  tokens. REQ-019 requires every catalogued late failure to carry the same full
  problem in Interactive and Analytical HTTP/gRPC execution, with SDK
  collection refusing partial results.
- **Exact location:** `architecture/bifrost-design.md:144-152,439-465,698-715`.
- **Evidence and producer-to-consumer trace:** The authority says an integer
  outside signed 64-bit is accepted whenever it fits a Variant decimal and
  says any other number becomes a double. The shared producer
  `EncodedVariant::from_json_text` instead parses `i64`, then `u64`, then emits
  `NumericOutOfRange`; the existing schema guide states that narrower behavior.
  On queries, `QueryTerminalFrame`, protobuf field 9, tonic conversion,
  `failed_terminal_on_path`, shared-client reconstruction, MCP, and scheduled
  collection implement a full `WyrdProblem` terminal. The authority's Variant
  SQL paragraph promises only Variant code/detail stability, while its terminal
  paragraph lists closed outcomes without stating the all-catalog-error,
  unchanged-SDK, no-partial-result rule. A maintainer following the winning
  authority can therefore reintroduce either superseded contract.
- **Smallest safe correction:** Update only those existing authority paragraphs.
  State that signed `i64` values remain integers, values from `i64::MAX + 1`
  through `u64::MAX` become scale-zero decimals, and every other integral token
  is refused. State that every late catalog failure carries the same full
  derive-backed problem as its pre-stream form across Interactive/Analytical
  HTTP/gRPC, every SDK raises it unchanged, collection returns no partial
  result, and only uncatalogued failures become
  `WYRD_VALA_500_QUERY_EXECUTION_FAILED`. Add no file, generator, checker,
  setting, option, compatibility path, or alternate error mechanism.
- **Focused closure proof:** Compare the corrected paragraphs to REQ-004 and
  REQ-019, then run `mise run docs:check`, `mise run check:docs`, and
  `git diff --check`.

### FIND-TASK-001-10 — The cumulative Rust diff still contains disallowed local imports and qualified declaration types

- **Discovery sources:** `MAINT-002`, `REPO-R3-2`, `FUP-003`
- **Status:** REVISED
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` requires ordinary
  imports at module scope and imported bare names in fields, signatures,
  return types, trait bounds, and `where` clauses. R1's stable
  `FIND-TASK-001-10` explicitly required the cumulative diff to contain no such
  site, except the documented local `Trait as _` case.
- **Exact locations:**
  - `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:1654,1678,1714`
  - `crates/shared/wyrd-client/src/observe/eval.rs:188`
  - `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2701-2713`
  - `crates/vala/vala-bifrost-redux/src/tables/signal.rs:1145`
  - `crates/wyrd/wyrd-server/src/query/service.rs:372`
  - `crates/wyrd/wyrd-server/src/verification/results.rs:593-595,758-761,947`
  - `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1302-1307`
- **Evidence and caller trace:** Three changed query-stream tests import
  `DataFusionError` inside individual functions even though the enclosing test
  module has an import block. Changed declarations spell `serde::Serialize`,
  `serde_json::Value`, `wyrd_spec::error::WyrdProblem`, and
  `std::fmt::Display` in bounds, fields, parameters, iterator items, and return
  types. These declarations are live: `json_value` serves Eval row projection;
  `VariantSpanRow` is the Rust SDK journey's deserialization target; `json_of`
  and `variant_cell` decode Variant proof values; `terminal_error` is called by
  MCP and scheduled queries; `EvalItem` and `variants` build persisted verifier
  results; and `prove_late_failures` is the multi-pod late-error journey. The
  local `use std::fmt::Write as _` elsewhere is the allowed trait-enablement
  exception and is excluded.
- **Smallest safe correction:** Move `DataFusionError` to the existing
  query-stream test-module import block. Import `Serialize`, `Value`,
  `WyrdProblem`, and `Display` at each owning module or test-module top and use
  those bare names at the listed declarations. Change no behavior and add no
  lint, checker, script, allow, or wrapper.
- **Focused closure proof:** Reinspect the cumulative Rust diff for ordinary
  function-local imports and qualified field/signature/return/bound names, then
  run `mise run fmt` and `mise run lints`.

### FIND-TASK-001-12 — Analytical worker catalog errors still use a Variant-only serialized-text carrier and prose heuristics

- **Discovery sources:** `SYS-001`, `ODF-001`, `FUP-001`
- **Status:** REVISED
- **Classification:** VIOLATION / DRIFT
- **Violated obligation:** Revision 11 REQ-019 and R2
  `FIND-TASK-001-12` require the full existing catalog problem for every late
  failure in Interactive and distributed execution and expressly prohibit
  prose parsing or a Variant-only branch. The standing direction rejects the
  bespoke family-specific carrier where the existing general catalog/error
  owner and ordinary structured remote-error pattern cover the requirement.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:635-695`;
  `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4240-4294`;
  pinned `datafusion-distributed` revision `4cfa166`,
  `src/protocol/grpc/errors/datafusion_error.rs:86-183,185-310`.
- **Evidence and producer-to-consumer trace:** `parse_json` wraps its four
  catalog failures in `VariantQueryError`; that wrapper serializes
  `BifrostError` into `Display`, and `decode` permits only the four Variant
  variants. The pinned worker transport reduces every external error to that
  string and reconstructs a generic external source on the coordinator.
  `catalog_query_error` therefore restores only Variant errors. Sibling worker
  failures do not use the wrapper: `OracleIcebergScanExec::start_stream` and
  `execute` raise typed `QueryTenantInvariant` errors, which the same transport
  erases; `is_tenant_refusal` then regains that identity only by matching the
  phrase `"tenant invariant"`. Reconciliation and audit failures use analogous
  message fragments. The candidate's own
  `variant_errors_keep_their_catalog_identity_locally_and_remotely` test
  serializes `QueryForbidden` and requires it to become
  `QueryExecutionFailed`, directly falsifying the all-errors claim. The real V9
  journey proves only the special Variant carrier and an uncatalogued generic
  cast error. Downstream terminal, protobuf, and shared-client code correctly
  carries whichever problem Oracle selects; the defect is upstream at the
  worker/coordinator boundary.
- **Observable consequence:** Distributed catalog identity depends on error
  family and wording. A worker footer refusal is currently one wording change
  away from becoming `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, and any catalogued
  worker error outside the four Variant variants has no general reconstruction
  path. The discovery reports' `QueryTimeout` example is not retained because
  the inspected Analytical worker path does not establish that producer.
- **Smallest safe correction:** Delete `VariantQueryError`, its closed decoder,
  and catalog/message-fragment reconstruction. Reuse `BifrostError`'s existing
  tagged serde representation and one general catalog-error wrapper at the
  existing `DataFusionError::External` worker boundary for every worker-side
  catalog error; reconstruct that general wrapper before the existing
  `map_datafusion_error`/`failed_terminal_on_path` path. Uncatalogued external
  failures alone remain `QueryExecutionFailed`. This is the standard typed
  error-envelope pattern over the already-installed transport, not a new public
  API, side channel, code list, protocol field, dependency repin, setting,
  option, compatibility path, or per-error parser.
- **Focused closure proof:** Replace the synthetic Variant-only forwarded-text
  assertion with a general round-trip covering a Variant error, a non-Variant
  `BifrostError`, malformed/uncatalogued text, and nested DataFusion context.
  Extend the existing Rust multi-pod journey using its established foreign- or
  missing-footer fixture so one valid worker batch precedes a
  `QueryTenantInvariant`; assert the late problem equals its pre-stream catalog
  form, graph ownership settles, and collection returns no partial result.
  Retain the uncatalogued late cast case as the generic fallback. Add no new
  test hook or harness.

## Prior-finding closure

| Stable finding | Validation result |
|---|---|
| `FIND-TASK-001-1`, `FIND-TASK-001-11` | Closed: raw-token classification accepts exact `i64`/`u64` only, refuses both adjacent out-of-range values, and `arbitrary_precision` remains off. |
| `FIND-TASK-001-2` | Closed: the shared server boundary recursively validates built-in Variant identity and bytes before ACK/WAL with catalogued errors. |
| `FIND-TASK-001-3` | Closed under the binding decision: production and required proofs use standard Iceberg v3 lineage handling without duplicate-ID or optional-metrics gates. |
| `FIND-TASK-001-4` | **Not closed:** revision-11 numeric and complete late-terminal behavior is absent or contradictory in the active authority. |
| `FIND-TASK-001-5` | Closed for its revision-10 Variant-specific obligation: remote Variant identity no longer depends on human prose. Revision 11's broader all-errors failure is retained under `FIND-TASK-001-12`. |
| `FIND-TASK-001-6` through `FIND-TASK-001-9` | Closed: validated construction, TypeScript documentation, native Bloom geometry, and required Rust documentation remain present. |
| `FIND-TASK-001-10` | **Not closed:** the cumulative changed Rust declarations still contain the listed prohibited forms. |
| `FIND-TASK-001-12` | **Not closed:** the public terminal carries a full problem, but the distributed producer still restores only Variant catalog identities structurally and classifies siblings by text. |

## Specification-decision assessment

No retained correction requires a specification revision. Revision 11 already
decides the numeric and all-errors terminal behavior; repository rules already
decide import placement; the existing authority file, catalog error owner,
serde representation, distributed external-error boundary, terminal, and
multi-pod harness are sufficient. The corrections require no new product,
public API, architecture, security, compatibility, concurrency, resource-
ownership, or persistent-data decision.

## Validation result

**VALIDATED — 3 retained findings:** `FIND-TASK-001-4`,
`FIND-TASK-001-10`, and `FIND-TASK-001-12`. Required reports, source, caller
paths, authorities, and pinned dependency evidence were available; no
disagreement remains unresolved.
