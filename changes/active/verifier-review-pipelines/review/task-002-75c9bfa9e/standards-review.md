# Repository standards review — TASK-002

Overall result: **FAIL**.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
- Base: `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`.
- Candidate: `75c9bfa9e07163099eac40d5a556c4447c6a519d`; HEAD remained this candidate at the final identity check.
- Reviewed the first-review full triple-dot range, all 41 changed files, surrounding production owners, language declarations, fixtures, and materially changed tests/helpers. No prior TASK-002 acceptance is carried forward.
- Applied `.agents/skills/wyrd-task-review/SKILL.md`, repository-standards role only. This report does not decide task acceptance or recommend optional refactors.
- Supplied authority: specification revision 18 plus recorded approved corrections. Current specification frontmatter is revision 21/draft; unrelated wait/approval additions were not treated as TASK-002 requirements. Publication and execution-state/durability supplements supply the applicable contract.
- Packet SHA-256 before and after inspection: spec `5af4975a59d8aaba3702e5c2fde40d34b074760456a293cf61f30cd907a5e683`; original task `7c1773159a2d348d097a46fd5876f748bb32d1569447b102b664657117690bbe`.

CodeGraph was consulted before source navigation, through `mise exec -- codegraph explore`. Follow-up inspection used the actual Git range and source, rather than relying on the implementation summary. No source edits or build/test commands were performed.

## Authority coverage

| Changed surface | Applicable authorities | Coverage |
|---|---|---|
| All changed files | Root `AGENTS.md`; `agent_docs/maintainer-style.md`; `changes/spec-driven-development.md`; `changes/implementation-execution.md`; `TESTING.md` | Immutable diff, execution evidence, ownership, permanent vocabulary, proof tiers, mandatory docs and execution discipline |
| `crates/wyrd-spec/src/card/agent.rs`, `card/verifier/validate.rs` | Root rules; `agent_docs/rust.md`, `errors.md`, `agent-surfaces.md`; `architecture/wyrd-design.md` Agents/tools/workspaces section; task and publication supplement | BuiltinTool variants/catalog/name conversion, workspace predicates, validation and negative tests; IO/PyO3-free contracts |
| `crates/skald/skald-tool/src/trait_.rs`, `skald-agent/src/loop_runtime.rs` | Root tier rules; Rust and maintainer guides; Wyrd Agent ownership and public contract | New invocation seam/default and provider-call dispatch; no dependency/feature changes |
| `crates/vala/vala-verify/src/blocks.rs`, `blocks/{agent,engine,tools}.rs`, `tests/integration/blocks_agent.rs` | Root rules; Rust/errors guides; Wyrd execution/Agent ownership; publication/durability supplement; testing | Attempt propagation, Role, shared tool recording, changed constructors and trait implementations |
| `crates/wyrd/wyrd-server/src/verification/hosted/{blocks,tools,workspace}.rs`, `tools/pg_tests.rs`, `workspace/tests.rs` | Root rules; Rust/errors/agent-surface guides; design Agents/tools/workspaces; security authorization/audit/tenant-isolation sections; Bifrost execution checkpoint and File sections | Composition, argument deserialization, invocation and domain-owner dispatch, writer locking, limits and private scratch, SQL capability composition and changed helpers |
| `wyrd-server/tests/integration/pg_hosted_{artifacts,files}.rs` | Root testing/Rust rules; existing test ownership | All added `attempt` initializers; no removed assertions, weakened budgets or altered target registration |
| `wyrd-mcp/tests/bifrost/mcp/verification.rs` | Root/Rust/errors/agent-surface/testing rules; design MCP and publication contract | Added provider helper/constants and journey; catalog errors and typed publication decoding; existing Postgres test module |
| Rust SDK `tests/integration/{hosted_agent_tools,main,verifier_building_blocks}.rs` | Root/Rust/testing rules, especially `TESTING.md` client-facing checklist; public SDK boundary | New story and seven journeys, all helper bodies, target membership, shared helper visibility and docs |
| Python `python/wyrd/verify/__init__.pyi`, `tests/integration/{conftest,support,test_hosted_agent_tools}.py` | Root; `sdks/wyrd-sdk-python/AGENTS.md`; maintainer guide; client-facing checklist | Authored BuiltinTool literal parity, public imports, top-level integration tests, fixtures/provider scripts, typed result inspection |
| TypeScript `wyrd/src/card-types.ts`, `wyrd/tests/integration/hosted-agent-tools.test.ts` | Root; `sdks/wyrd-sdk-ts/AGENTS.md`; maintainer/agent-surface guides; client-facing checklist | Generated literal union parity, public imports, local provider, declaration usage and journey |
| `architecture/wyrd-design.md`, docs reference and `docs/public/llms-full.txt` | Architecture authority map; design owning section; `docs/AGENTS.md`, `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`; agent-surfaces guide | Tool/workspace/receipt contract, Reference page layout and error tables, corresponding generated text |
| `fixtures/README.md`, all 11 `fixtures/cards/hosted_agent_tools/*.yaml` and `invalid/*.yaml` files | Root Card vocabulary/testing rules; fixture story conventions; design Agent/Verifier/publication contract | Real checked-in Service/Agent/Verifier documents and invalid selection fixtures; no extra Card kinds or generated schema files |

