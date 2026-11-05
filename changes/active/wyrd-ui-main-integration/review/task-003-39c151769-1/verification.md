# Verification provenance and limits

Immutable subject and attribution are recorded in [subject.md](subject.md).

## Caller-supplied green results at candidate 39c151769

| Lane | Supplied result |
|---|---|
| `mise run fmt:check` | Green |
| `mise run lints` | Green |
| `mise run codegen:check` | Green |
| `mise run ts:napi:check` | Green |
| `mise run ts:test:unit` | Green |
| cargo nextest for `wyrd-queue` | Green; caller did not supply an exact selector/count |
| UI `pnpm check` | Package and app: zero errors |
| UI `pnpm test` | 40/40 |
| UI `pnpm build` | Green |
| Repository-managed Postgres, `src/lib/ui-library.integration.test.ts` | 2/2 |

The R4 task records focused RED/GREEN, native rebuild, invalid-budget Node/MCP cases, maximum/default success, retained overflow behavior, post-refusal stream closure, canonical copying and focused Rust regressions. Those execution records are historical implementation evidence; independent reviewers assess their assertions against current source rather than treating the implementation summary as proof of acceptance.

## This review

Fresh work consists of read-only Git identity/diff/source/authority inspection and independent specialist reports. No tests or builds are rerun: the caller permits writes only inside the new review directory, and normal test/build commands can update generated or build output elsewhere. Supplied green results are not represented as fresh executions.

The preceding packet already established rebase preservation; this review retains the cumulative range and independently checks current source. Earlier recorded human visual acceptance and Darwin packaging evidence remain historical. New Linux/container release execution, renewed human visual inspection, third-party deployed MCP host compatibility and live degraded-terminal parity are not inferred from current green unit/integration summaries. The task and specification retain their established boundaries with later host/page work.

No source/test file edits, commits or pushes are authorized. Final candidate and workspace checks are recorded in the verdict.
