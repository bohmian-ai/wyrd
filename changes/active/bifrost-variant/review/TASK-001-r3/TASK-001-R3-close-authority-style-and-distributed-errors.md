---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-bifrost-variant
spec_revision: 11
parent_task: TASK-001
remediates: [FIND-TASK-001-4, FIND-TASK-001-10, FIND-TASK-001-12]
---

# Close TASK-001 authority, Rust-style, and distributed-error gaps

## Authority and immutable review subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Review evidence: `changes/active/bifrost-variant/review/TASK-001-r3/findings-validation.md`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Reviewed candidate: `555308ba14058ddc56102d2f925298ef43858175`

## Outcome

TASK-001's active authority states the shipped revision-11 contracts, its
cumulative Rust declarations satisfy the repository import rules, and every
catalogued Analytical worker failure reaches the existing late terminal as its
complete derive-backed problem without family-specific decoding or prose
classification.

## Issue diagnoses and required corrections

### FIND-TASK-001-4 — Active authority contradicts revision 11

`architecture/bifrost-design.md:144-152` still accepts any integral token that
fits a Variant decimal, while revision 11 and
`EncodedVariant::from_json_text` accept signed `i64`, then unsigned `u64` as a
scale-zero decimal, and refuse every other integral token. Its Variant SQL and
terminal sections at `:439-465` and `:698-715` also stop short of the revision-11
rule that every catalogued late failure carries the same complete problem over
Interactive and Analytical HTTP/gRPC, every SDK raises it unchanged, and
collection returns no partial result.

The active Bifrost design is the winning subsystem authority. Leaving it stale
makes the superseded wider-number and family-specific late-error contracts
appear valid even though executable source implements narrower behavior.

Update only those existing paragraphs. State the exact `i64`/`u64` boundary,
the out-of-range refusal, the complete derive-backed late problem for every
catalogued failure, unchanged SDK projection, no partial collected result, and
the generic fallback only for uncatalogued failures. Reuse the existing
authority and guide vocabulary. Add no document, generator, checker, setting,
option, compatibility path, or alternate error mechanism.

### FIND-TASK-001-10 — Prior import-rule remediation is incomplete

The cumulative changed Rust surface still contains ordinary function-local
`DataFusionError` imports in
`crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:1654,1678,1714`.
It also spells `serde::Serialize`, `serde_json::Value`,
`wyrd_spec::error::WyrdProblem`, and `std::fmt::Display` as qualified names in
changed fields, signatures, return types, iterator items, and bounds in:

- `crates/shared/wyrd-client/src/observe/eval.rs:188`;
- `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:2701-2713`;
- `crates/vala/vala-bifrost-redux/src/tables/signal.rs:1145`;
- `crates/wyrd/wyrd-server/src/query/service.rs:372`;
- `crates/wyrd/wyrd-server/src/verification/results.rs:593-595,758-761,947`;
- `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:1302-1307`.

These declarations are reachable production or journey surfaces, and R1's
stable finding explicitly required the cumulative diff to contain no such
site. Move the ordinary imports into each existing owning module or test-module
import block and use bare names in the listed declarations. Preserve the
documented local `Trait as _` exception. Change no behavior and add no lint,
checker, wrapper, script, or allow attribute.

### FIND-TASK-001-12 — Distributed identity is still Variant-specific and prose-dependent

The public terminal, protobuf conversion, and shared client can carry a full
`WyrdProblem`, but the Analytical worker-to-coordinator boundary produces the
wrong input for them. `oracle/variant_sql.rs:635-695` wraps and decodes exactly
four Variant errors through serialized `Display` text.
`oracle/mod.rs:4240-4294` applies that decoder generally and recognizes sibling
catalog failures through message fragments. The pinned distributed transport
reduces `DataFusionError::External` to a string, so worker-side catalog errors
outside the four Variant variants have no general structured reconstruction.
The candidate's focused test even requires a serialized `QueryForbidden` to
become generic. A reachable worker footer refusal regains
`QueryTenantInvariant` only through the phrase `tenant invariant`.