The applicable nested AGENTS files are the Python SDK, TypeScript SDK, and docs files above; no additional nested rules exist in changed Rust directories. SQL sources are not changed, but SQL capability rules were followed through the production owner chain rather than inferred from a green tenant-isolation claim.

## Rule results

| Rule | Source evidence | Result |
|---|---|---|
| Existing tier owns behavior; pure contracts remain IO-, async-, SQL-, and PyO3-free; no unearned dependency/feature | Spec changes are enum/pure validation only; Skald changes the tool hook; Vala supplies attempt/recording context; server composes `Publications`/`RunState`/`Workspace`. No manifest changes. | PASS |
| Durable state and upload lifecycle retain their owners | `tools.rs:349–372` constructs the existing publication adapter against the shared execution state; `blocks.rs:148–181` supplies the same RunState to evidence and tools. Existing `Publications::path/complete/accept` remains the storage/acceptance owner. | PASS |
| Production SQL uses real capabilities, never a raw pool shortcut | New `HostedTools` and `Invocation` fields contain AppState/Caller/RunState, not PgPool. Publication completion uses `state.postgres.wyrd()` (`&WyrdPostgres`) and existing upload/inventory owners; ServerPostgres exposes the existing tenant and operator capabilities. No added SQL, pool parameter, query, coordination timestamp or RLS bypass occurs in the changed range. | PASS for changed scope |
| Runtime identity/permission checks remain server-owned and audit non-blocking | `Invocation::call` retains `audit::authorize(Permission::tool_invoke(tool))` before dispatch; publication producer comes from `BlockCall`/Role, argument structs deny unknown fields, storage remains the existing owner. Detailed domain correctness is outside this standards role. | PASS for ownership/rule placement |
| Portable implementation; no prohibited platform isolation/gating | Workspace write uses portable std/tempfile operations; Unix symlink fixture is test code on the three supported Unix platforms, with no platform-specific feature behavior or `cfg(target_os)` addition. | PASS |
| Public errors/repair data use existing catalog and derive-backed owners | New publication-source error is `WyrdError::Validation` and existing structured conversion; no new handmade catalog/code method or compatibility route. | PASS |
| Every changed Rust item, including test helpers, has complete applicable rustdoc | New public/private items have intent docs, but panic-capable helpers omit mandatory `# Panics`; see SSR-001. | FAIL |
| Imports remain at module top, apart from the narrow documented trait-use exception | New Rust SDK digest has function-local `use base64::Engine as _` at `hosted_agent_tools.rs:684`; new MCP journey also adds function-local non-trait type imports at `verification.rs:3397–3400`. Neither is the documented rare generic-function exception. These are bounded corrections to batch with the affected test edits; no separate refactor is warranted. | FAIL |
| Python package remains a thin typed projection, public tests and authored stubs agree | BuiltinTool's Python Literal gains exactly the two Rust wire variants. No new native wrapper/registration/export is needed for a string-literal union; public package exports are unchanged. New tests import public `wyrd.*` and are top-level functions marked integration. | PASS |
| TypeScript projection remains a thin generated contract | `card-types.ts:416` adds exactly `fs.write` and `publish`; no hand-authored alternate union, N-API binding change or JS durable behavior. Literal parity checked statically; generator execution only claimed in task evidence. | PASS for source parity |
| Generated docs correspond to source and source authority was updated | Changed llms-full content mirrors changed reference prose/tables; source design includes scratch, publish receipt/replay and selection rules. No committed JSON Schema. Generator and docs gate were not rerun. | PASS for static parity |
| SDK journeys obey the public-surface checklist | Lifecycle journey uses internal verification state controls and a fixed sleep; all three SDK stories calculate digests in test code. See SSR-002. Provider fixture serialization and arbitrary JSON publication values are genuinely open boundaries; their mere presence is not a proposed finding. | FAIL |
| Required behavior is proved at the right tier, including negative user flows | The new hosted fs.write byte-budget refusal is asserted only directly against Workspace; the public Agent scripts never exceed this budget. See SSR-004. | FAIL |
| New executable behavior follows recorded RED → GREEN → REFACTOR | The task explicitly records production-first implementation and no expected RED; history corroborates production preceding the public scenario. See SSR-003. | FAIL |
| Tests are registered in existing targets; no gate is bypassed to clear a recorded failure | Rust story is one module in existing integration target; MCP remains in pg_tests. Journey `#[ignore]` is the existing repository-managed service-lane convention explicitly selected by the planned commands, not a response to a failing test. No deleted/weakened assertion, new allow, or timeout edit to clear a documented failure appears. | PASS |
| Docs use current nouns and Reference-page structure; fixtures are user-authored Cards | Exact tool spellings, step-only publish, receipt and failure tables are consistent; fixtures use allowed Card envelopes/kinds, checked-in YAML and fixed names. | PASS |

