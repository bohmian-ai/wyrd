# Verification supplement

Exact candidate: `6294bf6bbc0acb9abc37e4e4d617e2638f9dc3f7`. Reviewed source remains unchanged.

| Command | Result | Evidence |
|---|---|---|
| `mise run fmt:check` | PASS, exit 0; read-only equivalent of format gate | `fmt-check.log` |
| `mise exec -- cargo nextest run --locked -p wyrd-skald-spec --lib -E 'test(=wire::openai_responses::tests::easy_input_messages_decode_as_typed_messages)'` | PASS; 1 selected test passed, 98 skipped | `focused-test.log` |
| `mise run test:skald` | PASS; 345 tests passed, zero skipped | `skald-test.log` |
| `mise run lints` | PASS, exit 0; workspace all-features/all-targets and separate shipped-server check | `lints.log` |
| `mise run codegen:check` | PASS, exit 0 in disposable exact-HEAD git-archive source snapshot | `codegen-check.log` |

The codegen snapshot lives outside the worktree; it is a source copy, not another Git worktree. This permits the generator-based check without mutating the immutable subject. The final check also passed five UI generator tests and token parity. Logs may mention import fixes within the snapshot, but the task's generated-artifact comparison passed.

No new Agents SDK gateway journey exists to execute. Existing full gateway journeys, Python unit/typecheck, and TypeScript typecheck were not rerun in this acceptance audit. Discovery reports submitted while lints/codegen were pending should be read together with this final supplement; pending evidence alone is not an implementation finding.
