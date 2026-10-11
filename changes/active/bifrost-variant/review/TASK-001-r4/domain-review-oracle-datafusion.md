# Oracle / DataFusion / query transport domain review

## Immutable subject and reviewed boundary

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Approved authority: `SPEC-bifrost-variant` revision 11 and original `TASK-001`
- Boundary: the one Oracle Variant SQL registration across planning, admission,
  leader, analytical, and worker sessions; physical-plan codec binding; Variant
  operators/functions and placeholder/malformed-byte handling; catalog error
  preservation through local and distributed DataFusion execution; worker
  tenant refusal; late-failure terminal construction; HTTP/gRPC conversion;
  client reconstruction and rejection of partial results.

The checkout is one later documentation-only commit beyond the candidate.
`git diff --name-only a6429060f..HEAD` contains only the two
`wyrd-implement/SKILL.md` copies, so all runtime source and tests inspected on
disk are byte-identical to the immutable candidate. Candidate-specific reads
also used `git show a6429060f:<path>`.

## Authority coverage

| Concern | Governing authority | Result and source evidence |
|---|---|---|
| One registration owner in every production session, installed before planning/provider setup/codec decode/execution | `spec.md` Oracle registration contract; TASK-001 scenario 2; `architecture/bifrost-design.md` Variant SQL surface | PASS. `OracleVariantSql` owns the five UDFs and expression planner (`oracle/variant_sql.rs:75-134`). Planning installs it before `register_cut_providers` (`oracle/mod.rs:3094-3122`); admitted execution uses `OracleExecution::session_state` (`resources.rs:4235-4255`); analytical leader and worker install it (`oracle/analytical.rs:492-543`, `7622-7693`). The derived planning session uses `new_from_existing`, retaining the installed registry (`oracle/analytical.rs:7543-7583`). |
| Peer codec rejects a differing Variant SQL contract before decode | `spec.md:290-298`; TASK-001 scenario 2 | PASS. `ORACLE_VARIANT_SQL_VERSION == 1` and the physical-plan fingerprint binds it with codec version and bytes (`oracle/variant_sql.rs:53-59`; `oracle/codec.rs:20-37`). The pinned distributed dependency serializes `DataFusionError::External` as its inner display text and reconstructs it as `External`, matching the candidate envelope design. |
| `->`, `->>`, `parse_json`, `try_parse_json`, `to_json`; Struct remains exact `get_field` | REQ-017/REQ-019; locked semantic Variant access; AC-005 | PASS. The planner rewrites only when the left field carries the Variant extension and otherwise returns the original expression (`oracle/variant_sql.rs:150-183`). Literal chains collapse into one Arrow-rs `variant_get`; `->>` wraps `variant_as_text` (`:171-180`, `:245-280`). Strict parse emits the exact typed violation while lenient parse maps only invalid JSON to null (`:592-621`). `to_json` and text conversion decode the same Variant array (`:432-520`). The focused contract test asserts both `variant_get(...)` and native `get_field(...)` plan shapes (`:740-830`). |
| Null-parent placeholders and malformed stored bytes | INV-002; Arrow analytical interop fail-closed rule; TASK-001 diagnosis | PASS. Every Variant-consuming UDF routes through `mask_placeholders` (`oracle/variant_sql.rs:251-280`, `373-381`, `432-520`). The shared helper masks only the known empty-child placeholder and fully validates every other present cell before upstream decoding (`wyrd-queue/src/variant.rs:336-388`). Malformed-byte coverage executes `->`, dynamic `->`, `->>`, and `to_json` and requires an error rather than a panic (`oracle/variant_sql.rs:871-929`). |
| Exact catalog error identity through DataFusion and the distributed worker boundary | Revision-11 late-error contract; `architecture/bifrost-design.md` read terminal contract | PASS. Catalogued failures enter DataFusion only through `QueryCatalogError::external`; `find` walks local sources or decodes the distributed external text (`oracle/mod.rs:4241-4308`). The pinned peer codec serializes an external error using the inner error's `Display`, so the tagged serde envelope is not prefixed or altered. Footer tenant refusals and live/timeout refusals use that owner (`oracle/exec.rs:1205-1223`, `1530-1536`, `3134-3154`; `oracle/live.rs:630-644`; `oracle/bindings.rs:190-285`). `map_datafusion_error` gives typed resource exhaustion precedence, otherwise preserves the catalog error and maps only unidentified failures to `QueryExecutionFailed` (`oracle/mod.rs:4233-4247`). |
| Worker tenant refusal and fail-closed row behavior | INV-006; Bifrost footer tenant authority; terminal contract | PASS. Published/hot footer verification runs before row-group decode and converts `QueryTenantInvariant` into the catalog envelope (`oracle/exec.rs:1152-1161`, `1226-1247`, `3121-3154`). The four-node journey forces an Analytical graph, proves a follower graph lease advanced, requires the exact tenant-invariant problem, returns no result, and waits for every Oracle to return to baseline (`wyrd-testing/tests/bifrost/oracle/published.rs:1377-1450`). |
| Local/distributed late failures, terminal conversion, and no partial successful result | Revision 11; `architecture/bifrost-design.md:712-728`; Arrow terminal rule | PASS. Every post-schema batch error is mapped once and becomes a failed terminal carrying the full catalog problem and emitted-row count (`oracle/query_stream.rs:455-525`; `oracle/mod.rs:3675-3700`). HTTP and gRPC both encode the same domain terminal through `proto::QueryStreamFrame::from` (`wyrd-server/src/query/routes.rs:66-79`, `435-462`; `wyrd-server/src/grpc/query.rs:24-52`). Protobuf decoding parses the problem bytes and validates outcome, row count, ordering, and EOS (`wyrd-tonic/src/query_conversion.rs:330-378`, `456-501`, `580-588`). The client retains a failed terminal only after clean EOF, reconstructs its catalog error through the shared problem mapper, and `collect_bounded` returns the error without its accumulated batches (`wyrd-client/src/bifrost/query.rs:111-149`, `642-773`, `775-825`). |