Delete the Variant-only carrier/decoder and the catalog message heuristics.
Reuse the existing tagged `BifrostError` serde representation and one general
catalog-error envelope at the existing `DataFusionError::External`
worker boundary. Reconstruct that envelope before the existing
`map_datafusion_error` and `failed_terminal_on_path` flow. This keeps the
installed transport, terminal, catalog owner, and SDK projections; uncatalogued
external failures alone map to `QueryExecutionFailed`.

Do not repin or modify the dependency protocol, add a protobuf field or side
channel, maintain a closed error-code list, add a Variant branch, parse human
wording, or create a new public API, dependency, setting, compatibility path,
or test harness.

## Constraints and preserved behavior

- Preserve standard Iceberg v3 lineage only. Add no duplicate-ID scan or
  optional-metrics gate in production or tests.
- Keep `serde_json/arbitrary_precision` disabled and refuse integers outside
  `i64`/`u64`.
- Preserve Variant limits, fingerprints, admission ordering, built-in shapes,
  promotion semantics, sensitivity, tenant tripwires, audit hash inputs, and
  fixed trace identifiers.
- Preserve one shared Oracle session registration owner, Struct `get_field`,
  semantic Variant `variant_get`, and the existing terminal/protobuf/client
  full-problem path.
- Preserve the accepted Python/TypeScript Interactive-only journey limit;
  distributed proof remains in the Rust multi-pod journey through the shared
  client.
- Keep `wyrd-spec` IO-free and PyO3-free and keep DataFusion, Parquet, and
  Iceberg out of client-tier crates.
- Add no shredding, user-table authoring, migration, compatibility alias,
  second Variant model/reader, unrelated refactor, repository check, setting,
  option, or replacement lineage proof.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-4` | The active Bifrost authority states the exact `i64`/`u64` integer policy and the universal full-problem late-terminal/no-partial-result contract without a new artifact or mechanism. |
| `FIND-TASK-001-10` | Every listed cumulative changed declaration uses module-scope imports and bare names; the only retained local imports match the documented `Trait as _` exception. |
| `FIND-TASK-001-12` | A Variant catalog error and a non-Variant worker `BifrostError` round-trip through one general distributed envelope into identical pre-stream and late catalog problems; malformed or uncatalogued external text stays generic; no message parser or family-specific branch remains. |
| `FIND-TASK-001-12` | The existing Rust multi-pod journey produces at least one valid worker batch before a worker-side `QueryTenantInvariant`, receives the same complete problem as the pre-stream form, returns no partial collected result, and settles graph ownership; the unrelated late cast remains generic. |

## Focused proof and broader verification

Use the existing test owners and harnesses only:

1. Replace the Variant-only forwarded-text assertion with a general structured
   round-trip covering one Variant error, one non-Variant `BifrostError`,
   malformed or uncatalogued text, and nested DataFusion context. Run its exact
   `mise exec -- cargo nextest` selector.
2. Extend the existing Rust multi-pod late-failure journey with its established
   foreign- or missing-footer fixture. Prove a valid batch precedes
   `QueryTenantInvariant`, compare the late problem with the pre-stream problem,
   assert no partial result, and retain the generic cast case. Run its exact
   repository-managed Postgres selector.
3. Reinspect the complete cumulative Rust diff for ordinary function-local
   imports and qualified declaration types.
4. Run `mise run fmt`, `mise run lints`, `mise run docs:check`,
   `mise run check:docs`, the task's existing focused Oracle/tonic/client
   late-terminal tests, `mise run codegen:check`, and `git diff --check`.
5. Rerun the narrowest existing Bifrost journey lanes that cover the changed
   Oracle distributed path and shared client. Do not add or require a separate
   Python/TypeScript Analytical harness.

Route this task directly to `$wyrd-implement`.
