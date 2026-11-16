# Review execution evidence

Candidate `798b6b0e83595356c0d2328fa541119267e84fb2`, base `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`.
Commands run by orchestrator; reviewers perform independent source review. No production changes.

| Command | Observed result |
|---|---|
| `git diff --check cd0b8e18b...798b6b0e8` | PASS |
| `mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::approvals_hold_only_their_dependents_until_decided)'` | PASS, 1 test; nextest run f9a4bab8-4fce-4167-a130-5c0b84268e61 |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E "test(/pg_approvals::/)"'` | PASS: both migration checks, then all 5 approval tests; run 51112d9b-e039-45ca-9cc5-094fb4971256; isolated Postgres torn down successfully |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::state::tests::each_approval_occurrence_has_its_own_stable_id)'` | PASS, 1 test; run bfd14f16-cedf-4771-8e42-4b46e18cce9f |
| `mise run check:deps ::: check:tenant-isolation ::: fmt:check` | Dependency check PASS (including security_inline_gate test); tenant check initially unavailable because default uv cache is read-only; runner stopped before fmt |
| `UV_CACHE_DIR=/tmp/wyrd-task-008-review-uv mise run check:tenant-isolation` | PASS using writable cache |
| `mise run fmt:check` | PASS |
| `mise exec -- cargo nextest run --locked -p wyrd-spec -p wyrd-runtime --lib` | PASS, 855 tests; run ecc8e608-8859-4e1c-abc3-1945c69744d7 |

The five Postgres tests cover accepted/rejected decisions and same-execution resumption, restart of the verification runtime while parked, an unacknowledged decision and retry, execution expiry, and supersession racing a decision. Passing these scenarios does not prove omitted negative flows; source reports evaluate the actual assertions.

Execution notes: an initial `mise run fmt:check check:deps check:tenant-isolation` invocation incorrectly forwarded task names as fmt arguments and failed without changing source; corrected commands above passed. The dependency script unexpectedly invoked Cargo and briefly waited for the server unit build's lock; no parallel reviewer Cargo jobs were launched. All later Cargo operations were sequential.

## Limits

No full SDK/CLI/MCP journey lanes, Clippy, Python/TypeScript typing, codegen regeneration, docs build or SQL-wide suite rerun in this review. Their task Evidence entries are implementation claims, distinct from the observed results above. Codegen/docs tasks regenerate tracked artifacts, so they were not invoked under the request to keep source unchanged.

No full clean L1 performance qualification or baseline comparison is claimed. The selected system reviewer assesses hot-path applicability and sanctions for added/moved waits. The review does not generate synthetic host load or waive a required performance gate on a PASS.
