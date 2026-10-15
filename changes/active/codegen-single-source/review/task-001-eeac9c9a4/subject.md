# Immutable TASK-001 review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/codegen-single-source`.
- Base: `5bcd9e4872f1376f1722922fa63b67384243be04` (parent of first implementation commit `e4210b940`).
- Candidate: `eeac9c9a45b599b1727086ba30582d3f02b3230a`.
- Complete range: seven commits, `git diff 5bcd9e4872f1376f1722922fa63b67384243be04 eeac9c9a45b599b1727086ba30582d3f02b3230a`.
- Spec: `changes/active/codegen-single-source/spec.md`, approved revision 1.
- Task: `changes/active/codegen-single-source/tasks/TASK-001-consolidate-sdk-types-and-codegen.md`.
- Verification evidence: task's appended Execution Evidence (claims to assess against source, not review conclusions). No independent command transcripts supplied.
- Initial tracked working tree was clean. Review writes are confined to this new review directory. Do not edit reviewed source, regenerate into source, commit, or change HEAD.
- No earlier review or remediation was supplied for this task.

## Shared navigation map

This is a starting map, not complete coverage or a conclusion. Reviewers must expand and verify it against source and the complete cumulative diff.

| Surface | Changed owners/symbols | Likely consumers/proof |
| --- | --- | --- |
| Contract extraction/type lowering | `crates/wyrd-spec/examples/gen_schemas.rs`; `scripts/gen_ts_types.py` (`Lowering`, TS rendering, root mapping); deleted `scripts/gen_card_types.py` | `mise.toml` codegen; `wyrd/src/card-types.ts`; public `src/index.ts`; TS type fixtures |
| Python Card boundary | `sdks/wyrd-sdk-python/src/state/mod.rs` (`PyCards::get`, `PyCardEnvelope::aliases`) | cards/state exports/stubs; integration retrieval and workflow-loading tests; support helpers |
| Python authored declarations | package `.pyi`; `scripts/check_stubs.py`; deleted templates/assembler; `pyproject.toml` | new private pure-Python modules in cards/agent/bifrost/gateway/observe/testing.cli; public reexports; py:lints/typecheck/unit |
| Tool schema retention | `_schema.py`, `agent/tool.py`, `crates/skald/skald-prompt/src/python/mod.rs` | `@tool`, Agent.as_tool, workflow examples/tests |
| MCP schemas | `wyrd-server/src/mcp/{gateway,principals}.rs` | descriptor construction/filtering; gateway catalog test; actual request DTO parsers |
| Docs/runtime metadata | docs generators, `llms*.txt`, `diagrams/diagrams.test.ts`, `mise.toml` docs tasks | clean docs generation, enum consistency, publish-docs workflow |
| Build/CI/rules | `.github/scripts/select-ci.py`, workflows/tests, `mise.toml`, AGENTS/agent-rules/references/TESTING | lint task scope, generation reproducibility, package export/native declarations, CI lane selection |
| Retired artifacts/checks | schema JSON/fixture trees, snapshot readers, `_card_types.py`, Python `stubs/`/assembler | distinguish snapshot-only removal from live behavior/assertions |

## Review protocol

Read `.agents/skills/wyrd-task-review/SKILL.md` and the instructions for your assigned role. Read applicable current repository authority via the reference router; inspect original authority where the diff changes it. Use CodeGraph first when indexed. Only report task-required reachable failures or unearned code; reject optional redesign/hardening. Preserve output in your assigned report file. Findings require local ID, classification, violated obligation, exact location, source/caller evidence, observable consequence, and smallest testable correction. Read-only focused checks are allowed, but coordinate any Cargo-backed work with the orchestrator; do not run simultaneous builds/tests or mutate tracked output. Treat evidence claims and tradeoffs as claims, not proof. Do not read sibling reviewer reports during discovery.

Required discovery roles: behavior, invariants, standards, maintainer, system resilience, and SDK contract-domain reviewer. The domain is Rust-to-Python/TypeScript projection correctness and type precision; no authentication, tenancy, concurrency, or persistent-data semantics are changed as an authorized outcome, but flag an actual discovered change for orchestrator domain selection.
