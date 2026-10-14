# Verification evidence

Snapshot candidate: `100adbc04a39ca22022fb013b3c0e93bfe903c3a`. All independently rerun commands below exited 0; tracked disk source still matches that snapshot.

| Independent R2 rerun | Result | Log |
|---|---|---|
| `mise run fmt:check` | PASS | `fmt-check.log` |
| `mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib --features schemars -E 'test(=wire::openai_responses::tests::input_items_schema_admits_easy_messages) \| test(=wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages)'` | 2 selected tests passed, 98 skipped | `focused-skald.log` |
| Served `pg_openapi_contract::gateway_ingress_publishes_typed_contracts` | 1 selected test passed, 246 skipped | `focused-boundaries.log` |
| `agents sdk responses tool loop runs through the gateway` | 1 selected Vitest test passed, 8 skipped | `focused-boundaries.log` |
| `caller without gateway invoke cannot run an agents sdk agent` | 1 selected Vitest test passed, 8 skipped | `focused-boundaries.log` |
| `git diff --check` | PASS | Command output empty |

The three server/SDK checks shared one repository-managed Postgres lifecycle. Exact invocation:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_openapi_contract::gateway_ingress_publishes_typed_contracts)" && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "agents sdk responses tool loop runs through the gateway" && mise exec -- pnpm exec vitest run tests/integration/gateway-inference.test.ts -t "caller without gateway invoke cannot run an agents sdk agent"'
```

The frozen `remediation-evidence.md` records implementer final-pass results and exact commands for format, both lint stages, codegen check, Skald family tests, TypeScript/Python typecheck and their formatting/lint lanes. These broader results are supplied evidence, not independently rerun in R2. No assertion, timeout, skip or baseline was changed by review. The new journey was not separately run RED against a rebuilt pre-fix server; that disclosed history is assessed separately from current executable closure. No full aggregate or live-provider lane was run.

The unit schema test pins named reference/definition structure used by generated authoring declarations. The served schema test validates accepted and rejected bodies against the mounted contract after schema conversion. Both results are evidence; their scope is assessed by reviewers, not decided by this supplement.