## Material findings

### SSR-001 — VIOLATION: changed panic-capable Rust helpers lack required panic documentation

**Rule.** Root AGENTS Code requires rustdoc on every changed Rust item and `# Panics` when it can panic. `agent_docs/rust.md` explicitly includes test helpers, regardless of visibility, and the review skill preserves this mandatory gate.

**Locations and evidence.**

- New `sdks/wyrd-sdk-rust/tests/integration/hosted_agent_tools.rs:643–647`, `builder`: URL parsing calls `expect`, but its doc is one sentence with no `# Panics`.
- New same file `reply` at `:270`, indexing the BTreeMap returned by `results` at `:398`: a missing `c-manifest` key panics; no `# Panics` is documented. This is a real fixture path after the scripted consumer receives a missing/failed prior tool response, not an unrelated untouched helper.
- Materially changed `crates/wyrd/wyrd-server/src/verification/hosted/tools/pg_tests.rs:34–46`, `invocation`: adds the new publisher field and constructs the invocation for the changed tool seam; its `expect("slot")`/`expect("workspace materializes")` remain undocumented. This item is in the changed test-helper scope, not a sweep of untouched debt.

**Consequence/materiality.** The candidate fails an explicit mandatory merge gate and hides how new fixtures abort when their setup invariants break. Happy-path test success does not waive it.

**Smallest correction and proof.** Complete these helpers' panic sections with their actual invariants, or remove the panic if the existing local fallback already expresses the intended test behavior. Audit documentation on the same changed items while correcting behavior, not unrelated helpers. Static changed-item inspection closes this finding; no new behavioral test or separate documentation-only round is needed. Batch the module-scope import corrections listed above with edits to these new test files.

### SSR-002 — VIOLATION: SDK proofs cross the repository's permitted test boundary

**Rule.** `TESTING.md:330–339` restricts SDK journeys to public surfaces and named narrow controls, explicitly excluding sleep and digest computation. `TESTING.md:279–289` says internal or workaround-driven behavior belongs at the owning internal tier.

**Locations and evidence.** New `sdks/wyrd-sdk-rust/tests/integration/hosted_agent_tools.rs:1445` waits five wall-clock seconds to infer absence of later work. At `:1467–1480` the same journey calls `server.flush_bifrost()`, obtains `server.state()`, provisions `wyrd_testing::verification::VerificationFixture` and mutates delivery deadlines. These controls are outside the listed public SDK controls. The runtime proof reaches into queue/engine state to make recovery occur. Separately, new digest helpers perform the expressly prohibited calculation in Rust `:683–685`, Python `test_hosted_agent_tools.py:99–101`, and TS `hosted-agent-tools.test.ts:236–238` (called by each public download assertion).

**Consequence/materiality.** The lifecycle proof can pass under privileged fixture manipulation and a timing window that normal users cannot exercise; it is not a conforming SDK example/proof under the explicit checklist. This is material proof-boundary drift, not a preference about helper arrangement. The production recovery behavior may still be correct; this finding does not claim otherwise.

**Smallest correction and proof.** Keep ordinary SDK submit/verify/readback assertions at the public boundary. Prove internal crash/lease/flush coordination at the existing server-owned Postgres integration tier, which can use these dependencies, and replace the fixed sleep proof with deterministic owner-level evidence that observes cancellation/completion and attempts. Reuse the existing fixture/server owner; do not introduce another runtime or general testing framework. For fixed publication bytes, compare plans to fixture-owned expected digest values, or leave digest-algorithm assertions in the owning server/storage tier; retain SDK assertions of returned typed plan/size and immutable content. Re-run only the moved/changed focused tests and affected SDK journey. Do not delete recovery or disconnect obligations.

