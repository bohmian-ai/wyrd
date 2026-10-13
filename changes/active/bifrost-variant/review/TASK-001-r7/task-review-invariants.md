# TASK-001 R7 invariant review

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation evidence:
  `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

The candidate and tree matched the immutable subject before investigation and
immediately before this report was written.

## Navigation and invariant trace

CodeGraph was used first. The cumulative producer-to-sink paths and the R6
corrections were then expanded from source.

| Value or state | Producer / boundary | Shared owner and siblings | Sink / failure transition |
|---|---|---|---|
| JSON open value | `EncodedVariant::{from_json,from_json_text}` | `append_raw`, `scan_numbers`, `raw_number_variant`; buffered row builder and Oracle `parse_json` / `try_parse_json` | Canonical Arrow Variant or the stable size/invalid/numeric/depth error before write ACK or as a query error |
| Already encoded Arrow Variant | Raw Arrow IPC at Scribe admission | `validate_declared_variants` -> `validate_variant_values` -> `EncodedVariant::from_bytes` -> `scan_encoded`; every renderer reuses `EncodedVariant::validate` | Refusal before WAL/ACK, or bounded Arrow/native/JSON rendering after storage |
| Nullable persisted Struct | Verification, metrics, and gateway producers | `DomainTable::WHOLE_STRUCTS`, `refuse_partial_structs`, canonical validator | Whole-present or whole-absent rows only; partial shapes fail before ACK |
| Query result and late failure | Interactive and analytical Oracle execution | `QueryCatalogError`, terminal-frame validation, HTTP/gRPC adapters, Rust/Python/TypeScript collectors | Same catalog problem at the final frame; partial rows discarded by SDK collectors |
| Iceberg v3 row identity | Scribe append and Forge rewrite | pinned Iceberg/compaction forks, five-field handoff, v3 GC | Hidden lineage survives repeated rewrites and remains outside the logical schema |

R6 correctly centralizes raw canonicality in `EncodedVariant::validate`, applies
it before every recursive renderer, and preserves the admission-before-WAL
boundary. The remaining defect is in the JSON producer used by both write
preparation and Oracle.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003: one canonical Variant extension and logical schema/fingerprint contract | `wyrd-queue::variant`, schema conversion, built-in declarations, and Iceberg mapping retain the single `arrow.parquet.variant` owner | Recorded V1/V12 and queue/table suites | PASS |
| REQ-004: JSON and raw Arrow values retain exact cross-language meaning | Exact i64/u64 conversion; raw Decimal4/8, noncanonical Decimal16, and non-finite Float/Double are refused by `scan_encoded` | R6 raw-number unit and pre-ACK journey cases | PASS |
| Locked failure order: size, invalid encoding, numeric range, then depth | `VariantViolations`, `from_json_text`, and `EncodedVariant::validate` select the required classes independently of key order | R6 compound size/numeric/depth and raw canonicality cases | PASS |
| REQ-006–REQ-011 / AC-001 / AC-003: built-ins write typed Struct/Variant schemas | Signal, verification, gateway, audit, eval, and agent-trace table/projection owners | Recorded Rust/OTLP/server/SDK/MCP journeys | PASS |
| Nullable Structs are whole-present or whole-absent | Nullable child declarations plus `WHOLE_STRUCTS` / `refuse_partial_structs` | R6 gateway peer and hot/published query proof; prior verification/metrics proof | PASS |
| REQ-017: every Oracle session installs one semantic Variant SQL surface while Struct keeps `get_field` | Shared `OracleVariantSql` owner and all production session builders | Recorded local, published, and distributed Oracle tests | PASS |
| REQ-019: failures retain catalog identity before ACK and in late terminals; SDKs discard partial rows | Shared admission gate, `QueryCatalogError`, terminal protocol, and collector state transitions | Recorded V9 and Rust/Python/TypeScript/MCP late-failure journeys | PASS |
| INV-002: no silent field loss, narrowing, or retyping | Undeclared/schema checks precede value walks; exact numeric and canonical raw checks converge at shared owners | R6 raw IPC and JSON refusal journeys | PASS |
| INV-004 / INV-006: sensitivity and tenant isolation remain unchanged | Existing projection authorization, physical identity, peer, and tenant tripwires remain on all touched paths | Prior authorization, tenant, and distributed evidence; no R6 bypass found | PASS |
| INV-007: Variant conversion is bounded by value depth/size | Accepted construction stops at depth 64, but `scan_numbers` reparses every remaining subtree below that limit | The 10,000-level focused test passes but took 3.521 seconds in this review; no near-limit adversarial cost proof exists | **FAIL — INV-R7-001** |
| Raw hostile bytes cannot amplify reader work | Object fields receive disjoint slots; duplicate resolved names fail; raw scan is explicit-stack; all renderers validate first | R6 shared/overlapping-object and renderer cases | PASS |
| REQ-001/REQ-002/INV-003: Iceberg v3 creation, lineage-preserving rewrites, GC, and logical invisibility | Catalog, Forge, GC, and pinned fork owners remain unchanged by R6 | Recorded V10/V12/V13 | PASS |
| REQ-005 / AC-009: Bloom capacity follows row-group geometry | Shared writer-properties owner | Recorded V11 | PASS |
| Non-goals remain excluded | No second Variant model/reader, migration, compatibility layer, arbitrary-precision public type, or TASK-002/TASK-003 implementation entered R6 | Cumulative diff and R6 evidence | PASS |
| R6 prior-finding closure: authority, gateway proof, lifecycle status, and canonical raw domain | Bifrost authority update, gateway peer/server tests, R5 status, and shared raw scanner | Recorded R6 focused and broader green commands | PASS except for the bounded-processing regression in `INV-R7-001` |

## Proposed findings

### INV-R7-001 — Deep JSON admission performs quadratic CPU work before returning its bounded error

- **Classification:** INCORRECT
- **Relationship to prior findings:** incomplete root-cause closure of
  `FIND-TASK-001-18`. R6 removed stack exhaustion and correctly lets Wyrd select
  deep numeric/depth errors, but replaced that failure mode with repeated
  whole-subtree parsing on the same reachable input.
- **Violated obligation:** specification revision 13 `INV-007` requires Variant
  conversion to be bounded by the value's depth/size, and the R6 remediation
  requires deep valid JSON to reach Wyrd's decision without library work
  outrunning that bounded decision.
- **Exact locations:**
  `crates/shared/wyrd-queue/src/variant.rs:731-788,810-864`;
  production callers at
  `crates/shared/wyrd-queue/src/batch_builder.rs:383` and
  `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:609`.
- **Evidence:** every container inside the accepted 64 levels is parsed again
  from its complete remaining text by `append_raw`; once depth 65 is reached,
  `scan_numbers` explicitly stacks child `RawValue`s but again calls
  `serde_json::from_str` over each child's complete subtree. For a chain of
  single-item arrays, input suffixes of lengths `n`, `n-2`, `n-4`, and so on
  are rescanned, so work grows quadratically with nesting rather than with the
  8 MiB input bound. The source itself records this as “subtree size times
  depth.” The exact existing 10,000-level test consumed 3.521 seconds of CPU in
  this review; the admitted limit permits orders of magnitude more nesting.
- **Sibling consumers checked:** `batch_builder` sends caller JSON through this
  path before Scribe admission, while Oracle `parse_json` and `try_parse_json`
  execute it in the query process. Raw Arrow validation and rendering use the
  separate `scan_encoded` path and are not affected.
- **Observable consequence:** a syntactically valid, compact, deeply nested
  JSON value within the request and 8 MiB limits can monopolize a server worker
  for time that grows quadratically before returning the expected numeric or
  depth error, making write admission and Oracle query execution susceptible to
  CPU denial of service.
- **Required testable correction:** keep `EncodedVariant` and `serde_json` as
  the sole owners, but consume each JSON token/subtree once while collecting
  the first numeric and depth violations and building only accepted members;
  reuse the already-resolved bounded-stack mechanism if the selected serde
  traversal is recursive, and do not add a second semantic model, error path,
  or parser. Preserve syntax authority, last-key-wins normalization, exact
  number tokens, size-before-numeric-before-depth selection, and
  `try_parse_json`'s invalid-only null behavior. Add one focused adversarial
  depth-scaling check that would fail the current repeated-rescan path, plus
  retain the existing 64/65/128/129/10,000 classification cases and the
  buffered-write and Oracle boundary tests.

## Verification notes

- Independently inspected the cumulative base-to-candidate source, all R6
  commits, the approved revision-13 contracts, prior r1-r6 verdict/remediation
  history, installed `parquet-variant` behavior, and the recorded R6 command
  evidence.
- Re-ran
  `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::json_depth_is_decided_by_wyrd_at_any_depth)'`:
  1 passed in 3.521 seconds.
- Did not rerun the database, SDK, MCP, codegen, or full repository lanes; their
  recorded R6 results cover behavior unchanged by the retained finding.

## Overall result

**FAIL** — `INV-R7-001` leaves a reachable untrusted JSON path with quadratic
CPU growth, so the revision-13 bounded-conversion invariant is not satisfied.
