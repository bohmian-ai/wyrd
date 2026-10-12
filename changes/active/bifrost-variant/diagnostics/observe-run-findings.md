# Codex findings (gpt-5.6-sol, medium, read-only)

1. Cause: [`observe_run.rs:80`](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/sdks/wyrd-sdk-rust/tests/observe_run.rs:80) declares Variant `context` as `String`. The shared decoder emits Variant objects as native JSON values ([`facade.rs:893`](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/crates/shared/wyrd-client/src/bifrost/facade.rs:893)), so Serde receives a map and rejects it. `media` at line 88 is likewise incorrectly typed as `Option<String>` instead of a JSON value. The trace shows both Oracle queries succeeded; only client row conversion failed.

2. Fix site: the read-back DTOs in [`observe_run.rs:78`](/home/thorrester/Documents/GitHub/wyrd-bifrost-variant-task-003/sdks/wyrd-sdk-rust/tests/observe_run.rs:78): use `serde_json::Value` for Variant fields in `EvalRow` and `EvalSpanRow`. Do not stringify Variants or change `QueryResult::deserialize`; that shared owner is behaving according to its contract.

3. Affected callers: `assert_read_back` at line 303 and `assert_eval_joins_span` at line 417. Other `sql_as` Variant callers use the same owner; notably `drift_verification.rs:1659` and `pg_bifrost_e2e.rs:2713` already correctly deserialize Variant columns into `Value`.
