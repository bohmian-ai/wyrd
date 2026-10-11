# Arrow 60 verify failures — diagnoses

Failures from `mise run verify:bifrost` after the Arrow 60 upgrade. Each was
diagnosed by an independent read-only `codex exec` agent (`gpt-5.6-sol`,
medium) given only the command, trace, and diff. `*-prompt.md` is the
question; `*-findings.md` is the agent's final answer.

| Failure | Cause | Fix site | Proof |
|---|---|---|---|
| Oracle `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`, `published::variant_sql_registry_covers_every_session` | `seed_foreign_hot_row` hard-coded `Timestamp(µs, "UTC")`; managed columns are `"+00:00"` since `2afc97da2` (`tables/managed_columns.rs:48`) | Fixture: array types follow the schema (`wyrd-testing/tests/bifrost/oracle/support.rs`) | 2/2 pass |
| `pg_grpc_ingest_smoke` `system_writer_alone_writes_verification_results`, `system_result_writes_require_the_exact_signed_verifier_scope` | Fixture still wrote `details`, removed in `2227ba577` (`tables/verification/results.rs:85`); server correctly refuses undeclared fields | Fixture: built from `ResultsTable::arrow_fields()` (`wyrd-server/tests/pg_grpc_ingest_smoke.rs`) | 12/12 pass |
| `wyrd-sdk-rust::observe_run scoped_run_emits_drift_eval_and_generic_rows` | Read-back DTOs typed Variant `context`/`media` as `String`; the shared decoder returns Variants as JSON values by contract (`wyrd-client/src/bifrost/facade.rs:893`) | Test DTOs use `serde_json::Value` (`sdks/wyrd-sdk-rust/tests/observe_run.rs`) | 3/3 pass |

Reviews: `fixture-fixes-findings.md` approves both fixture fixes and finds no
weakened assertion. `observe-run-findings.md` places the fix in the test DTOs,
not in `QueryResult::deserialize`.