## Source and caller coverage

- Used CodeGraph first to locate `map_datafusion_error`, Oracle session owners,
  analytical workers, codec paths, and public transport consumers; expanded the
  result against the complete base-to-candidate diff and candidate source.
- Traced session creation through `Oracle::build_physical_root`,
  `AdmittedQueryGuard::execution_session`, `OracleExecution::session_state`,
  `AnalyticalExecutionHandle::{planning_session,leader_session}`, and
  `AnalyticalSessionBuilder::build_session_state`.
- Traced typed failures from `parse_json`, footer tenant proof, live dispatch,
  and timeout producers through `DataFusionError`, the pinned distributed peer
  protobuf, `map_datafusion_error`, the terminal owner, both server transports,
  `QueryStreamConverter`, `from_problem`, and bounded collection.
- Inspected the relevant focused unit and Postgres-backed journey tests,
  including session coverage, worker tenant refusal, early/late Variant errors,
  unrelated generic late failure, HTTP/gRPC problem round-trip, and partial-row
  discard.

## Verification and limits

Focused checks run during this review, all passing:

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract) | test(=oracle::variant_sql::tests::parse_json_keeps_exact_integers_and_refuses_the_rest) | test(=oracle::variant_sql::tests::malformed_stored_variant_fails_every_function_without_panicking)'` — 3 passed.
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support -E 'test(=oracle::query_stream::tests::late_catalog_error_keeps_its_identity)'` — 1 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-tonic --lib -E 'test(=query_conversion::tests::failed_terminal_problem_round_trips)'` — 1 passed.
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=bifrost::query::tests::resource_terminal_rejects_partial_rows) | test(=bifrost::query::tests::failed_terminal_problem_rebuilds_its_catalog_error)'` — 2 passed.

The serialized Postgres four-node journey was not rerun in this review. The
candidate's immutable task evidence records it passing, and its source was
reviewed end to end. This review also did not repeat the full repository gate;
that is outside this domain slice.

## Material findings

None.

## Overall result

**PASS** — the candidate satisfies the reviewed Oracle/DataFusion/query
transport obligations. No material domain finding requires remediation.