**Boundary calibration.** This finding does not reject serialization in local mock-provider scripts: those scripts necessarily produce/consume provider wire JSON. Nor does it reject reading arbitrary publication JSON as a public `value`. The identified violations are the concrete non-public lifecycle controls, fixed sleep, and test-side digest calculation.

### SSR-003 — VIOLATION: required RED evidence is explicitly absent

**Rule.** `changes/spec-driven-development.md` TDD execution and `changes/implementation-execution.md` implementation/test integrity require expected missing-behavior RED before minimum implementation, followed by GREEN and the relevant regression checks. Original task Scenario 1 specifically selects the public publication journey for RED; selector selection alone is not RED.

**Location/evidence.** Original task `TASK-002-agent-tooling.md:215–217` explicitly states that production commit `973b78e5e` was written before the journeys and that they were then written against it. Git range starts with that production implementation; `b6f8c39cf` subsequently introduces the public publication journey. No expected missing-behavior RED is supplied. Existing unit/assertion edits in the first commit do not establish the required public-scenario RED.

**Consequence/materiality.** The declared execution discipline and required evidence gate are unmet. Green tests establish current outcomes but cannot establish that the mapped public proof caught the absent behavior before implementation.

**Correction boundary.** This is a historical execution violation, distinct from a current code defect. Do not rewrite history, fabricate RED evidence, or add production churn. Preserve the truthful deviation for the orchestrator/maintainer to disposition; ordinary code remediation cannot retroactively restore TDD order. For actual behavioral corrections discovered in this review, execute and record genuine focused RED/GREEN proof before the correction. A post-hoc base regression demonstration may strengthen test credibility, but must not be described as restoring the original sequence.

### SSR-004 — MISSING: new fs.write budget refusal lacks public-path negative proof

**Rule.** Root AGENTS Testing requires journey edge/negative flows; `TESTING.md:15–19` explicitly says a user-observable negative flow proved only by a unit test is a coverage gap. Task Scenario 1/Proof 1 calls for excessive-byte refusal, including safe overwrite under shared byte limits.

**Locations/evidence.** `Workspace::write`, `workspace.rs:1097–1108`, enforces the new work-budget calculation. `workspace/tests.rs:656` and following test it through direct `Workspace::write`, using 16-byte and 4-byte limits. The public `reply` branches for scratch and limits (`hosted_agent_tools.rs:270–465`) generate successful writes and an oversized JSON *publication*, but no fs.write call exceeding the shared `/work` budget. `tool_limits_release_capacity_and_preserve_provenance` (`:1335`) asserts `publication_size`, not `work_budget`. The task itself confirms at `:226–227` that over-budget writes and the `/work` budget are unit-only.

**Consequence/materiality.** No public Agent-loop proof connects the new limit enforcement to typed refusal/recording and unchanged prior content. A dispatch/locking/budget wiring regression could leave the direct Workspace unit green. This concerns the explicitly required bound on an authenticated filesystem-writing capability, rather than exhaustive exotic input coverage.

**Smallest correction and proof.** Add one narrow public Agent-loop negative case through the existing hosted-tools story and server configuration: exceed a bounded cumulative work budget, then read back the prior accepted bytes and assert the exact refusal and recorded failure. Exercise an overwrite boundary if needed to distinguish cumulative from per-file enforcement. Reuse the existing fixture and owner; no new harness or broad suite is justified. Follow the actual focused journey selector with the relevant owner unit test only.

## Verification and limits

The task claims seven Rust journeys, Python two cases, TS one case, MCP one case, format/lints, codegen/dependency/tenant gates, Vala 60/60, and Python/TS checks passed. These are available verification claims, not independently observed executions by this reviewer. Source was inspected to determine what those tests assert. The parent reports the full-range `git diff --check` clean. No new test execution was required for the static rule findings above; Cargo-backed checks were not run concurrently with other agents.

No recorded `docs:check` or `ts:napi:check` result appears in the supplied task evidence. `codegen:check` is not the docs gate (`mise.toml:1224` runs generation, drift, docs unit/link/build/a11y checks) or N-API declaration gate (`mise.toml:1452`). This is an evidence limit, not an assertion that either command fails. Generated parity was checked statically; source immutability prevented rerunning mutating generators here.

No optional refactors are proposed. Past TDD order is separated from testable source corrections; mandatory rustdoc/import corrections should accompany the bounded behavior/proof remediation. The verdict remains **FAIL** for the explicit standards gaps above.
