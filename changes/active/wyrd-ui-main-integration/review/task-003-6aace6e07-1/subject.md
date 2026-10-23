# TASK-003 cumulative re-review subject

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
Base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`); immutable candidate `6aace6e07728e59deb1fcd85ce969b2d0a4260db`.
Read the complete `git diff c62ebce8c...6aace6e07` independently. Approved authority is `changes/active/wyrd-ui-main-integration/spec.md` revision 13 and original `tasks/TASK-003-shared-ui-foundation.md`.

Prior packets: task-003-ab1d2499f-1, task-003-c2bbd8e47-1, task-003-a0a5f177f-1, task-003-7579a2f60-1, task-003-39c151769-1 under this change's review directory. Read verdicts, validation and remediation as hypotheses, verify FIND-003-1..12 closure against current source. Latest remediation is `review/task-003-39c151769-1/TASK-003-R5-lossless-budget-admission.md`, commits `0d60b2b68` and `6aace6e07`. PR #205/main (pr205-base `fb1fd9234`) is not a finding. Earlier reviews confirmed rebase preservation. TASK-027 layout commits are separately owned and passed at `39c151769`; inspect shared foundation regression seams without re-owning that task.

## Navigation map (starting point, not coverage or conclusions)

- Portable package `crates/wyrd/wyrd-server/wyrd-ui/packages/ui`: exports, schema/catalog generator, contract, loaders, plot, Node/MCP adapters, view renderer, theme, Chart/CardHeader and primitives.
- Producer/consumer paths: range precedence and first-position duplicate columns; typed series identity and caption folding; exact flat Card reads and shared `format.ts::cardRef` into CardHeader/CardRefChip/actions.
- Runtime: WyrdView loads/actions/supersession -> remote loader -> tenant `/ui/load` BFF -> Rust-selected browser session; SDK and MCP query collection; AppBridge artifact, gallery and custom app.
- Rust/SDK: shared `wyrd-queue/src/json_result.rs` admission/builder -> server `query/collect.rs` and SDK `native/src/lib.rs::NativeBifrostQueryStream::collect`; `parse_ceiling`, existing `parse_deadline_ms`, stream take/drop/close; SDK wrappers/declarations; shared-client query-only browser-session transport; `scripts/build.sh` packaging.
- Proof: UI `src/lib/ui-library.test.ts`, `ui-library.integration.test.ts`, fixtures/setup; queue/server and SDK tests, catalog/build checks, historical per-component human checkpoint.

## Rules and proof limits

No `.codegraph/` exists. Read root and applicable nested AGENTS.md (SDK TS), spec-driven workflow, maintainer style, applicable agent_docs/architecture sections, TESTING.md and UI styling guidance. Compare new workflows to existing owners.

Supplied candidate verification: fmt:check, lints, codegen:check, ts:napi:check, ts:test:unit; cargo clippy -p wyrd-sdk-ts; UI check, test 40/40; real-server integration file 2/2. R5 records RED before rebuilt native addon and GREEN after, refusal/closed-stream/fresh-query assertions. These are reported results, not new reviewer executions. Do not infer renewed cross-platform, deployed browser or human proof.

Write only reports in this packet. No source/test modifications, builds or tests writing outside the packet, commits or pushes. No diff/patch/source archive. Each reviewer owns only its assigned report, is not alone, must not revert others' edits, and must not read peer discovery conclusions before finishing. Check HEAD remains immutable. Read the full skill for role requirements. Orchestrator reconciles; fresh independent validation confirms findings.
