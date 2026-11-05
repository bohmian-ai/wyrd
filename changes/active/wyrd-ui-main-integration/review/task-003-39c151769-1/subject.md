# TASK-003 cumulative re-review subject

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; branch `wyrd-ui-main-integration`.
Candidate: `39c15176955edca785cc578168c67e1efcb80ea5`.
Base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, parent of `ff4145ac6` (rebased original base). Each reviewer reads `git diff c62ebce8c4de0d43a2b9d50d360de628c49fd533...39c151769` independently.
Authority: approved `changes/active/wyrd-ui-main-integration/spec.md`, revision 13; original `tasks/TASK-003-shared-ui-foundation.md`.

Prior rounds: `review/task-003-ab1d2499f-1/`, `review/task-003-c2bbd8e47-1/`, `review/task-003-a0a5f177f-1/`, `review/task-003-7579a2f60-1/` (FIND-003-1..12). Read their verdicts, validation ledgers and remediation tasks as hypotheses; require current-source closure. Latest remediation is `review/task-003-7579a2f60-1/TASK-003-R4-bounded-reads-and-card-references.md`, commits `18e62740f` and `791122315`. Earlier pre-rebase subjects remain reachable through `wyrd-ui-pre-205`; the preceding round confirmed patch preservation. PR #205/main (`fb1fd9234`) is not a TASK-003 finding. TASK-027 commits, including `3302a3e6b` and later narrow WyrdView ordering changes, are separately owned; inspect only foundation regression seams.

## Navigation map — starting point, not exhaustive coverage

- Portable foundation: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/{schemas,contract,loaders,plot,node,mcp,view}.ts`, public exports, catalog generator and theme; all Chart/CardHeader/primitives.
- Chart producer `plot.ts::timeSeries` to Chart and sibling bar/table consumers: typed authored-series identity, display captions, overflow folding, range precedence, duplicate-column binding.
- Flat Card identity: schema -> loadCardHeader exact dependent reads -> CardHeader/CardRefChip/shared `format.ts::cardRef` -> Node/MCP/BFF. Latest correction carries kind/name/version and optional UID into canonical display/copy.
- Runtime: WyrdView loading/actions/supersession -> remoteLoader -> tenant `/ui/load` BFF -> Rust-selected browser session; Node query stream and MCP AppBridge adapter; app artifact/gallery/custom-app example.
- Rust/SDK: `wyrd-queue/src/json_result.rs` positional normalization and new shared ceiling admission -> `wyrd-server/src/query/collect.rs` and TS SDK `native/src/lib.rs`; generated declarations and TS wrappers; shared-client browser-session query-only connection; `scripts/build.sh` native SDK packaging.
- Proof: `src/lib/ui-library.test.ts`, `ui-library.integration.test.ts`, fixtures/setup, Rust queue/server tests, SDK tests, package/build/catalog checks and historical human visual checkpoint.

## Rules, supplied evidence and write constraints

No `.codegraph/` directory exists. Read root AGENTS.md, applicable nested SDK AGENTS.md, spec-driven workflow, maintainer style, applicable agent_docs and architecture sections, TESTING.md and UI styling authority as relevant. Compare new workflows to existing owners, not only leaf reuse.

Caller reports green at this candidate: `mise run fmt:check`, `mise run lints`, `mise run codegen:check`, `mise run ts:napi:check`, `mise run ts:test:unit`, cargo nextest for wyrd-queue, UI pnpm check (0 errors package/app), test (40/40), build, and repository-managed Postgres real-server `ui-library.integration.test.ts` (2/2). These are supplied results, not fresh reviewer executions. Historical Linux/container/native/human evidence must not be represented as renewed proof.

Only this new review packet directory may be written. No source/test changes, builds that write outside the packet, commits or pushes. Reports only: no diff/patch/source archives. Each discovery reviewer owns exactly its assigned report, is not alone in the workspace, must not revert others' changes, and must not read peer discovery reports before independent completion. Check candidate immutability. The orchestrator reconciles reports; independent validation owns finding confirmation.
