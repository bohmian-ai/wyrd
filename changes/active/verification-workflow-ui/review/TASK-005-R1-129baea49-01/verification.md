# Current-candidate remediation verification

Subject: `147c235fd19dd4a400da1515a908db0d5521477d` → `129baea4983d5b9444993503938e3641f760670b`.
Initial working tree clean. Ancestry check confirms the prior candidate is an ancestor; no rebased evidence correspondence needed. Approved spec blobs unchanged: verification-workflow-ui rev8 `e736e3463f8862b7ae0a34dcca9274e23c00dcfa`, authorization-model rev9 `55689bae96c62e78b1ea8d765a32629664b4e603`.

- `mise exec -- git diff --check 147c235fd 129baea49`: PASS.
- All requested fresh checks completed; detailed results follow.

Remediation evidence in the original R1 task includes component RED/GREEN, a focused three-test real-server run, mutation strength, the signoff-fixture diagnosis, full UI checks/build and codegen. The independent reviewer assesses the actual assertions and applicability, not the handoff summary alone.

## Executed at current candidate

- `mise run ui:check`: PASS, 115 tests across 11 files; package and host svelte-check each report 0 errors/0 warnings. Rebuilt shared library and MCP view artifact.
- `mise run codegen:check`: PASS, 7/7 brand checks, generated contracts/theme/catalog in sync.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.test.ts -t 'stays one selectable entry with every judgment|stays readable from its recorded invocation'`: PASS, 2 selected tests, 47 unselected.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`: PASS, shared artifact and production host build.
- `mise run ts:build:testing`: PASS. `mise run build:server`: PASS (`wyrd-server 0.4.2`).

## Focused real-server result

`mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t 'a requirement judged while its execution waits|member whose Repository policy and Verifier Card were deleted|a reader of the subject but not the Verifier'`: PASS, 3 selected tests, 20 unselected; 10.97 seconds. Fresh shared artifact build, real repository-managed Postgres and server. Covers new staged-approval checkpoint progress and existing deleted-definition and retained-output paths.

No full integration or broad journey aggregate, benchmark, new browser capture or mutation rerun was performed by root during this review. The prior task records mutation and RED evidence; the independent reviewer assesses those claims separately.

## Final subject check

HEAD remains `129baea4983d5b9444993503938e3641f760670b`; tracked diff is empty and git status shows only the new review directory. No verification process remains running. No production changes, commits or pushes were made by review.
