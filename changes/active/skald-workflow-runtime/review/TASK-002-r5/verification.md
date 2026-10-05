# TASK-002 R5 candidate-bound verification

Candidate `2d669917c03699876b3c8926f0de5ac88c578c01` was checked directly.

| Command | Result |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-semver --lib -E 'test(=block::tests::serde_preserves_the_exact_version_invariant)'` | PASS: 1 selected, 1 passed, 78 skipped |
| Exact repository-managed Postgres TypeScript Workflow journey from the R4 remediation | PASS: 1 file, 1 test; malformed uid/space/name/version and mixed selector assertions exercised before registry authorization |
| `mise run codegen:check` | PASS |
| `mise run ts:typecheck` | PASS |
| `mise run ts:napi:check` | PASS |
| `mise run check:client-tier` | PASS |
| `mise run check:sdk-client-tier` | PASS |
| `git diff --check 0569b79702218600c4f9790f45cc03100d5c6f1c..2d669917c03699876b3c8926f0de5ac88c578c01` | PASS |

The TypeScript command used the task's exact focused form:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/workflow-loading.test.ts -t "^Workflow loading workflow loading journey$"'
```

These R5 runs focus on the final R4 correction and its generated/boundary
effects. They do not independently repeat every broad aggregate recorded in
the original task and remediation evidence. The complete cumulative source,
all prior evidence, and every required independent report remain part of the
acceptance decision; no reviewer identified a proof gap requiring another
lane.
