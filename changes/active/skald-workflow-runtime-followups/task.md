---
id: TASK-001
kind: remediation
status: ready
parent_change: skald-workflow-runtime
base: ac19e6bbe9c975a1e2f64b22cc80a0b66093098e
---

# Close skald-workflow-runtime review follow-ups

Implementation skill: `$wyrd-implement`.

The `skald-workflow-runtime` change is complete
(`changes/completed/2026/skald-workflow-runtime.md`). Its final reviews left
twelve non-critical findings. The human directed that every one is addressed.
The full finding text, evidence, and correction boundaries are in Git history
at `395a3ad91` (read with `git show 395a3ad91:<path>`):

- `changes/active/skald-workflow-runtime/review/TASK-004-r6/findings-validation.md`
- `changes/active/skald-workflow-runtime/review/TASK-005-r1/findings-validation.md`
- `changes/active/skald-workflow-runtime/review/change-r1/review.md` (at `079794988`)

## Human decision

`wyrd workflow` commands take no `--server` option. The Wyrd endpoint is
inferred from the environment/configuration only, exactly as the existing
ambient client resolution does. The global CLI `--server` flag used by other
commands is unchanged.

## Required corrections

| # | Finding | Correction |
|---|---|---|
| 1 | `FIND-TASK-004-25` cache key ignores the Prompt destination | If `CacheKey::from_request` has no caller outside its own tests, delete it and its tests. Otherwise derive the key from the effective `Prompt::provider()`. Do not keep a request-only variant. |
| 2 | `FIND-TASK-004-24` unsupported-media error names Google for Vertex | Use `Prompt::provider()` at the existing media-error owner. |
| 3 | `FIND-TASK-004-17` task authority names Revision 13 | Closed by completion: the active task files were removed. Record that in the completed record; no other change. |
| 4 | `FIND-TASK-004-26` two rustdoc blocks conflate schema default and destination | Correct the two rustdoc blocks. |
| 5 | `FIND-TASK-004-27` four Prompt construction items lack `# Errors` | Add substantive rustdoc with `# Errors`. |
| 6 | `FIND-TASK-005-1` `--file` refuses `--server` | Apply the human decision: delete `--server` from every `wyrd workflow` subcommand and its parser refusal; the endpoint comes from the ambient client. Update the CLI tests and docs that used it. |
| 7 | `FIND-TASK-005-2` input file read before mode refusal | Validate mode compatibility before reading `--input-file`. |
| 8 | `FIND-TASK-005-3` function-local `PermissionsExt` imports | Move them to the module import blocks. |
| 9 | `FIND-TASK-005-4` Ctrl-C listener failure treated as interruption | Handle the `ctrl_c()` result in the existing select arm; project failure through the existing typed error. |
| 10 | `CHANGE-R1-DRIFT-001` external-gateway success-response credential scan | Delete the substring scan in `skald-providers/src/clients/external.rs` and its tests. Keep the standard secret handling and error/log redaction. Add nothing in its place. |
| 11 | `CHANGE-R1-INCORRECT-001` TypeScript example uses `new Cards()` | Change the example to `Cards.connect()`. |
| 12 | `CHANGE-R1-MISSING-001` OpenAPI test does not assert Workflow routes | Extend `crates/wyrd/wyrd-server/tests/pg_openapi_contract.rs` to assert the `/v1/workflow-runs*` paths in the served document. |

Finally, update the "Approved revisions and deviations" section of
`changes/completed/2026/skald-workflow-runtime.md` so it no longer lists the
closed deviations, and records the `wyrd workflow` no-`--server` decision.

## Verification

Run the narrowest lanes that cover the write set, plus `mise run fmt`,
`mise run lints`, `git diff --check`, and every focused test you change or add
by its exact command. Expected owners: `test:skald`, the three compiled CLI
workflow journeys, `test:principals:integration` for OpenAPI, `docs:check` if
docs change, and the TypeScript lane if the example is type-checked.

## Evidence

Append the acceptance table below.

| # | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 | `CacheKey::from_request` had no non-test caller; deleted with its helpers, tests, the now-unproducible `SkaldCacheError`, the `SkaldRuntimeError::Cache` wrapper, and unused `sha2`/`hex`/`thiserror` deps (`skald-cache/src/key.rs`, `lib.rs`, `error.rs` removed, `skald-runtime/src/error.rs`) | `mise run test:skald` 332/332; `cargo hakari generate --diff` and `manage-deps --dry-run` clean | PASS |
| 2 | `Prompt::bind_media_mut` passes `self.provider()` to the GenerateContent media helper (`skald-spec/src/prompt.rs`) | `mise exec -- cargo nextest run --locked -p skald-spec --lib -E 'test(=prompt::prompt_media::google_media_matrix_and_rejections)'` (new Vertex case asserts `vertex`, Google case unchanged) | PASS |
| 3 | Closed by completion; recorded in `changes/completed/2026/skald-workflow-runtime.md` | Source inspection | PASS |
| 4 | `ProviderResponse::provider` documented as the schema default (`skald-spec/src/response.rs`); `invoke_agent_span` documented as the effective dispatch target (`skald-agent/src/loop_runtime.rs`) | `mise run lints` | PASS |
| 5 | Rustdoc with `# Errors` on `Prompt::new`, `vertex`, `google_prompt`, `finalize_prompt` (incl. destination save/restore) | `mise run lints` | PASS |
| 6 | `server` removed from `RunArgs`/`RunIdArgs`; `from_global(None)` everywhere (`wyrd-cli/src/workflow.rs`); usage case now proves `--server` is rejected; docs (`how-to/build-a-workflow.svx`) and `architecture/wyrd-design.md` updated | CLI command below (4/4); `mise run docs:check` | PASS |
| 7 | `RunArgs::run` reads input only in valid arms; contract test adds `--execution server --file` and local `--detach`, both with a missing `--input-file` | `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_CLI_E2E=1 mise exec -- cargo nextest run --locked -p wyrd-cli --test cli --run-ignored all -E "test(=workflow_journey::workflow_cli_contract) \| test(=workflow_journey::workflow_file_apply_registered_local) \| test(=workflow_journey::workflow_server_detach_status_cancel) \| test(=workflow_journey::workflow_registered_route_protocol_matrix)"'` 4/4 | PASS |
| 8 | `#[cfg(unix)] use std::os::unix::fs::PermissionsExt as _;` moved to module imports in `wyrd-cli/tests/workflow_journey.rs` and `sdks/wyrd-sdk-rust/tests/workflow_loading.rs` | CLI journeys above; `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test workflow_loading --run-ignored all -E "test(=workflow_loading_journey)"'` 1/1 | PASS |
| 9 | `ctrl_c()` result matched in the select arm; `Err` maps to `WyrdError::WorkflowInternal` (boundary `signal_listener`) through `WyrdCliError::Server` | `workflow_server_detach_status_cancel` (SIGINT path, exit 130) above; `mise run lints` | PASS |
| 10 | `reflects_credential`, `contains_any`, `REFLECTED_CREDENTIAL` deleted (`skald-providers/src/clients/external.rs`); reflection test replaced by `external_gateway_decode_detail_withheld`, which keeps the existing decode-redaction case | `mise exec -- cargo nextest run --locked -p skald-workflow --lib -E 'test(=workflow::tests::external_gateway_decode_detail_withheld)'`; `mise run test:skald` | PASS |
| 11 | The `new Cards()` example existed only in the removed active `spec.md`; there is no in-tree TypeScript example to change. Recorded as closed by completion | `git grep "new Cards("` finds only the SDK's internal constructor | PASS |
| 12 | `the_served_document_describes_the_composed_surface` asserts `post /v1/workflow-runs`, `get /v1/workflow-runs/{run_id}`, `post /v1/workflow-runs/{run_id}/cancel` | `mise run test:principals:integration` (pg_openapi_contract 20/20) | PASS |
| Record | Deviations replaced with the no-`--server` decision and closure note; acceptance rows updated | Source inspection | PASS |
| Hygiene | — | `mise run fmt`, `mise run lints`, `git diff --check` all exit 0; TypeScript lane not run (no TS file changed) | PASS |
