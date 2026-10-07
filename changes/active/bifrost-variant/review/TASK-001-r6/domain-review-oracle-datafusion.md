# Oracle / DataFusion / public-error domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved authority: `SPEC-bifrost-variant` revision 13 and cumulative
  `TASK-001`, including review/remediation rounds r1-r5
- Evidence packet: `TASK-001-R5-close-remaining-variant-contract-gaps.md`

The checkout matched the candidate and tree at the start and immediately before
this report was written.

## Reviewed boundary and authority coverage

| Boundary | Authority and source coverage | Result |
|---|---|---|
| One Variant SQL surface in every production Oracle session | Spec REQ-017/REQ-019, Bifrost and DataFusion authorities; `oracle/variant_sql.rs`, leader planning in `oracle/mod.rs`, admitted/follower state through `resources.rs` and `oracle/follower.rs`, and analytical planning/leader/worker construction in `oracle/analytical.rs` | PASS |
| `variant_get`, `->>`, `parse_json`, `try_parse_json`, and `to_json` semantics | `oracle/variant_sql.rs`; shared encoding and rendering in `wyrd-queue/src/variant.rs`; the complete focused tests and recorded r5 evidence | **FAIL — ODF-R6-001** for raw duplicate object keys; JSON parsing, numeric/depth selection, and ordinary rendering otherwise pass |
| Numeric-range versus depth selection | Revision-13 locked precedence; `EncodedVariant::{from_json_text,from_bytes}`, `VariantViolations::finish`, `append_raw`, `scan_numbers`, `scan_encoded`; Oracle strict-parser mapping and the r5 unit/journey evidence | PASS |
| Local and distributed catalog identity | `QueryCatalogError`, `map_datafusion_error`, analytical worker construction, late stream mapping, full-problem terminal, tonic conversion, and shared-client reconstruction | PASS |
| Failed terminal and partial-row behavior | `oracle/query_stream.rs`, `failed_terminal_on_path`, `wyrd-tonic/query_conversion.rs`, and `wyrd-client/bifrost/query.rs`; r5 four-node Oracle journey evidence | PASS |

### Session and public-error trace

- `OracleVariantSql::install` remains the one owner of the five UDFs and the
  expression planner. Leader planning installs it before provider setup;
  admitted Interactive and follower sessions use
  `OracleExecution::session_state`; analytical leader and worker sessions
  install it directly; the derived analytical planning session uses
  `new_from_existing` and retains the same registry. The physical-only source
  resolution state copies the task's registered scalar functions rather than
  creating another SQL surface.
- Literal Variant paths use Arrow-rs `variant_get`, dynamic paths evaluate from
  the full root, and results are normalized to canonical unshredded extension
  storage. `->>` and `to_json` render through the installed Variant JSON
  conversion. Non-Variant operands stay on DataFusion's `get_field` path.
- JSON `parse_json` and raw Arrow admission select size first, then malformed
  encoding, numeric range, and depth. The r5 remediation's non-building numeric
  scan covers numbers below the depth boundary, and the raw explicit-stack scan
  applies the same selector. `try_parse_json` suppresses only invalid JSON;
  numeric, depth, and size failures remain typed.
- Catalogued failures enter DataFusion through `QueryCatalogError::external`.
  Local source chains retain the typed error; the distributed text-only hop
  carries its tagged serde form. `map_datafusion_error` reconstructs the same
  catalog error, while unidentified failures remain
  `QueryExecutionFailed`. Late errors become the same full `WyrdProblem`
  terminal as early errors. The client accepts a failed terminal only after
  clean EOF and returns it as an error, never the batches accumulated before
  it.

## Material finding

### ODF-R6-001 — INCORRECT — raw Variant objects may retain duplicate keys and silently lose a value at query/render time

- **Violated obligation:** Revision-13 REQ-004 requires every Variant object to
  have unique keys, INV-002 forbids silent field loss, and the raw Arrow trust
  boundary must refuse values outside that domain before ACK. `variant_get`,
  `->>`, and `to_json` must therefore never receive an admitted object with two
  values for one key.
- **Exact location:**
  `crates/shared/wyrd-queue/src/variant.rs:921-936`, reached by
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:395-415`; observable
  consumers include
  `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:245-280,432-519`.
- **Evidence:** `scan_encoded` requires strict increase (`name <= previous`)
  only when the metadata dictionary advertises itself as sorted. For an
  unsorted dictionary it rejects only `name < previous`, so adjacent equal
  object names pass. The pinned `parquet-variant` full validator has the same
  gap at `variant/object.rs:256-277`, so the later `Variant::try_new` call does
  not close it. This is reachable with canonical extension storage: use an
  ordinary valid two-field object's value bytes (field ids `0, 1`) with the
  dependency's accepted unsorted metadata dictionary whose first two entries
  are both `"a"`. Both the candidate scan and upstream full validation accept
  the object as fields `a, a`.
- **Producer-to-consumer trace:** Raw Arrow IPC passes extension/schema checks,
  then `validate_variant_values` calls `EncodedVariant::from_bytes`. Successful
  construction permits WAL/ACK and publication. Oracle `variant_get` performs
  a binary search among the duplicate names and can expose only one matching
  value. Both `variant_as_text` and `to_json` call upstream
  `to_json_value`; its object branch collects pairs into a
  `serde_json::Map`, where the later equal key replaces the earlier value.
  Thus a successfully acknowledged raw value loses one supplied field/value in
  public query and JSON-rendering terminals.
- **Why existing proof misses it:** The r5 raw validation cases cover malformed
  descendants, numeric-domain violations, excessive depth, and shared child
  offsets. JSON duplicate-key coverage correctly proves last-key-wins during
  JSON authoring, but no raw encoded object uses an unsorted dictionary with
  two field ids resolving to the same name.
- **Smallest testable correction:** In the existing `scan_encoded` owner,
  require object names to be strictly increasing regardless of the metadata
  sorted flag; equality is invalid in both branches. Do not add a second
  validator or normalize/drop a raw field. Add a direct `from_bytes` regression
  using a valid two-field value plus duplicate-name unsorted metadata and expect
  `VariantInvalidJson` at the object path. Extend the existing raw-IPC
  pre-ACK journey to assert the exact catalog problem, no retained row, and a
  successful following valid write. Retain the current JSON last-key-wins
  test, which is a separate authoring rule that produces one unique stored
  key.

## Verification evidence and limits

- Source review began with CodeGraph and expanded through every production
  session constructor, the UDF implementations, both Variant input walkers,
  raw/JSON rendering, DataFusion local/distributed error mapping, terminal
  conversion, and shared-client EOF/partial-row handling.
- The r5 packet records the focused Variant tests, Oracle UDF tests, the
  Postgres-backed four-node session journey, SDK/MCP journeys, format, lints,
  codegen, docs, and diff checks as passing. Those results are credible for the
  paths they exercise but contain no duplicate-name raw object case.
- Per shared-checkout coordination, this reviewer ran no Cargo, mise, codegen,
  or environment-owning test command. The finding is established from the
  reachable admission and dependency source paths; its closure proof remains
  unexecuted.
- TASK-003's shredded physical projection and pruning remain outside this
  TASK-001 domain slice.

## Overall result

**FAIL** — session parity, numeric/depth selection, structured local and
distributed error identity, and failed-terminal partial-row rejection remain
sound, but ODF-R6-001 permits a raw Variant state forbidden by revision 13 and
silently changes its observable query/rendered value.
